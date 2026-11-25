use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use fnoob_core::model::Id;
use fnoob_core::settings::RecordFormat;

pub const PEAK_BLOCK: usize = 256;
pub const MAX_FILE_CHANNELS: u16 = 32;

pub type AudioPool = HashMap<Id, Arc<AudioData>>;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioData {
    pub channels: u16,
    pub frames: u64,
    pub samples: Vec<f32>,
    pub peaks: Vec<f32>,
}

#[derive(Debug)]
pub enum AudioFileError {
    Wav(hound::Error),
    SampleRate { file: u32, project: u32 },
    Channels(u16),
    Empty,
    Decode(String),
}

impl fmt::Display for AudioFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wav(e) => write!(f, "cannot read WAV file: {e}"),
            Self::SampleRate { file, project } => write!(
                f,
                "the file runs at {file} Hz but the project runs at {project} Hz"
            ),
            Self::Channels(n) => write!(f, "{n} channels are not supported"),
            Self::Empty => write!(f, "the file has no audio"),
            Self::Decode(e) => write!(f, "cannot read the audio file: {e}"),
        }
    }
}

impl std::error::Error for AudioFileError {}

impl From<hound::Error> for AudioFileError {
    fn from(e: hound::Error) -> Self {
        Self::Wav(e)
    }
}

impl AudioData {
    pub fn from_samples(channels: u16, samples: Vec<f32>) -> Self {
        let width = usize::from(channels.max(1));
        let frames = (samples.len() / width) as u64;
        let peaks = samples
            .chunks(PEAK_BLOCK * width)
            .map(|block| block.iter().fold(0f32, |m, s| m.max(s.abs())))
            .collect();
        Self {
            channels: channels.max(1),
            frames,
            samples,
            peaks,
        }
    }

    pub fn frame(&self, index: u64) -> (f32, f32) {
        if index >= self.frames {
            return (0.0, 0.0);
        }
        let width = usize::from(self.channels);
        let at = index as usize * width;
        let left = self.samples[at];
        let right = if width > 1 {
            self.samples[at + 1]
        } else {
            left
        };
        (left, right)
    }

    pub fn peak(&self, from: u64, to: u64) -> f32 {
        let first = (from / PEAK_BLOCK as u64) as usize;
        let last = (to.div_ceil(PEAK_BLOCK as u64) as usize).min(self.peaks.len());
        self.peaks
            .get(first..last.max(first))
            .map_or(0.0, |p| p.iter().fold(0f32, |m, &v| m.max(v)))
    }
}

pub fn load_wav(path: &Path, project_rate: u32) -> Result<AudioData, AudioFileError> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate != project_rate {
        return Err(AudioFileError::SampleRate {
            file: spec.sample_rate,
            project: project_rate,
        });
    }
    if spec.channels == 0 || spec.channels > MAX_FILE_CHANNELS {
        return Err(AudioFileError::Channels(spec.channels));
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()?
        }
    };
    let data = AudioData::from_samples(spec.channels, samples);
    if data.frames == 0 {
        return Err(AudioFileError::Empty);
    }
    Ok(data)
}

pub fn wav_spec(channels: u16, sample_rate: u32, format: RecordFormat) -> hound::WavSpec {
    let (bits_per_sample, sample_format) = match format {
        RecordFormat::Int16 => (16, hound::SampleFormat::Int),
        RecordFormat::Int24 => (24, hound::SampleFormat::Int),
        RecordFormat::Float32 => (32, hound::SampleFormat::Float),
    };
    hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample,
        sample_format,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join("fnoob-engine-tests")
            .join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("file.wav")
    }

    fn write_int(path: &Path, channels: u16, rate: u32, bits: u16, values: &[i32]) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for v in values {
            w.write_sample(*v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn loads_16_bit_stereo_as_float() {
        let path = scratch("int16");
        write_int(&path, 2, 48_000, 16, &[16_384, -32_768, 0, 32_767]);
        let data = load_wav(&path, 48_000).unwrap();
        assert_eq!((data.channels, data.frames), (2, 2));
        assert_eq!(data.frame(0), (0.5, -1.0));
        assert_eq!(data.frame(5), (0.0, 0.0));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn mono_plays_on_both_sides() {
        let data = AudioData::from_samples(1, vec![0.25, -0.5]);
        assert_eq!(data.frame(1), (-0.5, -0.5));
    }

    #[test]
    fn float_round_trip_and_rate_check() {
        let path = scratch("float");
        let mut w =
            hound::WavWriter::create(&path, wav_spec(1, 44_100, RecordFormat::Float32)).unwrap();
        for v in [0.1f32, -0.2, 0.3] {
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
        assert_eq!(load_wav(&path, 44_100).unwrap().samples, [0.1, -0.2, 0.3]);
        assert!(matches!(
            load_wav(&path, 48_000),
            Err(AudioFileError::SampleRate { .. })
        ));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_or_broken_file_is_an_error() {
        let path = scratch("broken");
        assert!(load_wav(&path, 48_000).is_err());
        std::fs::write(&path, b"not a wav").unwrap();
        assert!(load_wav(&path, 48_000).is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn peaks_cover_blocks() {
        let mut samples = vec![0.0; PEAK_BLOCK * 3];
        samples[PEAK_BLOCK + 5] = -0.8;
        let data = AudioData::from_samples(1, samples);
        assert_eq!(data.peaks, [0.0, 0.8, 0.0]);
        assert_eq!(data.peak(0, PEAK_BLOCK as u64), 0.0);
        assert_eq!(data.peak(0, 3 * PEAK_BLOCK as u64), 0.8);
    }
}
