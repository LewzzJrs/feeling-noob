use fnoob_core::model::{Id, Project, TimeSignature};
use fnoob_core::time::{DIVISION_TICKS, MeterMap, Timeline, ticks_per_bar, ticks_per_beat};

pub const OVERVIEW_ROW: f32 = 12.0;
pub const TIME_ROW: f32 = 18.0;
const MAX_ZOOM_HISTORY: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSelection {
    pub start: u64,
    pub end: u64,
    pub tracks: Vec<Id>,
}

impl TimeSelection {
    pub fn length(&self) -> u64 {
        self.end - self.start
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomState {
    pub pixels_per_tick: f32,
    pub scroll_tick: f32,
}

#[derive(Debug, Default)]
pub struct ZoomHistory {
    states: Vec<ZoomState>,
}

impl ZoomHistory {
    pub fn push(&mut self, state: ZoomState) {
        if self.states.len() == MAX_ZOOM_HISTORY {
            self.states.remove(0);
        }
        self.states.push(state);
    }

    pub fn pop(&mut self) -> Option<ZoomState> {
        self.states.pop()
    }
}

pub fn position_text(meter: &MeterMap, tick: u64) -> String {
    let p = meter.position(tick);
    format!("{}.{}.{}", p.bar, p.beat, p.division)
}

pub fn length_text(ticks: u64, ts: TimeSignature) -> String {
    let bar = ticks_per_bar(ts);
    let beat = ticks_per_beat(ts);
    let rest = ticks % bar;
    format!(
        "{}.{}.{}",
        ticks / bar,
        rest / beat,
        rest % beat / DIVISION_TICKS
    )
}

pub fn clock_text(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

pub fn seconds_at(timeline: Timeline, tick: u64, rate: u32) -> f64 {
    timeline.tick_to_sample_f64(tick as f64) / f64::from(rate.max(1))
}

pub fn tick_at_seconds(timeline: Timeline, seconds: f64, rate: u32) -> u64 {
    timeline.sample_to_tick((seconds * f64::from(rate.max(1))).max(0.0) as u64)
}

pub fn clock_step(pixels_per_second: f32) -> u64 {
    const STEPS: [u64; 10] = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600];
    STEPS
        .into_iter()
        .find(|s| *s as f32 * pixels_per_second >= 56.0)
        .unwrap_or(1200)
}

pub fn fit(start: u64, end: u64, width: f32, min_ppt: f32, max_ppt: f32) -> ZoomState {
    let span = end.saturating_sub(start).max(1) as f32;
    let margin = width * 0.03;
    let pixels_per_tick = ((width - 2.0 * margin).max(1.0) / span).clamp(min_ppt, max_ppt);
    ZoomState {
        pixels_per_tick,
        scroll_tick: (start as f32 - margin / pixels_per_tick).max(0.0),
    }
}

pub fn song_end(project: &Project) -> u64 {
    let regions = project
        .tracks
        .iter()
        .flat_map(|t| t.regions.iter())
        .map(|r| r.end_tick())
        .max()
        .unwrap_or(0);
    let markers = project.markers.iter().map(|m| m.tick).max().unwrap_or(0);
    let floor = 4 * ticks_per_bar(project.time_signature);
    regions
        .max(markers)
        .max(project.loop_range.end_tick)
        .max(floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOUR_FOUR: TimeSignature = TimeSignature {
        numerator: 4,
        denominator: 4,
    };

    #[test]
    fn positions_and_lengths_read_like_the_display() {
        let meter = MeterMap::new(FOUR_FOUR);
        let bar = ticks_per_bar(FOUR_FOUR);
        assert_eq!(position_text(&meter, 0), "1.1.1");
        assert_eq!(position_text(&meter, 14 * bar), "15.1.1");
        assert_eq!(length_text(8 * bar, FOUR_FOUR), "8.0.0");
        assert_eq!(
            length_text(bar + ticks_per_beat(FOUR_FOUR) + DIVISION_TICKS, FOUR_FOUR),
            "1.1.1"
        );
    }

    #[test]
    fn clock_shows_minutes_and_seconds() {
        assert_eq!(clock_text(0.0), "0:00");
        assert_eq!(clock_text(28.4), "0:28");
        assert_eq!(clock_text(125.0), "2:05");
    }

    #[test]
    fn seconds_follow_the_tempo() {
        let timeline = Timeline::new(48_000, 120.0);
        let bar = ticks_per_bar(FOUR_FOUR);
        assert!((seconds_at(timeline, bar, 48_000) - 2.0).abs() < 1e-6);
        assert_eq!(tick_at_seconds(timeline, 2.0, 48_000), bar);
    }

    #[test]
    fn clock_labels_stay_apart() {
        assert_eq!(clock_step(100.0), 1);
        assert_eq!(clock_step(20.0), 5);
        assert_eq!(clock_step(0.01), 1200);
    }

    #[test]
    fn fitting_puts_the_range_inside_the_width() {
        let state = fit(1000, 2000, 1000.0, 0.0001, 10.0);
        let left = (1000.0 - state.scroll_tick) * state.pixels_per_tick;
        let right = (2000.0 - state.scroll_tick) * state.pixels_per_tick;
        assert!(left > 0.0 && right < 1000.0);
        assert!((right - left - 940.0).abs() < 1.0);
    }

    #[test]
    fn zoom_history_forgets_the_oldest() {
        let mut history = ZoomHistory::default();
        for i in 0..40 {
            history.push(ZoomState {
                pixels_per_tick: 1.0,
                scroll_tick: i as f32,
            });
        }
        assert_eq!(history.pop().map(|s| s.scroll_tick), Some(39.0));
        let mut count = 1;
        while history.pop().is_some() {
            count += 1;
        }
        assert_eq!(count, MAX_ZOOM_HISTORY);
    }
}
