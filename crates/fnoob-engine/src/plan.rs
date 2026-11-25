use crate::audio_data::{AudioData, AudioPool};
use crate::session::Session;
use crate::synth::Synth;
use fnoob_core::model::{
    AudioInput, Automation, ChannelStrip, Id, Instrument, MAX_INSERTS, MAX_RETURNS, Output,
    PluginSlot, Project, RegionContent, TrackKind, db_to_gain, pan_gains,
};
use fnoob_core::time::{Timeline, ticks_per_beat};
use std::collections::HashMap;
use std::sync::Arc;

pub const MAX_BLOCK: usize = 2048;
pub const MAX_PLUGINS: usize = 256;
pub const NO_OFF: u64 = u64::MAX;

pub type PluginMap = HashMap<Id, usize>;
pub type Inserts = [Option<(usize, bool)>; MAX_INSERTS];

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
    pub strip: StripPlan,
    pub envelopes: Envelopes,
    pub notes: Vec<PlannedNote>,
    pub synth: Synth,
    pub session: Session,
    pub instrument: Option<usize>,
    pub pending_offs: [u64; 128],
    pub next_off: u64,
    pub cursor: usize,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub start: u64,
    pub end: u64,
    pub offset: u64,
    pub gain: f32,
    pub fade_in: u64,
    pub fade_out: u64,
    pub data: Arc<AudioData>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Envelopes {
    pub volume: Vec<(u64, f32)>,
    pub pan: Vec<(u64, f32)>,
}

pub fn envelope_at(points: &[(u64, f32)], sample: u64, default: f32) -> f32 {
    let next = points.partition_point(|p| p.0 <= sample);
    match (next.checked_sub(1).map(|i| points[i]), points.get(next)) {
        (None, None) => default,
        (None, Some(first)) => first.1,
        (Some(last), None) => last.1,
        (Some(a), Some(b)) => {
            let span = (b.0 - a.0) as f32;
            let t = if span > 0.0 {
                (sample - a.0) as f32 / span
            } else {
                1.0
            };
            a.1 + (b.1 - a.1) * t
        }
    }
}

