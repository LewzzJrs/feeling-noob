
pub const MIN_BPM: f64 = 20.0;
pub const MAX_BPM: f64 = 400.0;

pub fn clamp_bpm(bpm: f64) -> f64 {
    if bpm.is_nan() {
        120.0
    } else {
        bpm.clamp(MIN_BPM, MAX_BPM)
    }
}

pub fn samples_per_beat(sample_rate: u32, bpm: f64) -> f64 {
    f64::from(sample_rate) * 60.0 / clamp_bpm(bpm)
}

pub fn beat_start_sample(beat: u64, samples_per_beat: f64) -> u64 {
    (beat as f64 * samples_per_beat).round() as u64
}

pub fn first_beat_at_or_after(sample: u64, samples_per_beat: f64) -> u64 {
    let mut beat = (sample as f64 / samples_per_beat).floor() as u64;
    while beat_start_sample(beat, samples_per_beat) < sample {
        beat += 1;
    }
    beat
}

pub fn bar_and_beat(sample: u64, samples_per_beat: f64, beats_per_bar: u32) -> (u64, u64) {
    let beats_per_bar = u64::from(beats_per_bar.max(1));
    let beat = (sample as f64 / samples_per_beat).floor() as u64;
    (beat / beats_per_bar + 1, beat % beats_per_bar + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_per_beat_at_120_bpm_48k() {
        assert_eq!(samples_per_beat(48_000, 120.0), 24_000.0);
    }

    #[test]
    fn bpm_is_clamped() {
        assert_eq!(clamp_bpm(1.0), MIN_BPM);
        assert_eq!(clamp_bpm(5000.0), MAX_BPM);
        assert_eq!(clamp_bpm(f64::NAN), 120.0);
    }

    #[test]
    fn first_beat_handles_exact_and_between() {
        let spb = 24_000.0;
        assert_eq!(first_beat_at_or_after(0, spb), 0);
        assert_eq!(first_beat_at_or_after(24_000, spb), 1);
        assert_eq!(first_beat_at_or_after(24_001, spb), 2);
    }

    #[test]
    fn first_beat_with_fractional_beat_length() {
        let spb = samples_per_beat(44_100, 97.0);
        for beat in 0..1000 {
            let start = beat_start_sample(beat, spb);
            assert_eq!(first_beat_at_or_after(start, spb), beat);
            assert_eq!(first_beat_at_or_after(start + 1, spb), beat + 1);
        }
    }

    #[test]
    fn bar_and_beat_are_one_based() {
        let spb = 24_000.0;
        assert_eq!(bar_and_beat(0, spb, 4), (1, 1));
        assert_eq!(bar_and_beat(23_999, spb, 4), (1, 1));
        assert_eq!(bar_and_beat(24_000 * 5, spb, 4), (2, 2));
    }
}
