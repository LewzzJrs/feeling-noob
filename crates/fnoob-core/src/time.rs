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
pub const MAX_METER_CHANGES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timeline {
    pub sample_rate: u32,
    pub bpm: f64,
    changes: [(u64, f64, bool); MAX_TEMPO_CHANGES],
    count: usize,
}

impl Timeline {
    pub const fn new(sample_rate: u32, bpm: f64) -> Self {
        Self {
            sample_rate,
            bpm,
            changes: [(0, 0.0, false); MAX_TEMPO_CHANGES],
            count: 0,
        }
    }

    pub fn with_changes(
        sample_rate: u32,
        bpm: f64,
        changes: impl IntoIterator<Item = (u64, f64)>,
    ) -> Self {
        Self::with_ramps(
            sample_rate,
            bpm,
            changes.into_iter().map(|(t, b)| (t, b, false)),
        )
    }

    pub fn with_ramps(
        sample_rate: u32,
        bpm: f64,
        changes: impl IntoIterator<Item = (u64, f64, bool)>,
    ) -> Self {
        let mut timeline = Self::new(sample_rate, bpm);
        let mut last = 0;
        for (tick, change, ramp) in changes {
            if timeline.count == MAX_TEMPO_CHANGES {
                break;
            }
            if tick > last {
                timeline.changes[timeline.count] = (tick, clamp_bpm(change), ramp);
                timeline.count += 1;
                last = tick;
            }
        }
        timeline
    }

    fn scale(self) -> f64 {
        f64::from(self.sample_rate) * 60.0 / PPQ as f64
    }

    pub fn samples_per_tick(self) -> f64 {
        self.scale() / clamp_bpm(self.bpm)
    }

