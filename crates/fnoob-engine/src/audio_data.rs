use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use fnoob_core::model::Id;
use fnoob_core::settings::RecordFormat;

pub const PEAK_BLOCK: usize = 256;
pub const SUMMARY_BLOCK: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Summary {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

pub const MAX_FILE_CHANNELS: u16 = 32;

pub type AudioPool = HashMap<Id, Arc<AudioData>>;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioData {
    pub channels: u16,
    pub frames: u64,
    pub samples: Vec<f32>,
    pub peaks: Vec<f32>,
    pub summaries: [Vec<Summary>; 2],
    onsets: std::sync::OnceLock<Arc<[u64]>>,
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
        let summaries = std::array::from_fn(|lane| {
            let channel = lane.min(width - 1);
            samples
                .chunks(SUMMARY_BLOCK * width)
                .map(|block| {
                    let mut summary = Summary::default();
                    let mut energy = 0.0;
                    let mut count = 0;
                    for frame in block.chunks(width) {
                        let s = frame[channel];
                        summary.min = summary.min.min(s);
                        summary.max = summary.max.max(s);
                        energy += s * s;
                        count += 1;
                    }
                    summary.rms = (energy / count.max(1) as f32).sqrt();
                    summary
                })
                .collect()
        });
        Self {
            channels: channels.max(1),
            frames,
            samples,
            peaks,
            summaries,
            onsets: std::sync::OnceLock::new(),
        }
    }

    pub fn onsets(&self) -> Arc<[u64]> {
        Arc::clone(self.onsets.get_or_init(|| self.find_onsets().into()))
    }

    fn refine_onset(&self, from: usize, to: usize) -> usize {
        let width = usize::from(self.channels);
        let to = to.min(self.frames as usize);
        let level = |frame: usize| {
            self.samples[frame * width..frame * width + width]
                .iter()
                .fold(0f32, |m, s| m.max(s.abs()))
        };
        let top = (from..to).map(level).fold(0f32, f32::max);
        (from..to)
            .find(|&frame| level(frame) >= top * 0.3)
            .map_or(from, |frame| frame.saturating_sub(16))
    }

    fn find_onsets(&self) -> Vec<u64> {
        const HOP: usize = 256;
        const RADIUS: usize = 12;
        let flux = self.onset_strength(HOP);
        let mean = flux.iter().sum::<f32>() / flux.len().max(1) as f32;
        let gap = (self.frames as usize / HOP).clamp(1, 9);
        let mut found = vec![0u64];
        let mut last = 0usize;
        for i in 1..flux.len().saturating_sub(1) {
            let (from, to) = (i.saturating_sub(RADIUS), (i + RADIUS + 1).min(flux.len()));
            let local = flux[from..to].iter().sum::<f32>() / (to - from) as f32;
            let peak = flux[i] >= flux[i - 1] && flux[i] > flux[i + 1];
            if peak && flux[i] > local * 1.5 + mean * 0.5 && i - last >= gap {
                let frame = self.refine_onset((i - 1) * HOP, (i + 1) * HOP) as u64;
                if frame > 0 && frame + 64 < self.frames {
                    found.push(frame);
                    last = i;
                }
            }
        }
        found
    }

    pub fn onset_strength(&self, hop: usize) -> Vec<f32> {
        let width = usize::from(self.channels);
        let energies: Vec<f32> = self
            .samples
            .chunks(hop * width)
            .map(|block| block.iter().map(|s| s * s).sum::<f32>().sqrt())
            .collect();
        let mut flux = vec![0.0; energies.len()];
        for i in 1..energies.len() {
            flux[i] = (energies[i] - energies[i - 1]).max(0.0);
        }
        flux
    }

    pub fn detect_tempo(&self, sample_rate: u32) -> Option<f64> {
        const HOP: usize = 512;
        let flux = self.onset_strength(HOP);
        let hops_per_second = f64::from(sample_rate) / HOP as f64;
        let lag_for = |bpm: f64| 60.0 * hops_per_second / bpm;
        let (shortest, longest) = (
            lag_for(180.0).floor() as usize,
            lag_for(60.0).ceil() as usize,
        );
        if flux.len() < longest * 4 || shortest == 0 {
            return None;
        }
        let score = |lag: usize| -> f32 {
            flux.iter()
                .zip(&flux[lag..])
                .map(|(a, b)| a * b)
                .sum::<f32>()
                / (flux.len() - lag) as f32
        };
        let (best, value) = (shortest..=longest)
            .map(|lag| (lag, score(lag)))
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        if value <= 0.0 {
            return None;
        }
        let refine = |lag: usize| -> f64 {
            let (a, b, c) = (score(lag - 1), score(lag), score(lag + 1));
            let denominator = a - 2.0 * b + c;
            if denominator.abs() < f32::EPSILON {
                lag as f64
            } else {
                lag as f64 + f64::from(0.5 * (a - c) / denominator)
            }
        };
        let mut bpm = 60.0 * hops_per_second / refine(best.max(1));
        while bpm < 80.0 {
            bpm *= 2.0;
        }
        while bpm > 160.0 {
            bpm /= 2.0;
        }
        Some((bpm * 100.0).round() / 100.0)
    }

    pub fn summary(&self, lane: usize, from: f64, to: f64) -> Summary {
        let width = usize::from(self.channels);
        let channel = lane.min(width - 1).min(1);
        let from = from.max(0.0);
        let to = to.max(from + 1.0).min(self.frames as f64);
        if from >= to {
            return Summary::default();
        }
        let (first, last) = (from as usize, to.ceil() as usize);
        if last - first < SUMMARY_BLOCK * 2 {
            let mut summary = Summary::default();
            let mut energy = 0.0;
            for frame in first..last {
                let s = self.samples[frame * width + lane.min(width - 1)];
                summary.min = summary.min.min(s);
                summary.max = summary.max.max(s);
                energy += s * s;
            }
            summary.rms = (energy / (last - first) as f32).sqrt();
            return summary;
        }
        let blocks = &self.summaries[channel];
        let (a, b) = (
            first / SUMMARY_BLOCK,
            last.div_ceil(SUMMARY_BLOCK).min(blocks.len()),
        );
        let span = &blocks[a..b.max(a)];
        let mut total = Summary::default();
        let mut energy = 0.0;
        for block in span {
            total.min = total.min.min(block.min);
            total.max = total.max.max(block.max);
            energy += block.rms * block.rms;
        }
        total.rms = (energy / span.len().max(1) as f32).sqrt();
        total
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
    fn onsets_land_on_each_hit() {
        let period = 12_000usize;
        let samples: Vec<f32> = (0..period * 8)
            .map(|i| {
                let phase = i % period;
                if phase < 600 {
                    (1.0 - phase as f32 / 600.0) * 0.9
                } else {
                    0.0
                }
            })
            .collect();
        let data = AudioData::from_samples(1, samples);
        let onsets = data.onsets();
        assert_eq!(onsets.len(), 8, "{onsets:?}");
        for (k, onset) in onsets.iter().enumerate().skip(1) {
            let expected = (k * period) as i64;
            assert!(
                (*onset as i64 - expected).abs() <= 512,
                "{onset} vs {expected}"
            );
        }
    }

    #[test]
    fn tempo_of_a_click_track_is_found() {
        let rate = 48_000u32;
        for bpm in [96.0f64, 120.0, 140.0] {
            let period = (60.0 / bpm * f64::from(rate)) as usize;
            let samples: Vec<f32> = (0..rate as usize * 12)
                .map(|i| {
                    let phase = i % period;
                    if phase < 400 {
                        (1.0 - phase as f32 / 400.0) * 0.8
                    } else {
                        0.0
                    }
                })
                .collect();
            let data = AudioData::from_samples(1, samples);
            let found = data.detect_tempo(rate).unwrap();
            assert!((found - bpm).abs() < 1.5, "{bpm} -> {found}");
        }
        assert!(
            AudioData::from_samples(1, vec![0.0; 1000])
                .detect_tempo(rate)
                .is_none()
        );
    }

    #[test]
    fn summaries_keep_each_side_of_the_wave() {
        let samples: Vec<f32> = (0..4000)
            .flat_map(|i| {
                let v = if i % 2 == 0 { 0.5 } else { -0.25 };
                [v, v * 0.5]
            })
            .collect();
        let data = AudioData::from_samples(2, samples);
        for (from, to) in [(0.0, 50.0), (0.0, 4000.0), (100.0, 3000.0)] {
            let left = data.summary(0, from, to);
            assert_eq!((left.min, left.max), (-0.25, 0.5));
            let right = data.summary(1, from, to);
            assert_eq!((right.min, right.max), (-0.125, 0.25));
            assert!((left.rms - (0.15625f32).sqrt()).abs() < 1e-3);
        }
        assert_eq!(data.summary(0, 5000.0, 6000.0), Summary::default());
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
