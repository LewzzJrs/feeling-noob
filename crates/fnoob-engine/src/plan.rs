use fnoob_core::model::{Project, TrackKind};
use fnoob_core::time::{Timeline, ticks_per_beat};

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    pub samples_per_tick: f64,
    pub samples_per_beat: f64,
    pub beats_per_bar: u64,
    pub metronome: bool,
    pub count_in_beats: u64,
    pub loop_range: Option<(u64, u64)>,
    pub beeps: Vec<u64>,
}

impl RenderPlan {
    pub fn from_project(project: &Project, sample_rate: u32) -> Self {
        let timeline = Timeline {
            sample_rate,
            bpm: project.tempo_bpm,
        };
        let samples_per_tick = timeline.samples_per_tick();
        let ts = project.time_signature;
        let beats_per_bar = u64::from(ts.numerator.max(1));
        let loop_range = project.loop_range;
        let mut beeps: Vec<u64> = project
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Instrument && project.is_track_audible(t))
            .flat_map(|t| t.regions.iter())
            .filter(|r| !r.muted)
            .map(|r| timeline.tick_to_sample(r.start_tick))
            .collect();
        beeps.sort_unstable();
        beeps.dedup();
        Self {
            samples_per_tick,
            samples_per_beat: ticks_per_beat(ts) as f64 * samples_per_tick,
            beats_per_bar,
            metronome: project.metronome.enabled,
            count_in_beats: if project.metronome.count_in_enabled {
                u64::from(project.metronome.count_in_bars) * beats_per_bar
            } else {
                0
            },
            loop_range: loop_range
                .enabled
                .then(|| {
                    (
                        timeline.tick_to_sample(loop_range.start_tick),
                        timeline.tick_to_sample(loop_range.end_tick),
                    )
                })
                .filter(|(start, end)| end > start),
            beeps,
        }
    }

    pub fn idle(sample_rate: u32) -> Self {
        Self::from_project(&Project::new("", sample_rate), sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnoob_core::edit::{add_midi_region, add_track};
    use fnoob_core::model::TimeSignature;

    #[test]
    fn beeps_follow_audible_instrument_regions() {
        let mut p = Project::new("t", 48_000);
        let a = add_track(&mut p, TrackKind::Instrument);
        let b = add_track(&mut p, TrackKind::Instrument);
        add_midi_region(&mut p, a, 960, 960);
        let muted = add_midi_region(&mut p, a, 1920, 960).unwrap();
        add_midi_region(&mut p, b, 3840, 960);
        p.tracks[0]
            .regions
            .iter_mut()
            .find(|r| r.id == muted)
            .unwrap()
            .muted = true;
        assert_eq!(RenderPlan::from_project(&p, 48_000).beeps, [24_000, 96_000]);
        p.tracks[1].solo = true;
        assert_eq!(RenderPlan::from_project(&p, 48_000).beeps, [96_000]);
        p.tracks[1].mute = true;
        assert!(RenderPlan::from_project(&p, 48_000).beeps.is_empty());
    }

    #[test]
    fn beat_follows_time_signature_denominator() {
        let mut p = Project::new("t", 48_000);
        p.time_signature = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        p.metronome.count_in_enabled = true;
        p.metronome.count_in_bars = 2;
        let plan = RenderPlan::from_project(&p, 48_000);
        assert_eq!(plan.samples_per_beat, 12_000.0);
        assert_eq!(plan.beats_per_bar, 6);
        assert_eq!(plan.count_in_beats, 12);
    }

    #[test]
    fn loop_in_samples_only_when_enabled() {
        let mut p = Project::new("t", 48_000);
        assert_eq!(RenderPlan::from_project(&p, 48_000).loop_range, None);
        p.loop_range.enabled = true;
        assert_eq!(
            RenderPlan::from_project(&p, 48_000).loop_range,
            Some((0, 4 * 96_000))
        );
    }
}