    pub fn changes(&self) -> impl Iterator<Item = (u64, f64)> + '_ {
        self.changes[..self.count].iter().map(|&(t, b, _)| (t, b))
    }

    fn point(&self, index: usize) -> (f64, f64, bool) {
        if index == 0 {
            (0.0, clamp_bpm(self.bpm), false)
        } else {
            let (t, b, r) = self.changes[index - 1];
            (t as f64, b, r)
        }
    }

    fn segment_samples(&self, from: (f64, f64), to: (f64, f64, bool), tick: f64) -> f64 {
        let k = self.scale();
        let (t0, b0) = from;
        let (t1, b1, ramp) = to;
        let delta = b1 - b0;
        if !ramp || delta.abs() < 1e-9 || t1 <= t0 {
            return (tick - t0) * k / b0;
        }
        let length = t1 - t0;
        let bpm = b0 + delta * (tick - t0) / length;
        k * length / delta * (bpm / b0).ln()
    }

    fn segment_ticks(&self, from: (f64, f64), to: (f64, f64, bool), samples: f64) -> f64 {
        let k = self.scale();
        let (t0, b0) = from;
        let (t1, b1, ramp) = to;
        let delta = b1 - b0;
        if !ramp || delta.abs() < 1e-9 || t1 <= t0 {
            return t0 + samples * b0 / k;
        }
        let length = t1 - t0;
        let bpm = b0 * (samples * delta / (k * length)).exp();
        t0 + length * (bpm - b0) / delta
    }

    pub fn bpm_at_tick(self, tick: u64) -> f64 {
        let tick = tick as f64;
        for index in 0..self.count {
            let (t0, b0, _) = self.point(index);
            let (t1, b1, ramp) = self.point(index + 1);
            if tick < t1 {
                return if ramp && t1 > t0 {
                    b0 + (b1 - b0) * (tick - t0) / (t1 - t0)
                } else {
                    b0
                };
            }
        }
        self.point(self.count).1
    }

    pub fn tick_to_sample_f64(self, tick: f64) -> f64 {
        let mut samples = 0.0;
        for index in 0..self.count {
            let (t0, b0, _) = self.point(index);
            let next = self.point(index + 1);
            if tick <= next.0 {
                return samples + self.segment_samples((t0, b0), next, tick);
            }
            samples += self.segment_samples((t0, b0), next, next.0);
        }
        let (t0, b0, _) = self.point(self.count);
        samples + (tick - t0) * self.scale() / b0
    }

    pub fn sample_to_tick_f64(self, sample: f64) -> f64 {
        let mut samples = 0.0;
        for index in 0..self.count {
            let (t0, b0, _) = self.point(index);
            let next = self.point(index + 1);
            let length = self.segment_samples((t0, b0), next, next.0);
            if sample < samples + length {
                return self.segment_ticks((t0, b0), next, sample - samples);
            }
            samples += length;
        }
        let (t0, b0, _) = self.point(self.count);
        t0 + (sample - samples) * b0 / self.scale()
    }

    pub fn tick_to_sample(self, tick: u64) -> u64 {
        self.tick_to_sample_f64(tick as f64).round() as u64
    }

    pub fn sample_to_tick(self, sample: u64) -> u64 {
        let mut tick = self.sample_to_tick_f64(sample as f64).max(0.0).floor() as u64;
        while tick > 0 && self.tick_to_sample(tick) > sample {
            tick -= 1;
        }
        while self.tick_to_sample(tick + 1) <= sample {
            tick += 1;
        }
        tick
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeterMap {
    base: TimeSignature,
    changes: [(u64, TimeSignature); MAX_METER_CHANGES],
    count: usize,
}

impl MeterMap {
    pub fn new(base: TimeSignature) -> Self {
        Self {
            base,
            changes: [(0, base); MAX_METER_CHANGES],
            count: 0,
        }
    }

    pub fn with_changes(
        base: TimeSignature,
        changes: impl IntoIterator<Item = (u64, TimeSignature)>,
    ) -> Self {
        let mut map = Self::new(base);
        let mut last = 0;
        for (bar, ts) in changes {
            if map.count == MAX_METER_CHANGES {
                break;
            }
            if bar > last {
                map.changes[map.count] = (bar, ts);
                map.count += 1;
                last = bar;
            }
        }
        map
    }

    fn segments(&self) -> impl Iterator<Item = (u64, u64, TimeSignature)> + '_ {
        let mut start_tick = 0;
        let mut previous = (0, self.base);
        std::iter::once((0, 0, self.base)).chain(self.changes[..self.count].iter().map(
            move |&(bar, ts)| {
                start_tick += (bar - previous.0) * ticks_per_bar(previous.1);
                previous = (bar, ts);
                (bar, start_tick, ts)
            },
        ))
    }

    pub fn segment_at_tick(&self, tick: u64) -> (u64, u64, TimeSignature) {
        self.segments()
            .take_while(|&(_, start, _)| start <= tick)
            .last()
            .unwrap_or((0, 0, self.base))
    }

    pub fn signature_at(&self, tick: u64) -> TimeSignature {
        self.segment_at_tick(tick).2
    }

    pub fn bar_start(&self, bar: u64) -> u64 {
        let (first_bar, start, ts) = self
            .segments()
            .take_while(|&(b, _, _)| b <= bar)
            .last()
            .unwrap_or((0, 0, self.base));
        start + (bar - first_bar) * ticks_per_bar(ts)
    }

    pub fn bar_at(&self, tick: u64) -> (u64, u64, TimeSignature) {
        let (first_bar, start, ts) = self.segment_at_tick(tick);
        let length = ticks_per_bar(ts);
        let bar = first_bar + (tick - start) / length;
        (bar, start + (bar - first_bar) * length, ts)
    }

    pub fn bar_floor(&self, tick: u64) -> u64 {
        self.bar_at(tick).1
    }

    pub fn bar_round(&self, tick: u64) -> u64 {
        let (bar, start, ts) = self.bar_at(tick);
        let next = self.bar_start(bar + 1);
        if tick - start >= (next - start) / 2 && ticks_per_bar(ts) > 0 {
            next
        } else {
            start
        }
    }

    pub fn position(&self, tick: u64) -> Position {
        let (bar, start, ts) = self.bar_at(tick);
        let beat_len = ticks_per_beat(ts);
        let in_bar = tick - start;
        let in_beat = in_bar % beat_len;
        Position {
            bar: bar + 1,
            beat: in_bar / beat_len + 1,
            division: in_beat / DIVISION_TICKS + 1,
            tick: in_beat % DIVISION_TICKS + 1,
        }
    }

    pub fn snap_floor(&self, tick: u64, grid: Grid) -> u64 {
        let (_, start, ts) = self.bar_at(tick);
        let length = grid_ticks(grid, ts).max(1);
        start + (tick - start) / length * length
    }

    pub fn snap_round(&self, tick: u64, grid: Grid) -> u64 {
        let down = self.snap_floor(tick, grid);
        let (bar, _, ts) = self.bar_at(tick);
        let next_bar = self.bar_start(bar + 1);
        let up = (down + grid_ticks(grid, ts).max(1)).min(next_bar);
        if tick - down >= (up - down).div_ceil(2) {
            up
        } else {
            down
        }
    }

    pub fn next_beat(&self, tick: u64) -> (u64, bool) {
        let (_, start, ts) = self.bar_at(tick);
        let beat_len = ticks_per_beat(ts);
        let beat = (tick - start).div_ceil(beat_len) * beat_len + start;
        let (_, next_start, _) = self.bar_at(beat);
        (beat, beat == next_start)
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
        assert_eq!(tl.changes().count(), 2);
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

    #[test]
    fn tempo_ramps_glide_between_points() {
        let tl = Timeline::with_ramps(48_000, 60.0, [(960, 120.0, true)]);
        let k = 48_000.0 * 60.0 / 960.0;
        let expected = k * 960.0 / 60.0 * 2f64.ln();
        assert!((tl.tick_to_sample_f64(960.0) - expected).abs() < 1e-6);
        assert!((tl.bpm_at_tick(480) - 90.0).abs() < 1e-9);
        assert_eq!(tl.bpm_at_tick(2000), 120.0);
        for tick in (0..4000).step_by(53) {
            assert_eq!(tl.sample_to_tick(tl.tick_to_sample(tick)), tick, "{tick}");
        }
        let after = tl.tick_to_sample(1920) - tl.tick_to_sample(960);
        assert_eq!(after, 24_000);
    }

    #[test]
    fn meter_changes_move_bars_and_beats() {
        let four = TimeSignature {
            numerator: 4,
            denominator: 4,
        };
        let three = TimeSignature {
            numerator: 3,
            denominator: 4,
        };
        let six_eight = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        let map = MeterMap::with_changes(four, [(2, three), (4, six_eight)]);
        assert_eq!(map.bar_start(2), 7680);
        assert_eq!(map.bar_start(3), 7680 + 2880);
        assert_eq!(map.bar_start(4), 7680 + 5760);
        assert_eq!(map.bar_start(5), 7680 + 5760 + 2880);
        assert_eq!(
            map.position(7680 + 2880 + 960).to_string(),
            Position {
                bar: 4,
                beat: 2,
                division: 1,
                tick: 1
            }
            .to_string()
        );
        assert_eq!(map.signature_at(20_000), six_eight);
        assert_eq!(map.bar_floor(9000), 7680);
        assert_eq!(map.bar_round(10_000), 10_560);
        assert_eq!(map.next_beat(7680), (7680, true));
        assert_eq!(map.next_beat(7681), (8640, false));
        assert_eq!(map.next_beat(13_441), (13_920, false));
        assert_eq!(map.next_beat(16_000), (16_320, true));
        assert_eq!(map.snap_round(8000, Grid::Bar), 7680);
        assert_eq!(map.snap_round(9500, Grid::Bar), 10_560);
        assert_eq!(map.snap_floor(9500, Grid::Beat), 9600 - 960);
        assert_eq!(map.snap_round(10_500, Grid::Beat), 10_560);
    }
}
