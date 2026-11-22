use crate::audio_data::wav_spec;
use crate::processor::audio_channel;
use fnoob_core::settings::RecordFormat;
use rtrb::{Consumer, Producer};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

const RING_SECONDS: usize = 4;
const IDLE_SLEEP: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordTarget {
    pub path: PathBuf,
    pub first_channel: u16,
    pub channels: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedFile {
    pub path: PathBuf,
    pub channels: u16,
    pub frames: u64,
}

pub(crate) type Writer = hound::WavWriter<BufWriter<File>>;

pub struct WavRecorder {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Result<Vec<RecordedFile>, String>>,
}

pub fn start_recording(
    input_channels: u16,
    sample_rate: u32,
    format: RecordFormat,
    targets: Vec<RecordTarget>,
) -> Result<(WavRecorder, Producer<f32>), String> {
    let width = usize::from(input_channels.max(1));
    let mut writers = Vec::with_capacity(targets.len());
    for target in &targets {
        if target.first_channel + target.channels > input_channels {
            return Err(format!(
                "input channel {} is not available on this device",
                target.first_channel + target.channels
            ));
        }
        if let Some(dir) = target.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let writer =
            hound::WavWriter::create(&target.path, wav_spec(target.channels, sample_rate, format))
                .map_err(|e| format!("cannot create {}: {e}", target.path.display()))?;
        writers.push(writer);
    }
    let (producer, consumer) = audio_channel(sample_rate as usize * width * RING_SECONDS);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = std::thread::Builder::new()
        .name("fnoob-recorder".into())
        .spawn(move || write_loop(consumer, width, format, targets, writers, &thread_stop))
        .map_err(|e| e.to_string())?;
    Ok((WavRecorder { stop, handle }, producer))
}

pub(crate) fn write_sample(
    writer: &mut Writer,
    format: RecordFormat,
    sample: f32,
) -> hound::Result<()> {
    let sample = if sample.is_finite() { sample } else { 0.0 };
    match format {
        RecordFormat::Float32 => writer.write_sample(sample),
        RecordFormat::Int16 => {
            writer.write_sample((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)
        }
        RecordFormat::Int24 => {
            writer.write_sample((sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)
        }
    }
}

fn write_loop(
    mut consumer: Consumer<f32>,
    width: usize,
    format: RecordFormat,
    targets: Vec<RecordTarget>,
    mut writers: Vec<Writer>,
    stop: &AtomicBool,
) -> Result<Vec<RecordedFile>, String> {
    let mut frame = vec![0.0f32; width];
    let mut frames = 0u64;
    loop {
        let stopping = stop.load(Ordering::Acquire);
        let mut wrote = false;
        while consumer.slots() >= width {
            for sample in frame.iter_mut() {
                *sample = consumer.pop().unwrap_or(0.0);
            }
            for (target, writer) in targets.iter().zip(writers.iter_mut()) {
                let first = usize::from(target.first_channel);
                for &sample in &frame[first..first + usize::from(target.channels)] {
                    write_sample(writer, format, sample).map_err(|e| e.to_string())?;
                }
            }
            frames += 1;
            wrote = true;
        }
        if stopping {
            break;
        }
        if !wrote {
            std::thread::sleep(IDLE_SLEEP);
        }
    }
    let mut results = Vec::with_capacity(targets.len());
    for (target, writer) in targets.into_iter().zip(writers) {
        writer.finalize().map_err(|e| e.to_string())?;
        results.push(RecordedFile {
            path: target.path,
            channels: target.channels,
            frames,
        });
    }
    Ok(results)
}

impl WavRecorder {
    pub fn finish(self) -> Result<Vec<RecordedFile>, String> {
        self.stop.store(true, Ordering::Release);
        self.handle
            .join()
            .map_err(|_| "the recorder thread crashed".to_owned())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_data::load_wav;

    #[test]
    fn writes_selected_channels_to_separate_files() {
        let dir = std::env::temp_dir()
            .join("fnoob-engine-tests")
            .join(format!("record-{}", std::process::id()));
        let targets = vec![
            RecordTarget {
                path: dir.join("audio").join("left.wav"),
                first_channel: 0,
                channels: 1,
            },
            RecordTarget {
                path: dir.join("audio").join("both.wav"),
                first_channel: 0,
                channels: 2,
            },
        ];
        let (recorder, mut producer) =
            start_recording(2, 48_000, RecordFormat::Float32, targets).unwrap();
        for i in 0..1000 {
            producer.push(i as f32 / 1000.0).unwrap();
            producer.push(-(i as f32) / 1000.0).unwrap();
        }
        let files = recorder.finish().unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].frames, 1000);
        let left = load_wav(&files[0].path, 48_000).unwrap();
        assert_eq!(left.channels, 1);
        assert_eq!(left.samples[10], 0.01);
        let both = load_wav(&files[1].path, 48_000).unwrap();
        assert_eq!(both.frame(10), (0.01, -0.01));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_channels_the_device_does_not_have() {
        let dir = std::env::temp_dir().join("fnoob-engine-tests");
        let target = RecordTarget {
            path: dir.join("never.wav"),
            first_channel: 1,
            channels: 2,
        };
        assert!(start_recording(2, 48_000, RecordFormat::Float32, vec![target]).is_err());
    }

    #[test]
    fn engine_recording_lands_in_the_wav_file() {
        use crate::plan::RenderPlan;
        use crate::processor::{Command, create};
        use fnoob_core::edit::add_track;
        use fnoob_core::model::{Project, TrackKind};

        let dir = std::env::temp_dir()
            .join("fnoob-engine-tests")
            .join(format!("engine-record-{}", std::process::id()));
        let mut project = Project::new("t", 48_000);
        project.metronome.enabled = false;
        let track = add_track(&mut project, TrackKind::Audio);
        project.track_mut(track).unwrap().record_enabled = true;
        let (mut processor, mut handle) = create(
            48_000,
            2,
            Box::new(RenderPlan::from_project(&project, 48_000)),
        );
        let (mut input_tx, input_rx) = audio_channel(1 << 16);
        handle
            .send(Command::SetAudioInput {
                samples: input_rx,
                channels: 2,
            })
            .unwrap();
        let target = RecordTarget {
            path: dir.join("audio").join("take.wav"),
            first_channel: 1,
            channels: 1,
        };
        let (recorder, sink) =
            start_recording(2, 48_000, RecordFormat::Float32, vec![target]).unwrap();
        handle.send(Command::SetRecordSink(sink)).unwrap();
        handle.send(Command::Record).unwrap();
        let mut out = vec![0.0f32; 2 * 480];
        for block in 0..10 {
            for i in 0..480 {
                input_tx.push(0.0).unwrap();
                input_tx.push((block * 480 + i) as f32 / 10_000.0).unwrap();
            }
            processor.process(&mut out);
        }
        handle.send(Command::Stop).unwrap();
        processor.process(&mut out);
        assert!(!handle.is_recording());
        assert_eq!(handle.audio_take_start(), Some(0));
        let files = recorder.finish().unwrap();
        assert_eq!(files[0].frames, 4800);
        let data = crate::audio_data::load_wav(&files[0].path, 48_000).unwrap();
        assert_eq!(data.samples[1234], 0.1234);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn integer_formats_keep_the_signal() {
        let dir = std::env::temp_dir()
            .join("fnoob-engine-tests")
            .join(format!("record-int-{}", std::process::id()));
        for (format, bits, tolerance) in [
            (RecordFormat::Int16, 16, 1.0 / 32_000.0),
            (RecordFormat::Int24, 24, 1.0 / 8_000_000.0),
        ] {
            let path = dir.join(format!("take-{bits}.wav"));
            let target = RecordTarget {
                path: path.clone(),
                first_channel: 0,
                channels: 1,
            };
            let (recorder, mut producer) =
                start_recording(1, 48_000, format, vec![target]).unwrap();
            for v in [0.5f32, -0.25, 1.5, f32::NAN] {
                producer.push(v).unwrap();
            }
            recorder.finish().unwrap();
            let reader = hound::WavReader::open(&path).unwrap();
            assert_eq!(reader.spec().bits_per_sample, bits);
            let data = crate::audio_data::load_wav(&path, 48_000).unwrap();
            for (got, want) in data.samples.iter().zip([0.5f32, -0.25, 1.0, 0.0]) {
                assert!(
                    (got - want).abs() <= tolerance,
                    "{bits}-bit: {got} vs {want}"
                );
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
