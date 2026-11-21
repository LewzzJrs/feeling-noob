use crate::plan::RenderPlan;
use crate::processor::{self, EngineHandle, audio_channel};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, ErrorKind, SampleFormat, StreamConfig, SupportedBufferSize,
    SupportedStreamConfig,
};
use rtrb::{Consumer, RingBuffer};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const ERROR_CAPACITY: usize = 16;
const INPUT_RING_SECONDS: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

pub type OutputDeviceInfo = DeviceInfo;

#[derive(Debug)]
pub enum DeviceError {
    NoDevice,
    NotFound(String),
    UnsupportedFormat(SampleFormat),
    SampleRate { input: u32, output: u32 },
    Backend(cpal::Error),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => write!(f, "no audio device found"),
            Self::NotFound(id) => write!(f, "audio device not found: {id}"),
            Self::UnsupportedFormat(format) => {
                write!(
                    f,
                    "device sample format {format} is not supported (need f32)"
                )
            }
            Self::SampleRate { input, output } => write!(
                f,
                "the input runs at {input} Hz but the output runs at {output} Hz"
            ),
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

fn describe(devices: impl Iterator<Item = Device>) -> Vec<DeviceInfo> {
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            Some(DeviceInfo {
                id,
                name: device_name(&device),
            })
        })
        .collect()
}

pub fn list_output_devices() -> Result<Vec<DeviceInfo>, DeviceError> {
    Ok(describe(cpal::default_host().output_devices()?))
}

pub fn list_input_devices() -> Result<Vec<DeviceInfo>, DeviceError> {
    Ok(describe(cpal::default_host().input_devices()?))
}

fn find_device(device_id: Option<&str>, input: bool) -> Result<Device, DeviceError> {
    let host = cpal::default_host();
    match device_id {
        None if input => host.default_input_device().ok_or(DeviceError::NoDevice),
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

fn stream_config(supported: &SupportedStreamConfig, buffer: Option<u32>) -> StreamConfig {
    let mut config = supported.config();
    if let Some(frames) = buffer
        && let SupportedBufferSize::Range { min, max } = *supported.buffer_size()
        && (min..=max).contains(&frames)
    {
        config.buffer_size = BufferSize::Fixed(frames);
    }
    config
}

fn to_frames(duration: Duration, sample_rate: u32) -> u64 {
    (duration.as_secs_f64() * f64::from(sample_rate)).round() as u64
}

pub struct AudioOutput {
    _stream: cpal::Stream,
    errors: Consumer<ErrorKind>,
    latency: Arc<AtomicU64>,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioOutput {
    pub fn take_error(&mut self) -> Option<ErrorKind> {
        self.errors.pop().ok()
    }

    pub fn latency_frames(&self) -> u64 {
        self.latency.load(Ordering::Relaxed)
    }
}

pub fn open_output(
    device_id: Option<&str>,
    buffer: Option<u32>,
    plan: impl FnOnce(u32) -> RenderPlan,
) -> Result<(AudioOutput, EngineHandle), DeviceError> {
    let device = find_device(device_id, false)?;
    let supported = device.default_output_config()?;
    if supported.sample_format() != SampleFormat::F32 {
        return Err(DeviceError::UnsupportedFormat(supported.sample_format()));
    }
    let config = stream_config(&supported, buffer);
    let rate = config.sample_rate;
    let (mut processor, handle) = processor::create(rate, config.channels, Box::new(plan(rate)));
    let (mut error_tx, error_rx) = RingBuffer::new(ERROR_CAPACITY);
    let latency = Arc::new(AtomicU64::new(0));
    let latency_out = Arc::clone(&latency);
    let stream = device.build_output_stream(
        config,
        move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
            assert_no_alloc::assert_no_alloc(|| {
                let ts = info.timestamp();
                let ahead = ts.playback.duration_since(ts.callback);
                latency_out.store(to_frames(ahead, rate), Ordering::Relaxed);
                processor.process(data);
            })
        },
        move |err: cpal::Error| {
            let _ = error_tx.push(err.kind());
        },
        None,
    )?;
    stream.play()?;
    let output = AudioOutput {
        _stream: stream,
        errors: error_rx,
        latency,
        device_name: device_name(&device),
        sample_rate: rate,
        channels: config.channels,
    };
    Ok((output, handle))
}

pub struct AudioInputStream {
    _stream: cpal::Stream,
    errors: Consumer<ErrorKind>,
    latency: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioInputStream {
    pub fn take_error(&mut self) -> Option<ErrorKind> {
        self.errors.pop().ok()
    }

    pub fn latency_frames(&self) -> u64 {
        self.latency.load(Ordering::Relaxed)
    }

    pub fn dropped_samples(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

pub fn open_input(
    device_id: Option<&str>,
    buffer: Option<u32>,
    output_rate: u32,
) -> Result<(AudioInputStream, Consumer<f32>), DeviceError> {
    let device = find_device(device_id, true)?;
    let supported = device.default_input_config()?;
    if supported.sample_format() != SampleFormat::F32 {
        return Err(DeviceError::UnsupportedFormat(supported.sample_format()));
    }
    let mut config = stream_config(&supported, buffer);
    let native_rate = config.sample_rate;
    config.sample_rate = output_rate;
    let rate = output_rate;
    let width = usize::from(config.channels.max(1));
    let (mut samples_tx, samples_rx) = audio_channel(rate as usize * width * INPUT_RING_SECONDS);
    let (mut error_tx, error_rx) = RingBuffer::new(ERROR_CAPACITY);
    let latency = Arc::new(AtomicU64::new(0));
    let dropped = Arc::new(AtomicU64::new(0));
    let (latency_in, dropped_in) = (Arc::clone(&latency), Arc::clone(&dropped));
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], info: &cpal::InputCallbackInfo| {
            assert_no_alloc::assert_no_alloc(|| {
                let ts = info.timestamp();
                let behind = ts.callback.duration_since(ts.capture);
                latency_in.store(to_frames(behind, rate), Ordering::Relaxed);
                for &sample in data {
                    if samples_tx.push(sample).is_err() {
                        dropped_in.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        },
        move |err: cpal::Error| {
            let _ = error_tx.push(err.kind());
        },
        None,
    );
    let stream = match stream {
        Ok(stream) => stream,
        Err(_) if native_rate != output_rate => {
            return Err(DeviceError::SampleRate {
                input: native_rate,
                output: output_rate,
            });
        }
        Err(e) => return Err(e.into()),
    };
    stream.play()?;
    let input = AudioInputStream {
        _stream: stream,
        errors: error_rx,
        latency,
        dropped,
        device_name: device_name(&device),
        sample_rate: rate,
        channels: config.channels,
    };
    Ok((input, samples_rx))
}
