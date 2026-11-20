use crate::model::{Grid, TimeSignature};

pub const PPQ: u64 = 960;
pub const DIVISION_TICKS: u64 = PPQ / 4;
pub const MIN_BPM: f64 = 20.0;
pub const MAX_BPM: f64 = 400.0;

pub fn clamp_bpm(bpm: f64) -> f64 {
    if bpm.is_nan() {
        120.0
    } else {
        bpm.clamp(MIN_BPM, MAX_BPM)
    }
}

pub fn ticks_per_beat(ts: TimeSignature) -> u64 {
    PPQ * 4 / u64::from(ts.denominator.max(1))
}

pub fn ticks_per_bar(ts: TimeSignature) -> u64 {
    ticks_per_beat(ts) * u64::from(ts.numerator.max(1))
}

pub fn grid_ticks(grid: Grid, ts: TimeSignature) -> u64 {
    match grid {
        Grid::Bar => ticks_per_bar(ts),
        Grid::Beat => ticks_per_beat(ts),
        Grid::Eighth => PPQ / 2,
        Grid::Sixteenth => PPQ / 4,
    }
}

pub fn snap_round(tick: u64, grid: u64) -> u64 {
    if grid == 0 {
        return tick;
    }
    (tick + grid / 2) / grid * grid
}

pub fn snap_floor(tick: u64, grid: u64) -> u64 {
    if grid == 0 {
        return tick;
    }
    tick / grid * grid
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timeline {
    pub sample_rate: u32,
    pub bpm: f64,
}

impl Timeline {
    pub fn samples_per_tick(self) -> f64 {
        f64::from(self.sample_rate) * 60.0 / (clamp_bpm(self.bpm) * PPQ as f64)
    }

    pub fn tick_to_sample(self, tick: u64) -> u64 {
        (tick as f64 * self.samples_per_tick()).round() as u64
    }

    pub fn sample_to_tick(self, sample: u64) -> u64 {
        (sample as f64 / self.samples_per_tick()).floor() as u64
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub bar: u64,
    pub beat: u64,
    pub division: u64,
    pub tick: u64,
}

impl Position {
    pub fn from_tick(tick: u64, ts: TimeSignature) -> Self {
        let bar_len = ticks_per_bar(ts);
        let beat_len = ticks_per_beat(ts);
        let in_bar = tick % bar_len;
        let in_beat = in_bar % beat_len;
        Self {
            bar: tick / bar_len + 1,
            beat: in_bar / beat_len + 1,
            division: in_beat / DIVISION_TICKS + 1,
            tick: in_beat % DIVISION_TICKS + 1,
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:>3} {} {} {:>3}",
            self.bar, self.beat, self.division, self.tick
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOUR_FOUR: TimeSignature = TimeSignature {
        numerator: 4,
        denominator: 4,
    };
    const SIX_EIGHT: TimeSignature = TimeSignature {
        numerator: 6,
        denominator: 8,
    };

    #[test]
    fn bpm_is_clamped() {
        assert_eq!(clamp_bpm(1.0), MIN_BPM);
        assert_eq!(clamp_bpm(5000.0), MAX_BPM);
        assert_eq!(clamp_bpm(f64::NAN), 120.0);
    }

    #[test]
    fn bar_and_beat_lengths() {
        assert_eq!(ticks_per_beat(FOUR_FOUR), 960);
        assert_eq!(ticks_per_bar(FOUR_FOUR), 3840);
        assert_eq!(ticks_per_beat(SIX_EIGHT), 480);
        assert_eq!(ticks_per_bar(SIX_EIGHT), 2880);
        assert_eq!(grid_ticks(Grid::Sixteenth, SIX_EIGHT), 240);
    }

    #[test]
    fn snapping() {
        assert_eq!(snap_round(1919, 960), 1920);
        assert_eq!(snap_round(1439, 960), 960);
        assert_eq!(snap_floor(1919, 960), 960);
        assert_eq!(snap_round(77, 0), 77);
    }

    #[test]
    fn tick_sample_round_trip_at_120_bpm_48k() {
        let tl = Timeline {
            sample_rate: 48_000,
            bpm: 120.0,
        };
        assert_eq!(tl.tick_to_sample(960), 24_000);
        assert_eq!(tl.sample_to_tick(24_000), 960);
        assert_eq!(tl.sample_to_tick(23_999), 959);
    }

    #[test]
    fn first_beat_with_fractional_beat_length() {
        let spb = 44_100.0 * 60.0 / 97.0;
        for beat in 0..1000 {
            let start = beat_start_sample(beat, spb);
            assert_eq!(first_beat_at_or_after(start, spb), beat);
            assert_eq!(first_beat_at_or_after(start + 1, spb), beat + 1);
        }
    }

    #[test]
    fn position_is_one_based() {
        let p = Position::from_tick(0, FOUR_FOUR);
        assert_eq!((p.bar, p.beat, p.division, p.tick), (1, 1, 1, 1));
        let p = Position::from_tick(3840 + 960 + 240 + 5, FOUR_FOUR);
        assert_eq!((p.bar, p.beat, p.division, p.tick), (2, 2, 2, 6));
        let p = Position::from_tick(2880 + 480, SIX_EIGHT);
        assert_eq!((p.bar, p.beat), (2, 2));
    }
}
