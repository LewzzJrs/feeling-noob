use crate::audio_data::{AudioData, AudioPool};
use fnoob_core::model::{DRUM_PADS, DrumParams, Instrument, SamplerParams, SynthParams, Waveform};
use std::f32::consts::TAU;
use std::sync::Arc;

pub const VOICES: usize = 16;
const DRUM_RELEASE_MS: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Voice {
    stage: Stage,
    pitch: u8,
    live: bool,
    one_shot: bool,
    pad: usize,
    phase: f32,
    increment: f32,
    cursor: f64,
    rate: f64,
    gain: f32,
    level: f32,
    off_at: u64,
    age: u64,
}

const IDLE: Voice = Voice {
    stage: Stage::Idle,
    pitch: 0,
    live: false,
    one_shot: false,
    pad: 0,
    phase: 0.0,
    increment: 0.0,
    cursor: 0.0,
    rate: 0.0,
    gain: 0.0,
    level: 0.0,
    off_at: 0,
    age: 0,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Pad {
    data: Option<Arc<AudioData>>,
    left: f32,
    right: f32,
}

#[derive(Debug, Clone, PartialEq)]
enum Source {
    Oscillator(Waveform),
    Sampler {
        data: Option<Arc<AudioData>>,
        root: u8,
    },
    Drums(Box<[Pad; DRUM_PADS]>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Synth {
    source: Source,
    attack_step: f32,
    decay_step: f32,
    sustain: f32,
    release_step: f32,
    output_gain: f32,
    sample_rate: f32,
    voices: [Voice; VOICES],
    counter: u64,
}

fn step_for(ms: f32, sample_rate: f32) -> f32 {
    1.0 / (ms * sample_rate / 1000.0).max(1.0)
}

fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

pub fn pitch_to_hz(pitch: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0)
}

fn interpolate(data: &AudioData, cursor: f64) -> (f32, f32) {
    let index = cursor as u64;
    let t = (cursor - index as f64) as f32;
    let (l0, r0) = data.frame(index);
    let (l1, r1) = data.frame(index + 1);
    (l0 + (l1 - l0) * t, r0 + (r1 - r0) * t)
}

struct Envelope {
    attack_ms: f32,
    decay_ms: f32,
    sustain: f32,
    release_ms: f32,
    level_db: f32,
}

impl Synth {
    fn with(source: Source, envelope: &Envelope, sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        let sustain = envelope.sustain.clamp(0.0, 1.0);
        Self {
            source,
            attack_step: step_for(envelope.attack_ms, sr),
            decay_step: step_for(envelope.decay_ms, sr) * (1.0 - sustain),
            sustain,
            release_step: step_for(envelope.release_ms, sr),
            output_gain: 10f32.powf(envelope.level_db / 20.0),
            sample_rate: sr,
            voices: [IDLE; VOICES],
            counter: 0,
        }
    }

    pub fn new(params: &SynthParams, sample_rate: u32) -> Self {
        Self::with(
            Source::Oscillator(params.waveform),
            &Envelope {
                attack_ms: params.attack_ms,
                decay_ms: params.decay_ms,
                sustain: params.sustain,
                release_ms: params.release_ms,
                level_db: params.level_db,
            },
            sample_rate,
        )
    }

    pub fn sampler(params: &SamplerParams, sample_rate: u32, pool: &AudioPool) -> Self {
        Self::with(
            Source::Sampler {
                data: params.file.and_then(|id| pool.get(&id).cloned()),
                root: params.root_pitch.min(127),
            },
            &Envelope {
                attack_ms: params.attack_ms,
                decay_ms: params.decay_ms,
                sustain: params.sustain,
                release_ms: params.release_ms,
                level_db: params.level_db,
            },
            sample_rate,
        )
    }

    pub fn drums(params: &DrumParams, sample_rate: u32, pool: &AudioPool) -> Self {
        let pads = params.pads.map(|pad| {
            let pan = pad.pan.clamp(-1.0, 1.0);
            let gain = 10f32.powf(pad.gain_db / 20.0);
            let (left, right) = ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0));
            Pad {
                data: pad.file.and_then(|id| pool.get(&id).cloned()),
                left: left * gain,
                right: right * gain,
            }
        });
        Self::with(
            Source::Drums(Box::new(pads)),
            &Envelope {
                attack_ms: 0.0,
                decay_ms: 0.0,
                sustain: 1.0,
                release_ms: DRUM_RELEASE_MS,
                level_db: params.level_db,
            },
            sample_rate,
        )
    }

    pub fn for_instrument(
        instrument: Option<&Instrument>,
        sample_rate: u32,
        pool: &AudioPool,
    ) -> Self {
        match instrument {
            Some(Instrument::BasicSynth(params)) => Self::new(params, sample_rate),
            Some(Instrument::Sampler(params)) => Self::sampler(params, sample_rate, pool),
            Some(Instrument::DrumSampler(params)) => Self::drums(params, sample_rate, pool),
            Some(Instrument::Vst3(_)) => Self::new(&SynthParams::default(), sample_rate),
            None => Self::sampler(
                &fnoob_core::model::SamplerParams::default(),
                sample_rate,
                pool,
            ),
        }
    }

    pub fn take_voices(&mut self, other: &Synth) {
        self.voices = other.voices;
        self.counter = other.counter;
    }

    fn free_voice(&self) -> usize {
        if let Some(i) = self.voices.iter().position(|v| v.stage == Stage::Idle) {
            return i;
        }
        let releasing = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.stage == Stage::Release)
            .min_by(|a, b| a.1.level.total_cmp(&b.1.level))
            .map(|(i, _)| i);
        releasing.unwrap_or_else(|| {
            self.voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.age)
                .map_or(0, |(i, _)| i)
        })
    }

    pub fn note_on(&mut self, pitch: u8, velocity: u8, off_at: u64, live: bool) {
        let pitch = pitch.min(127);
        let (one_shot, pad, rate) = match &self.source {
            Source::Oscillator(_) => (false, 0, 0.0),
            Source::Sampler { data, root } => {
                if data.is_none() {
                    return;
                }
                (
                    false,
                    0,
                    2f64.powf((f64::from(pitch) - f64::from(*root)) / 12.0),
                )
            }
            Source::Drums(pads) => {
                let Some(pad) = DrumParams::pad_for(pitch).filter(|&i| pads[i].data.is_some())
                else {
                    return;
                };
                (true, pad, 1.0)
            }
        };
        let index = self.free_voice();
        self.counter += 1;
        self.voices[index] = Voice {
            stage: if one_shot {
                Stage::Sustain
            } else {
                Stage::Attack
            },
            pitch,
            live,
            one_shot,
            pad,
            phase: 0.0,
            increment: pitch_to_hz(pitch) / self.sample_rate,
            cursor: 0.0,
            rate,
            gain: f32::from(velocity.min(127)) / 127.0,
            level: if one_shot { 1.0 } else { 0.0 },
            off_at,
            age: self.counter,
        };
    }

    pub fn note_off_live(&mut self, pitch: u8) {
        for voice in &mut self.voices {
            if voice.live && !voice.one_shot && voice.pitch == pitch && voice.stage != Stage::Idle {
                voice.stage = Stage::Release;
            }
        }
    }

    pub fn release_scheduled(&mut self) {
        for voice in &mut self.voices {
            if !voice.live && voice.stage != Stage::Idle {
                voice.stage = Stage::Release;
            }
        }
    }

    pub fn release_live(&mut self) {
        for voice in &mut self.voices {
            if voice.live && voice.stage != Stage::Idle {
                voice.stage = Stage::Release;
            }
        }
    }

    pub fn active_voices(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.stage != Stage::Idle)
            .count()
    }

    fn oscillator(waveform: Waveform, phase: f32, dt: f32) -> f32 {
        match waveform {
            Waveform::Sine => (TAU * phase).sin(),
            Waveform::Saw => 2.0 * phase - 1.0 - poly_blep(phase, dt),
            Waveform::Square => {
                let naive = if phase < 0.5 { 1.0 } else { -1.0 };
                let shifted = (phase + 0.5).fract();
                naive + poly_blep(phase, dt) - poly_blep(shifted, dt)
            }
        }
    }

    fn voice_frame(&self, voice: &mut Voice) -> Option<(f32, f32)> {
        match &self.source {
            Source::Oscillator(waveform) => {
                let x = Self::oscillator(*waveform, voice.phase, voice.increment);
                voice.phase = (voice.phase + voice.increment).fract();
                Some((x, x))
            }
            Source::Sampler { data, .. } => {
                let data = data.as_ref()?;
                if voice.cursor >= data.frames as f64 {
                    return None;
                }
                let frame = interpolate(data, voice.cursor);
                voice.cursor += voice.rate;
                Some(frame)
            }
            Source::Drums(pads) => {
                let pad = &pads[voice.pad];
                let data = pad.data.as_ref()?;
                if voice.cursor >= data.frames as f64 {
                    return None;
                }
                let (l, r) = data.frame(voice.cursor as u64);
                voice.cursor += voice.rate;
                Some((l * pad.left, r * pad.right))
            }
        }
    }

    pub fn next_frame(&mut self, position: u64) -> (f32, f32) {
        let (mut left, mut right) = (0.0, 0.0);
        for i in 0..VOICES {
            let mut voice = self.voices[i];
            if voice.stage == Stage::Idle {
                continue;
            }
            if !voice.live
                && !voice.one_shot
                && voice.stage != Stage::Release
                && position >= voice.off_at
            {
                voice.stage = Stage::Release;
            }
            match voice.stage {
                Stage::Attack => {
                    voice.level += self.attack_step;
                    if voice.level >= 1.0 {
                        voice.level = 1.0;
                        voice.stage = Stage::Decay;
                    }
                }
                Stage::Decay => {
                    voice.level -= self.decay_step;
                    if voice.level <= self.sustain {
                        voice.level = self.sustain;
                        voice.stage = Stage::Sustain;
                    }
                }
                Stage::Sustain => {
                    if !voice.one_shot {
                        voice.level = self.sustain;
                    }
                }
                Stage::Release => {
                    voice.level -= self.release_step;
                    if voice.level <= 0.0 {
                        self.voices[i] = IDLE;
                        continue;
                    }
                }
                Stage::Idle => {}
            }
            let Some((l, r)) = self.voice_frame(&mut voice) else {
                self.voices[i] = IDLE;
                continue;
            };
            let amount = voice.level * voice.gain;
            left += l * amount;
            right += r * amount;
            self.voices[i] = voice;
        }
        (left * self.output_gain, right * self.output_gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn params(waveform: Waveform) -> SynthParams {
        SynthParams {
            waveform,
            attack_ms: 1.0,
            decay_ms: 10.0,
            sustain: 0.5,
            release_ms: 10.0,
            level_db: 0.0,
        }
    }

    fn render(synth: &mut Synth, from: u64, frames: u64) -> Vec<f32> {
        (from..from + frames)
            .map(|p| synth.next_frame(p).0)
            .collect()
    }

    #[test]
    fn a_track_without_instrument_is_silent() {
        let mut synth = Synth::for_instrument(None, 48_000, &AudioPool::new());
        synth.note_on(60, 100, u64::MAX, true);
        let loudest = (0..2000)
            .map(|i| synth.next_frame(i))
            .fold(0.0f32, |m, (l, r)| m.max(l.abs()).max(r.abs()));
        assert_eq!(loudest, 0.0);
    }

    #[test]
    fn silent_without_notes() {
        let mut s = Synth::new(&SynthParams::default(), SR);
        assert!(render(&mut s, 0, 1000).iter().all(|&x| x == 0.0));
    }

    #[test]
    fn a4_is_440_hz() {
        assert_eq!(pitch_to_hz(69), 440.0);
        assert!((pitch_to_hz(81) - 880.0).abs() < 0.01);
    }

    #[test]
    fn scheduled_note_releases_at_off_sample() {
        let mut s = Synth::new(&params(Waveform::Saw), SR);
        s.note_on(60, 127, 4800, false);
        let out = render(&mut s, 0, 4800);
        assert!(out.iter().any(|x| x.abs() > 0.1));
        assert_eq!(s.active_voices(), 1);
        render(&mut s, 4800, 480);
        assert_eq!(s.active_voices(), 0);
    }

    #[test]
    fn output_is_finite_and_bounded_for_every_waveform() {
        for waveform in Waveform::ALL {
            let mut s = Synth::new(&params(waveform), SR);
            for pitch in [0, 21, 60, 108, 127] {
                s.note_on(pitch, 127, u64::MAX, true);
            }
            let out = render(&mut s, 0, 20_000);
            assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 5.0 * 1.2));
            assert!(out.iter().any(|x| x.abs() > 0.1));
        }
    }

    #[test]
    fn live_note_off_only_hits_matching_live_voices() {
        let mut s = Synth::new(&params(Waveform::Sine), SR);
        s.note_on(60, 100, u64::MAX, true);
        s.note_on(60, 100, 1_000_000, false);
        s.note_on(64, 100, u64::MAX, true);
        s.note_off_live(60);
        render(&mut s, 0, 2000);
        assert_eq!(s.active_voices(), 2);
        s.release_scheduled();
        s.release_live();
        render(&mut s, 2000, 2000);
        assert_eq!(s.active_voices(), 0);
    }

    #[test]
    fn seventeenth_note_steals_a_voice() {
        let mut s = Synth::new(&params(Waveform::Sine), SR);
        for pitch in 40..40 + VOICES as u8 + 1 {
            s.note_on(pitch, 100, u64::MAX, true);
        }
        assert_eq!(s.active_voices(), VOICES);
        assert!(!s.voices.iter().any(|v| v.pitch == 40));
    }

    #[test]
    fn velocity_scales_output() {
        let peak = |velocity| {
            let mut s = Synth::new(&params(Waveform::Square), SR);
            s.note_on(60, velocity, u64::MAX, true);
            render(&mut s, 0, 2000)
                .iter()
                .fold(0f32, |m, x| m.max(x.abs()))
        };
        assert!(peak(127) > peak(32) * 3.0);
    }

    fn pool_with(id: fnoob_core::model::Id, samples: Vec<f32>) -> AudioPool {
        let mut pool = AudioPool::new();
        pool.insert(id, Arc::new(AudioData::from_samples(1, samples)));
        pool
    }

    #[test]
    fn sampler_plays_at_root_and_an_octave_up() {
        let ramp: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let pool = pool_with(7, ramp);
        let params = SamplerParams {
            file: Some(7),
            root_pitch: 60,
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 1.0,
            level_db: 0.0,
        };
        let mut s = Synth::sampler(&params, SR, &pool);
        s.note_on(60, 127, u64::MAX, true);
        let out = render(&mut s, 0, 1200);
        assert!((out[500] - 0.5).abs() < 0.01);
        assert_eq!(s.active_voices(), 0);
        s.note_on(72, 127, u64::MAX, true);
        let out = render(&mut s, 0, 600);
        assert!((out[250] - 0.5).abs() < 0.01);
        assert_eq!(s.active_voices(), 0);
    }

    #[test]
    fn sampler_without_a_file_is_silent() {
        let mut s = Synth::sampler(&SamplerParams::default(), SR, &AudioPool::new());
        s.note_on(60, 127, u64::MAX, true);
        assert_eq!(s.active_voices(), 0);
        assert!(render(&mut s, 0, 100).iter().all(|&x| x == 0.0));
    }

    #[test]
    fn drum_pads_are_one_shots_mapped_from_c1() {
        let pool = pool_with(3, vec![0.5; 2000]);
        let mut params = DrumParams {
            level_db: 0.0,
            ..DrumParams::default()
        };
        params.pads[2].file = Some(3);
        params.pads[2].pan = 1.0;
        let mut s = Synth::drums(&params, SR, &pool);
        s.note_on(36, 127, u64::MAX, true);
        assert_eq!(s.active_voices(), 0);
        s.note_on(38, 127, 10, false);
        s.note_off_live(38);
        let left: Vec<f32> = (0..1500).map(|p| s.next_frame(p).0).collect();
        assert!(left.iter().all(|&x| x == 0.0));
        assert_eq!(s.active_voices(), 1);
        let right = s.next_frame(1500).1;
        assert!((right - 0.5).abs() < 1e-6);
        render(&mut s, 1501, 600);
        assert_eq!(s.active_voices(), 0);
    }
}
