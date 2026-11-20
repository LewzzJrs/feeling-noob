use fnoob_core::model::{SynthParams, Waveform};
use std::f32::consts::TAU;

pub const VOICES: usize = 16;

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
    phase: f32,
    increment: f32,
    gain: f32,
    level: f32,
    off_at: u64,
    age: u64,
}

const IDLE: Voice = Voice {
    stage: Stage::Idle,
    pitch: 0,
    live: false,
    phase: 0.0,
    increment: 0.0,
    gain: 0.0,
    level: 0.0,
    off_at: 0,
    age: 0,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Synth {
    waveform: Waveform,
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

impl Synth {
    pub fn new(params: &SynthParams, sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        let sustain = params.sustain.clamp(0.0, 1.0);
        Self {
            waveform: params.waveform,
            attack_step: step_for(params.attack_ms, sr),
            decay_step: step_for(params.decay_ms, sr) * (1.0 - sustain),
            sustain,
            release_step: step_for(params.release_ms, sr),
            output_gain: 10f32.powf(params.level_db / 20.0),
            sample_rate: sr,
            voices: [IDLE; VOICES],
            counter: 0,
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
        let index = self.free_voice();
        self.counter += 1;
        self.voices[index] = Voice {
            stage: Stage::Attack,
            pitch,
            live,
            phase: 0.0,
            increment: pitch_to_hz(pitch.min(127)) / self.sample_rate,
            gain: f32::from(velocity.min(127)) / 127.0,
            level: 0.0,
            off_at,
            age: self.counter,
        };
    }

    pub fn note_off_live(&mut self, pitch: u8) {
        for voice in &mut self.voices {
            if voice.live && voice.pitch == pitch && voice.stage != Stage::Idle {
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

    fn oscillator(&self, phase: f32, dt: f32) -> f32 {
        match self.waveform {
            Waveform::Sine => (TAU * phase).sin(),
            Waveform::Saw => 2.0 * phase - 1.0 - poly_blep(phase, dt),
            Waveform::Square => {
                let naive = if phase < 0.5 { 1.0 } else { -1.0 };
                let shifted = (phase + 0.5).fract();
                naive + poly_blep(phase, dt) - poly_blep(shifted, dt)
            }
        }
    }

    pub fn next_sample(&mut self, position: u64) -> f32 {
        let mut sum = 0.0;
        for i in 0..VOICES {
            let mut voice = self.voices[i];
            if voice.stage == Stage::Idle {
                continue;
            }
            if !voice.live && voice.stage != Stage::Release && position >= voice.off_at {
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
                Stage::Sustain => voice.level = self.sustain,
                Stage::Release => {
                    voice.level -= self.release_step;
                    if voice.level <= 0.0 {
                        self.voices[i] = IDLE;
                        continue;
                    }
                }
                Stage::Idle => {}
            }
            sum += self.oscillator(voice.phase, voice.increment) * voice.level * voice.gain;
            voice.phase = (voice.phase + voice.increment).fract();
            self.voices[i] = voice;
        }
        sum * self.output_gain
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
            .map(|p| synth.next_sample(p))
            .collect()
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
}