impl Envelopes {
    fn new(automation: &Automation, timeline: Timeline) -> Self {
        let convert = |points: &[fnoob_core::model::AutomationPoint]| {
            points
                .iter()
                .map(|p| (timeline.tick_to_sample(p.tick), p.value))
                .collect()
        };
        Self {
            volume: convert(&automation.volume),
            pan: convert(&automation.pan),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.volume.is_empty() && self.pan.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioTrackPlan {
    pub id: Id,
    pub audible: bool,
    pub strip: StripPlan,
    pub envelopes: Envelopes,
    pub clips: Vec<Clip>,
    pub session: Session,
    pub cursor: usize,
    pub input: AudioInput,
    pub monitor: bool,
    pub record: bool,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    pub samples_per_tick: f64,
    pub samples_per_beat: f64,
    pub timeline: Timeline,
    pub meter: fnoob_core::time::MeterMap,
    pub ticks_per_beat: u64,
    pub beats_per_bar: u64,
    pub tempo: f64,
    pub numerator: i32,
    pub denominator: i32,
    pub metronome: bool,
    pub count_in_beats: u64,
    pub loop_range: Option<(u64, u64)>,
    pub launch_quantum: u64,
    pub record_window: (u64, u64),
    pub tracks: Vec<TrackPlan>,
    pub audio_tracks: Vec<AudioTrackPlan>,
    pub records_audio: bool,
    pub returns: Vec<ReturnPlan>,
    pub master: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReturnPlan {
    pub gain: (f32, f32),
    pub inserts: Inserts,
    pub meter: usize,
}

pub const TRACK_METERS: usize = 256;
pub const MASTER_METER: usize = TRACK_METERS + MAX_RETURNS;
pub const METER_SLOTS: usize = MASTER_METER + 1;

pub const fn return_meter(index: usize) -> usize {
    TRACK_METERS + index
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripPlan {
    pub volume_db: f32,
    pub pan: f32,
    pub left: f32,
    pub right: f32,
    pub sends: [f32; MAX_RETURNS],
    pub pre_fader: [bool; MAX_RETURNS],
    pub to_main: bool,
    pub meter: usize,
    pub inserts: Inserts,
}

fn inserts(slots: &[PluginSlot], plugins: &PluginMap) -> Inserts {
    let mut result = [None; MAX_INSERTS];
    for (target, slot) in result.iter_mut().zip(slots) {
        *target = plugins.get(&slot.id).map(|&index| (index, slot.bypass));
    }
    result
}

impl StripPlan {
    pub fn new(strip: &ChannelStrip, meter: usize, plugins: &PluginMap, to_main: bool) -> Self {
        let gain = db_to_gain(strip.volume_db);
        let (left, right) = pan_gains(strip.pan);
        Self {
            volume_db: strip.volume_db,
            pan: strip.pan,
            left: gain * left,
            right: gain * right,
            sends: std::array::from_fn(|i| db_to_gain(strip.send_at(i).level_db)),
            pre_fader: std::array::from_fn(|i| strip.send_at(i).pre_fader),
            to_main,
            meter: if meter < TRACK_METERS {
                meter
            } else {
                METER_SLOTS
            },
            inserts: inserts(&strip.inserts, plugins),
        }
    }

    pub fn at(&self, envelopes: &Envelopes, sample: u64) -> StripPlan {
        if envelopes.is_empty() {
            return *self;
        }
        let volume = envelope_at(&envelopes.volume, sample, self.volume_db);
        let pan = envelope_at(&envelopes.pan, sample, self.pan);
        let gain = db_to_gain(if volume <= -60.0 { -144.0 } else { volume });
        let (left, right) = pan_gains(pan);
        StripPlan {
            left: gain * left,
            right: gain * right,
            ..*self
        }
    }
}

fn audio_track_plans(
    project: &Project,
    timeline: Timeline,
    pool: &AudioPool,
    plugins: &PluginMap,
) -> Vec<AudioTrackPlan> {
    project
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind == TrackKind::Audio)
        .map(|(index, track)| {
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
                    let fade = |ticks: u64| {
                        timeline.tick_to_sample(region.start_tick + ticks)
                            - timeline.tick_to_sample(region.start_tick)
                    };
                    let start = timeline.tick_to_sample(region.start_tick);
                    let end = timeline.tick_to_sample(region.end_tick());
                    (end > start && offset_frames < data.frames).then(|| Clip {
                        start,
                        end,
                        offset: offset_frames,
                        gain: 10f32.powf(gain_db / 20.0),
                        fade_in: fade(region.fade_in_tick),
                        fade_out: fade(region.fade_out_tick),
                        data: Arc::clone(data),
                    })
                })
                .collect();
            clips.sort_by_key(|c| c.start);
            AudioTrackPlan {
                id: track.id,
                audible,
                strip: StripPlan::new(&track.strip, index, plugins, track.output == Output::Main),
                envelopes: Envelopes::new(&track.automation, timeline),
                clips,
                session: Session::build(track, timeline, pool),
                cursor: 0,
                input: track.audio_input.unwrap_or_default(),
                monitor: project.monitors(track) && audible,
                record: track.record_enabled,
                left: vec![0.0; MAX_BLOCK],
                right: vec![0.0; MAX_BLOCK],
            }
        })
        .collect()
}

fn track_plans(
    project: &Project,
    timeline: Timeline,
    pool: &AudioPool,
    plugins: &PluginMap,
) -> Vec<TrackPlan> {
    project
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind == TrackKind::Instrument)
        .map(|(index, track)| {
            let instrument = match &track.instrument {
                Some(Instrument::Vst3(slot)) => plugins.get(&slot.id).copied(),
                _ => None,
            };
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
            TrackPlan {
                id: track.id,
                audible,
                strip: StripPlan::new(&track.strip, index, plugins, track.output == Output::Main),
                envelopes: Envelopes::new(&track.automation, timeline),
                notes,
                session: Session::build(track, timeline, pool),
                synth: Synth::for_instrument(track.instrument.as_ref(), timeline.sample_rate, pool),
                instrument,
                pending_offs: [NO_OFF; 128],
                next_off: NO_OFF,
                cursor: 0,
                left: vec![0.0; MAX_BLOCK],
                right: vec![0.0; MAX_BLOCK],
            }
        })
        .collect()
}

impl RenderPlan {
    pub fn from_project(project: &Project, sample_rate: u32) -> Self {
        Self::build(project, sample_rate, &AudioPool::new(), &PluginMap::new())
    }

    pub fn with_audio(project: &Project, sample_rate: u32, pool: &AudioPool) -> Self {
        Self::build(project, sample_rate, pool, &PluginMap::new())
    }

    pub fn build(
        project: &Project,
        sample_rate: u32,
        pool: &AudioPool,
        plugins: &PluginMap,
    ) -> Self {
        let timeline = project.timeline(sample_rate);
        let samples_per_tick = timeline.samples_per_tick();
        let ts = project.time_signature;
        let beats_per_bar = u64::from(ts.numerator.max(1));
        let loop_range = project.loop_range;
        Self {
            samples_per_tick,
            samples_per_beat: ticks_per_beat(ts) as f64 * samples_per_tick,
            timeline,
            meter: project.meter(),
            ticks_per_beat: ticks_per_beat(ts).max(1),
            beats_per_bar,
            tempo: project.tempo_bpm,
            numerator: ts.numerator as i32,
            denominator: ts.denominator as i32,
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
            launch_quantum: (project.launch_quantize.ticks(ts) as f64 * samples_per_tick).round()
                as u64,
            record_window: (
                if project.punch.punch_in {
                    timeline.tick_to_sample(loop_range.start_tick)
                } else {
                    0
                },
                if project.punch.punch_out {
                    timeline.tick_to_sample(loop_range.end_tick)
                } else {
                    u64::MAX
                },
            ),
            tracks: track_plans(project, timeline, pool, plugins),
            audio_tracks: audio_track_plans(project, timeline, pool, plugins),
            returns: project
                .returns
                .iter()
                .take(MAX_RETURNS)
                .enumerate()
                .map(|(index, ret)| {
                    let gain = if ret.mute {
                        0.0
                    } else {
                        db_to_gain(ret.volume_db)
                    };
                    let (left, right) = pan_gains(ret.pan);
                    ReturnPlan {
                        gain: (gain * left, gain * right),
                        inserts: inserts(&ret.inserts, plugins),
                        meter: return_meter(index),
                    }
                })
                .collect(),
            master: db_to_gain(project.master.volume_db),
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
