use crate::audio_data::{AudioData, AudioPool};
use crate::synth::Synth;
use fnoob_core::model::{AudioInput, Id, Project, RegionContent, TrackKind};
use fnoob_core::time::{Timeline, ticks_per_beat};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedNote {
    pub start: u64,
    pub end: u64,
    pub pitch: u8,
    pub velocity: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackPlan {
    pub id: Id,
    pub audible: bool,
    pub notes: Vec<PlannedNote>,
    pub synth: Synth,
    pub cursor: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub start: u64,
    pub end: u64,
    pub offset: u64,
    pub gain: f32,
    pub data: Arc<AudioData>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioTrackPlan {
    pub id: Id,
    pub audible: bool,
    pub clips: Vec<Clip>,
    pub cursor: usize,
    pub input: AudioInput,
    pub monitor: bool,
    pub record: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    pub samples_per_tick: f64,
    pub samples_per_beat: f64,
    pub beats_per_bar: u64,
    pub metronome: bool,
    pub count_in_beats: u64,
    pub loop_range: Option<(u64, u64)>,
    pub tracks: Vec<TrackPlan>,
    pub audio_tracks: Vec<AudioTrackPlan>,
    pub records_audio: bool,
}

fn audio_track_plans(
    project: &Project,
    timeline: Timeline,
    pool: &AudioPool,
) -> Vec<AudioTrackPlan> {
    project
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .map(|track| {
            let audible = project.is_track_audible(track);
            let mut clips: Vec<Clip> = track
                .regions
                .iter()
                .filter(|r| audible && !r.muted)
                .filter_map(|region| {
                    let RegionContent::Audio {
                        file_id,
                        offset_frames,
                        gain_db,
                    } = region.content
                    else {
                        return None;
                    };
                    let data = pool.get(&file_id)?;
                    let start = timeline.tick_to_sample(region.start_tick);
                    let end = timeline.tick_to_sample(region.end_tick());
                    (end > start && offset_frames < data.frames).then(|| Clip {
                        start,
                        end,
                        offset: offset_frames,
                        gain: 10f32.powf(gain_db / 20.0),
                        data: Arc::clone(data),
                    })
                })
                .collect();
            clips.sort_by_key(|c| c.start);
            AudioTrackPlan {
                id: track.id,
                audible,
                clips,
                cursor: 0,
                input: track.audio_input.unwrap_or_default(),
                monitor: track.input_monitoring && audible,
                record: track.record_enabled,
            }
        })
        .collect()
}

fn track_plans(project: &Project, timeline: Timeline) -> Vec<TrackPlan> {
    project
        .tracks
        .iter()
        .filter_map(|track| {
            let params = track.synth()?;
            let audible = project.is_track_audible(track);
            let mut notes: Vec<PlannedNote> = if audible {
                track
                    .regions
                    .iter()
                    .filter(|r| !r.muted)
                    .flat_map(|region| {
                        region
                            .notes()
                            .iter()
                            .filter(|n| n.start_tick < region.length_tick)
                            .map(move |n| {
                                let start = region.start_tick + n.start_tick;
                                let end = (start + n.length_tick).min(region.end_tick());
                                PlannedNote {
                                    start: timeline.tick_to_sample(start),
                                    end: timeline.tick_to_sample(end),
                                    pitch: n.pitch,
                                    velocity: n.velocity,
                                }
                            })
                    })
                    .filter(|n| n.end > n.start)
                    .collect()
            } else {
                Vec::new()
            };
            notes.sort_unstable_by_key(|n| (n.start, n.pitch));
            Some(TrackPlan {
                id: track.id,
                audible,
                notes,
                synth: Synth::new(&params, timeline.sample_rate),
                cursor: 0,
            })
        })
        .collect()
}

impl RenderPlan {
    pub fn from_project(project: &Project, sample_rate: u32) -> Self {
        Self::with_audio(project, sample_rate, &AudioPool::new())
    }

    pub fn with_audio(project: &Project, sample_rate: u32, pool: &AudioPool) -> Self {
        let timeline = Timeline {
            sample_rate,
            bpm: project.tempo_bpm,
        };
        let samples_per_tick = timeline.samples_per_tick();
        let ts = project.time_signature;
        let beats_per_bar = u64::from(ts.numerator.max(1));
        let loop_range = project.loop_range;
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
            tracks: track_plans(project, timeline),
            audio_tracks: audio_track_plans(project, timeline, pool),
            records_audio: project
                .tracks
                .iter()
                .any(|t| t.kind == TrackKind::Audio && t.record_enabled),
        }
    }

    pub fn idle(sample_rate: u32) -> Self {
        Self::from_project(&Project::new("", sample_rate), sample_rate)
    }

    pub fn track_mut(&mut self, id: Id) -> Option<&mut TrackPlan> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnoob_core::edit::{add_midi_region, add_note, add_track};
    use fnoob_core::model::{Note, TimeSignature, TrackKind};

    fn note(pitch: u8, start_tick: u64, length_tick: u64) -> Note {
        Note {
            pitch,
            velocity: 100,
            start_tick,
            length_tick,
        }
    }

    #[test]
    fn notes_are_placed_in_samples_and_clipped_to_region() {
        let mut p = Project::new("t", 48_000);
        let audio = add_track(&mut p, TrackKind::Audio);
        let inst = add_track(&mut p, TrackKind::Instrument);
        let region = add_midi_region(&mut p, inst, 960, 960).unwrap();
        add_note(&mut p, region, note(64, 480, 960));
        add_note(&mut p, region, note(60, 0, 240));
        add_note(&mut p, region, note(62, 960, 240));
        let plan = RenderPlan::from_project(&p, 48_000);
        assert_eq!(plan.tracks.len(), 1);
        assert_ne!(plan.tracks[0].id, audio);
        let starts: Vec<(u64, u64, u8)> = plan.tracks[0]
            .notes
            .iter()
            .map(|n| (n.start, n.end, n.pitch))
            .collect();
        assert_eq!(starts, [(24_000, 30_000, 60), (36_000, 48_000, 64)]);
    }

    #[test]
    fn muted_regions_and_tracks_have_no_notes() {
        let mut p = Project::new("t", 48_000);
        let a = add_track(&mut p, TrackKind::Instrument);
        let b = add_track(&mut p, TrackKind::Instrument);
        let ra = add_midi_region(&mut p, a, 0, 960).unwrap();
        let rb = add_midi_region(&mut p, b, 0, 960).unwrap();
        add_note(&mut p, ra, note(60, 0, 100));
        add_note(&mut p, rb, note(60, 0, 100));
        let counts = |p: &Project| -> Vec<usize> {
            RenderPlan::from_project(p, 48_000)
                .tracks
                .iter()
                .map(|t| t.notes.len())
                .collect()
        };
        assert_eq!(counts(&p), [1, 1]);
        p.tracks[1].solo = true;
        assert_eq!(counts(&p), [0, 1]);
        p.tracks[1].regions[0].muted = true;
        assert_eq!(counts(&p), [0, 0]);
        let plan = RenderPlan::from_project(&p, 48_000);
        assert!(!plan.tracks[0].audible && plan.tracks[1].audible);
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
