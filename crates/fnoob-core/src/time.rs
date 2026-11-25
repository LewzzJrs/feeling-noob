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

pub const MAX_TEMPO_CHANGES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timeline {
    pub sample_rate: u32,
    pub bpm: f64,
    changes: [(u64, f64); MAX_TEMPO_CHANGES],
    count: usize,
}

impl Timeline {
    pub const fn new(sample_rate: u32, bpm: f64) -> Self {
        Self {
            sample_rate,
            bpm,
            changes: [(0, 0.0); MAX_TEMPO_CHANGES],
            count: 0,
        }
    }

    pub fn with_changes(
        sample_rate: u32,
        bpm: f64,
        changes: impl IntoIterator<Item = (u64, f64)>,
    ) -> Self {
        let mut timeline = Self::new(sample_rate, bpm);
        let mut last = 0;
        for (tick, change) in changes {
            if timeline.count == MAX_TEMPO_CHANGES {
                break;
            }
            if tick > last {
                timeline.changes[timeline.count] = (tick, change);
                timeline.count += 1;
                last = tick;
            }
        }
        timeline
    }

    fn per_tick(self, bpm: f64) -> f64 {
        f64::from(self.sample_rate) * 60.0 / (clamp_bpm(bpm) * PPQ as f64)
    }

    pub fn samples_per_tick(self) -> f64 {
        self.per_tick(self.bpm)
    }

    pub fn changes(&self) -> &[(u64, f64)] {
        &self.changes[..self.count]
    }

    pub fn bpm_at_tick(self, tick: u64) -> f64 {
        self.changes()
            .iter()
            .take_while(|(t, _)| *t <= tick)
            .last()
            .map_or(self.bpm, |(_, b)| *b)
    }

    pub fn tick_to_sample_f64(self, tick: f64) -> f64 {
        let mut samples = 0.0;
        let (mut from, mut bpm) = (0.0, self.bpm);
        for &(at, next) in self.changes() {
            let at = at as f64;
            if at >= tick {
                break;
            }
            samples += (at - from) * self.per_tick(bpm);
            from = at;
            bpm = next;
        }
        samples + (tick - from) * self.per_tick(bpm)
    }

    pub fn sample_to_tick_f64(self, sample: f64) -> f64 {
        let mut samples = 0.0;
        let (mut from, mut bpm) = (0.0, self.bpm);
        for &(at, next) in self.changes() {
            let at = at as f64;
            let end = samples + (at - from) * self.per_tick(bpm);
            if end > sample {
                break;
            }
            samples = end;
            from = at;
            bpm = next;
        }
        from + (sample - samples) / self.per_tick(bpm)
    }

    pub fn tick_to_sample(self, tick: u64) -> u64 {
        self.tick_to_sample_f64(tick as f64).round() as u64
    }

    pub fn sample_to_tick(self, sample: u64) -> u64 {
        let tick = self.sample_to_tick_f64(sample as f64);
        let rounded = tick.round();
        if (tick - rounded).abs() < 1e-6 {
            rounded as u64
        } else {
            tick.floor() as u64
        }
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
        let tl = Timeline::new(48_000, 120.0);
        assert_eq!(tl.tick_to_sample(960), 24_000);
        assert_eq!(tl.sample_to_tick(24_000), 960);
        assert_eq!(tl.sample_to_tick(23_999), 959);
    }

    #[test]
    fn tempo_changes_bend_the_time_line() {
        let tl = Timeline::with_changes(48_000, 120.0, [(960, 60.0), (0, 30.0), (2880, 240.0)]);
        assert_eq!(tl.changes().len(), 2);
        assert_eq!(tl.tick_to_sample(960), 24_000);
        assert_eq!(tl.tick_to_sample(1920), 72_000);
        assert_eq!(tl.tick_to_sample(2880), 120_000);
        assert_eq!(tl.tick_to_sample(3840), 132_000);
        assert_eq!(tl.sample_to_tick(48_000), 1440);
        assert_eq!(tl.sample_to_tick(132_000), 3840);
        assert_eq!(tl.bpm_at_tick(959), 120.0);
        assert_eq!(tl.bpm_at_tick(960), 60.0);
        assert_eq!(tl.bpm_at_tick(5000), 240.0);
        for tick in (0..6000).step_by(37) {
            assert_eq!(tl.sample_to_tick(tl.tick_to_sample(tick)), tick);
        }
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
