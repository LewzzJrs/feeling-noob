use crate::plan::RenderPlan;
use crate::processor::{self, EngineHandle};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, ErrorKind, SampleFormat};
use rtrb::{Consumer, RingBuffer};
use std::fmt;

const ERROR_CAPACITY: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug)]
pub enum DeviceError {
    NoDevice,
    NotFound(String),
    UnsupportedFormat(SampleFormat),
    Backend(cpal::Error),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => write!(f, "no audio output device found"),
            Self::NotFound(id) => write!(f, "audio output device not found: {id}"),
            Self::UnsupportedFormat(format) => {
                write!(
                    f,
                    "device sample format {format} is not supported (need f32)"
                )
            }
            Self::Backend(e) => write!(f, "audio device error: {e}"),
        }
    }
}

impl std::error::Error for DeviceError {}

impl From<cpal::Error> for DeviceError {
    fn from(e: cpal::Error) -> Self {
        Self::Backend(e)
    }
}

fn device_name(device: &Device) -> String {
    device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| device.to_string())
}

pub fn list_output_devices() -> Result<Vec<OutputDeviceInfo>, DeviceError> {
    let host = cpal::default_host();
    let devices = host
        .output_devices()?
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            Some(OutputDeviceInfo {
                id,
                name: device_name(&device),
            })
        })
        .collect();
    Ok(devices)
}

fn find_device(device_id: Option<&str>) -> Result<Device, DeviceError> {
    let host = cpal::default_host();
    match device_id {
        None => host.default_output_device().ok_or(DeviceError::NoDevice),
        Some(id) => {
            let parsed = id
                .parse()
                .map_err(|_| DeviceError::NotFound(id.to_owned()))?;
            host.device_by_id(&parsed)
                .ok_or_else(|| DeviceError::NotFound(id.to_owned()))
        }
    }
}

pub struct AudioOutput {
    _stream: cpal::Stream,
    errors: Consumer<ErrorKind>,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioOutput {
    pub fn take_error(&mut self) -> Option<ErrorKind> {
        self.errors.pop().ok()
    }
}

pub fn open_output(
    device_id: Option<&str>,
    plan: impl FnOnce(u32) -> RenderPlan,
) -> Result<(AudioOutput, EngineHandle), DeviceError> {
    let device = find_device(device_id)?;
    let supported = device.default_output_config()?;
    if supported.sample_format() != SampleFormat::F32 {
        return Err(DeviceError::UnsupportedFormat(supported.sample_format()));
    }
    let config = supported.config();
    let (mut processor, handle) = processor::create(
        config.sample_rate,
        config.channels,
        Box::new(plan(config.sample_rate)),
    );
    let (mut error_tx, error_rx) = RingBuffer::new(ERROR_CAPACITY);
    let stream = device.build_output_stream(
        config,
        move |data: &mut [f32], _| assert_no_alloc::assert_no_alloc(|| processor.process(data)),
        move |err: cpal::Error| {
            let _ = error_tx.push(err.kind());
        },
        None,
    )?;
    stream.play()?;
    let output = AudioOutput {
        _stream: stream,
        errors: error_rx,
        device_name: device_name(&device),
        sample_rate: config.sample_rate,
        channels: config.channels,
    };
    Ok((output, handle))
}
