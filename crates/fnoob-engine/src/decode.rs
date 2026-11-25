use crate::audio_data::{AudioData, AudioFileError, MAX_FILE_CHANNELS, load_wav, wav_spec};
use crate::record::write_sample;
use fnoob_core::settings::RecordFormat;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use std::path::Path;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub const AUDIO_EXTENSIONS: [&str; 10] = [
    "wav", "mp3", "flac", "ogg", "oga", "aif", "aiff", "m4a", "mp4", "aac",
];

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

fn is_wav(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}

pub struct Decoded {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

fn error(e: impl std::fmt::Display) -> AudioFileError {
    AudioFileError::Decode(e.to_string())
}

pub fn decode_file(path: &Path) -> Result<Decoded, AudioFileError> {
    let file = std::fs::File::open(path).map_err(error)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(error)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| error("the file has no audio track"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| error("the audio format is not known"))?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(error)?;
    let mut sample_rate = params.sample_rate.unwrap_or(0);
    let mut channels = 0usize;
    let mut samples = Vec::new();
    let mut chunk: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(DecodeError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(error(e)),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                channels = buffer.spec().channels().count();
                sample_rate = buffer.spec().rate();
                buffer.copy_to_vec_interleaved(&mut chunk);
                samples.extend_from_slice(&chunk);
            }
            Err(DecodeError::DecodeError(_)) => continue,
            Err(e) => return Err(error(e)),
        }
    }
    if channels == 0 || samples.is_empty() || sample_rate == 0 {
        return Err(AudioFileError::Empty);
    }
    let channels = u16::try_from(channels).map_err(|_| AudioFileError::Channels(u16::MAX))?;
    if channels > MAX_FILE_CHANNELS {
        return Err(AudioFileError::Channels(channels));
    }
    Ok(Decoded {
        channels,
        sample_rate,
        samples,
    })
}

pub fn resample(
    samples: &[f32],
    channels: usize,
    from: u32,
    to: u32,
) -> Result<Vec<f32>, AudioFileError> {
    if from == to || samples.is_empty() {
        return Ok(samples.to_vec());
    }
    let frames = samples.len() / channels;
    let params = SincInterpolationParameters::new(128, WindowFunction::Blackman2)
        .oversampling_factor(256)
        .interpolation(SincInterpolationType::Quadratic);
    let mut resampler = Async::<f32>::new_sinc(
        f64::from(to) / f64::from(from),
        1.1,
        &params,
        1024,
        channels,
        FixedAsync::Input,
    )
    .map_err(error)?;
    let out_frames = resampler.process_all_needed_output_len(frames);
    let mut out = vec![0.0f32; out_frames * channels];
    let input = InterleavedSlice::new(samples, channels, frames).map_err(error)?;
    let mut output = InterleavedSlice::new_mut(&mut out, channels, out_frames).map_err(error)?;
    let (_, written) = resampler
        .process_all_into_buffer(&input, &mut output, frames, None)
        .map_err(error)?;
    out.truncate(written * channels);
    Ok(out)
}

pub fn load_any(path: &Path, project_rate: u32) -> Result<AudioData, AudioFileError> {
    if is_wav(path) {
        match load_wav(path, project_rate) {
            Err(AudioFileError::SampleRate { .. }) => {}
            other => return other,
        }
    }
    let decoded = decode_file(path)?;
    let samples = resample(
        &decoded.samples,
        usize::from(decoded.channels),
        decoded.sample_rate,
        project_rate,
    )?;
    let data = AudioData::from_samples(decoded.channels, samples);
    if data.frames == 0 {
        return Err(AudioFileError::Empty);
    }
    Ok(data)
}

pub fn needs_conversion(path: &Path, project_rate: u32) -> bool {
    !is_wav(path)
        || hound::WavReader::open(path).map_or(true, |r| r.spec().sample_rate != project_rate)
}

pub fn write_wav(path: &Path, data: &AudioData, sample_rate: u32) -> Result<(), AudioFileError> {
    let spec = wav_spec(data.channels, sample_rate, RecordFormat::Float32);
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in &data.samples {
        write_sample(&mut writer, RecordFormat::Float32, sample)?;
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fnoob-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn audio_extensions_are_recognised() {
        assert!(is_audio_file(Path::new("a/Song.MP3")));
        assert!(is_audio_file(Path::new("b.flac")));
        assert!(!is_audio_file(Path::new("notes.txt")));
    }

    #[test]
    fn resampling_keeps_length_and_level() {
        let input: Vec<f32> = (0..44_100)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 0.5)
            .collect();
        let out = resample(&input, 1, 44_100, 48_000).unwrap();
        assert!((out.len() as i64 - 48_000).abs() <= 2, "{}", out.len());
        let peak = out[1000..47_000].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 0.02, "{peak}");
    }

    #[test]
    fn wav_at_another_rate_is_converted() {
        let path = temp("other_rate.wav");
        let data = AudioData::from_samples(2, vec![0.25; 2 * 22_050]);
        write_wav(&path, &data, 22_050).unwrap();
        assert!(needs_conversion(&path, 48_000));
        assert!(!needs_conversion(&path, 22_050));
        let loaded = load_any(&path, 44_100).unwrap();
        assert_eq!(loaded.channels, 2);
        assert!((loaded.frames as i64 - 44_100).abs() <= 2);
        assert!((loaded.frame(20_000).0 - 0.25).abs() < 0.01);
        let decoded = decode_file(&path).unwrap();
        assert_eq!((decoded.channels, decoded.sample_rate), (2, 22_050));
        std::fs::remove_file(path).unwrap();
    }
}
