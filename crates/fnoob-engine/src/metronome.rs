use std::f32::consts::TAU;

const CLICK_SECONDS: f32 = 0.03;
const ACCENT_HZ: f32 = 1568.0;
const NORMAL_HZ: f32 = 1046.5;
const LEVEL: f32 = 0.5;

pub struct Metronome {
    accent: Vec<f32>,
    normal: Vec<f32>,
    current: Option<(bool, usize)>,
}

fn click(sample_rate: u32, freq: f32) -> Vec<f32> {
    let sr = sample_rate as f32;
    let len = (sr * CLICK_SECONDS) as usize;
    let decay = 5.0 / len.max(1) as f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / sr;
            (TAU * freq * t).sin() * (-(i as f32) * decay).exp() * LEVEL
        })
        .collect()
}

impl Metronome {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            accent: click(sample_rate, ACCENT_HZ),
            normal: click(sample_rate, NORMAL_HZ),
            current: None,
        }
    }

    pub fn trigger(&mut self, accent: bool) {
        self.current = Some((accent, 0));
    }

    pub fn stop(&mut self) {
        self.current = None;
    }

    pub fn next_sample(&mut self) -> f32 {
        let Some((accent, index)) = self.current else {
            return 0.0;
        };
        let buffer = if accent { &self.accent } else { &self.normal };
        match buffer.get(index) {
            Some(&s) => {
                self.current = Some((accent, index + 1));
                s
            }
            None => {
                self.current = None;
                0.0
            }
        }
    }
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
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= LEVEL));
    }
}
