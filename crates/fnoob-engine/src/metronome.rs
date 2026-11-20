use std::f32::consts::TAU;

const CLICK_SECONDS: f32 = 0.03;
const ACCENT_HZ: f32 = 1568.0;
const NORMAL_HZ: f32 = 1046.5;
const CLICK_LEVEL: f32 = 0.5;
const BEEP_SECONDS: f32 = 0.12;
const BEEP_HZ: f32 = 659.3;
const BEEP_LEVEL: f32 = 0.3;

pub struct OneShot {
    buffer: Vec<f32>,
    index: Option<usize>,
}

impl OneShot {
    pub fn tone(sample_rate: u32, freq: f32, seconds: f32, level: f32) -> Self {
        let sr = sample_rate as f32;
        let len = (sr * seconds) as usize;
        let decay = 5.0 / len.max(1) as f32;
        let buffer = (0..len)
            .map(|i| (TAU * freq * i as f32 / sr).sin() * (-(i as f32) * decay).exp() * level)
            .collect();
        Self {
            buffer,
            index: None,
        }
    }

    pub fn trigger(&mut self) {
        self.index = Some(0);
    }

    pub fn stop(&mut self) {
        self.index = None;
    }

    pub fn next_sample(&mut self) -> f32 {
        let Some(index) = self.index else {
            return 0.0;
        };
        match self.buffer.get(index) {
            Some(&s) => {
                self.index = Some(index + 1);
                s
            }
            None => {
                self.index = None;
                0.0
            }
        }
    }
}

pub struct Metronome {
    accent: OneShot,
    normal: OneShot,
}

impl Metronome {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            accent: OneShot::tone(sample_rate, ACCENT_HZ, CLICK_SECONDS, CLICK_LEVEL),
            normal: OneShot::tone(sample_rate, NORMAL_HZ, CLICK_SECONDS, CLICK_LEVEL),
        }
    }

    pub fn trigger(&mut self, accent: bool) {
        self.stop();
        if accent {
            self.accent.trigger();
        } else {
            self.normal.trigger();
        }
    }

    pub fn stop(&mut self) {
        self.accent.stop();
        self.normal.stop();
    }

    pub fn next_sample(&mut self) -> f32 {
        self.accent.next_sample() + self.normal.next_sample()
    }
}

pub fn beep(sample_rate: u32) -> OneShot {
    OneShot::tone(sample_rate, BEEP_HZ, BEEP_SECONDS, BEEP_LEVEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_until_triggered() {
        let mut m = Metronome::new(48_000);
        assert!((0..100).all(|_| m.next_sample() == 0.0));
    }

    #[test]
    fn click_has_finite_length_and_level() {
        let mut m = Metronome::new(48_000);
        m.trigger(true);
        let samples: Vec<f32> = (0..3000).map(|_| m.next_sample()).collect();
        let len = (48_000.0 * CLICK_SECONDS) as usize;
        assert!(samples[..len].iter().any(|s| s.abs() > 0.1));
        assert!(samples[len..].iter().all(|&s| s == 0.0));
        assert!(
            samples
                .iter()
                .all(|s| s.is_finite() && s.abs() <= CLICK_LEVEL)
        );
    }

    #[test]
    fn retrigger_restarts() {
        let mut b = beep(48_000);
        b.trigger();
        let first: Vec<f32> = (0..50).map(|_| b.next_sample()).collect();
        b.trigger();
        let again: Vec<f32> = (0..50).map(|_| b.next_sample()).collect();
        assert_eq!(first, again);
    }
}
