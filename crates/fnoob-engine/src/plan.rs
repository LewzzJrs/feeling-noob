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
    pub chance: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackPlan {
    pub id: Id,
    pub audible: bool,
    pub strip: StripPlan,
    pub envelopes: Envelopes,
    pub device_automation: Vec<DeviceAutomation>,
    pub keys: Vec<(usize, Node)>,
    pub notes: Vec<PlannedNote>,
    pub synth: Synth,
    pub session: Session,
    pub instrument: Option<usize>,
    pub midi_fx: Vec<fnoob_core::midi_fx::MidiDevice>,
    pub arp: Option<crate::arp::Arp>,
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
    pub loop_len: u64,
    pub data: Arc<AudioData>,
    pub warp: Option<ClipWarp>,
    pub fade_in_curve: f32,
    pub fade_out_curve: f32,
    pub xfade_in: u64,
    pub xfade_from: u64,
    pub xfade_out: u64,
}

fn crossfade(clips: &mut [Clip]) {
    for index in 1..clips.len() {
        let (before, after) = clips.split_at_mut(index);
        let (a, b) = (&mut before[index - 1], &mut after[0]);
        if b.start < a.end && b.end > a.end {
            let overlap = a.end - b.start;
            let length = match b.fade_in {
                0 => overlap,
                fade => fade.min(overlap),
            };
            a.xfade_from = b.start;
            a.xfade_out = length;
            b.xfade_in = length;
            b.fade_in = 0;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipWarp {
    pub repitch: bool,
    pub start_tick: f64,
    pub origin: f64,
    pub loop_ticks: f64,
    pub map: fnoob_core::model::WarpMap,
    pub timeline: Timeline,
}

impl ClipWarp {
    pub fn source_at(&self, sample: u64) -> f64 {
        let mut ticks = self.timeline.sample_to_tick_f64(sample as f64) - self.start_tick;
        if self.loop_ticks > 0.0 {
            ticks = ticks.rem_euclid(self.loop_ticks);
        }
        self.map.frame(self.origin + ticks)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Envelopes {
    pub volume: Vec<(u64, f32, f32)>,
    pub pan: Vec<(u64, f32, f32)>,
    pub sends: Vec<Vec<(u64, f32, f32)>>,
}

pub type Envelope = Vec<(u64, f32, f32)>;

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceAutomation {
    pub slot: usize,
    pub base: Option<fnoob_core::effects::BuiltinEffect>,
    pub lanes: Vec<(u32, Envelope)>,
}

fn device_automation(
    track: &fnoob_core::model::Track,
    timeline: Timeline,
    plugins: &PluginMap,
) -> Vec<DeviceAutomation> {
    if track.automation_off {
        return Vec::new();
    }
    track
        .strip
        .inserts
        .iter()
        .filter_map(|slot| {
            let base = slot.builtin;
            let index = *plugins.get(&slot.id)?;
            let lanes: Vec<(u32, Envelope)> = track
                .automation
                .devices
                .iter()
                .filter(|l| l.slot == slot.id && !l.points.is_empty())
                .map(|l| {
                    (
                        l.param,
                        l.points
                            .iter()
                            .map(|p| (timeline.tick_to_sample(p.tick), p.value, p.curve))
                            .collect(),
                    )
                })
                .collect();
            (!lanes.is_empty()).then_some(DeviceAutomation {
                slot: index,
                base,
                lanes,
            })
        })
        .collect()
}

pub fn envelope_at(points: &[(u64, f32, f32)], sample: u64, default: f32) -> f32 {
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
            a.1 + (b.1 - a.1) * fnoob_core::model::shape(t, a.2)
        }
    }
}

impl Envelopes {
    fn for_track(track: &fnoob_core::model::Track, timeline: Timeline) -> Self {
        if track.automation_off {
            Self::default()
        } else {
            Self::new(&track.automation, timeline)
        }
    }

    fn new(automation: &Automation, timeline: Timeline) -> Self {
        let convert = |points: &[fnoob_core::model::AutomationPoint]| -> Vec<(u64, f32, f32)> {
            points
                .iter()
                .map(|p| (timeline.tick_to_sample(p.tick), p.value, p.curve))
                .collect()
        };
        Self {
            volume: convert(&automation.volume),
            pan: convert(&automation.pan),
            sends: automation
                .sends
                .iter()
                .take(MAX_RETURNS)
                .map(|lane| convert(lane))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.volume.is_empty() && self.pan.is_empty() && self.sends.iter().all(Vec::is_empty)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioTrackPlan {
    pub id: Id,
    pub audible: bool,
    pub strip: StripPlan,
    pub envelopes: Envelopes,
    pub device_automation: Vec<DeviceAutomation>,
    pub keys: Vec<(usize, Node)>,
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
    pub groups: Vec<GroupPlan>,
    pub order: Vec<Node>,
    pub master: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Track(usize),
    Audio(usize),
}

fn sidechains(track: &fnoob_core::model::Track, plugins: &PluginMap) -> Vec<(usize, Id)> {
    track
        .strip
        .inserts
        .iter()
        .filter(|slot| slot.takes_sidechain())
        .filter_map(|slot| Some((*plugins.get(&slot.id)?, slot.sidechain?)))
        .filter(|(_, source)| *source != track.id)
        .collect()
}

fn processing_order(nodes: &[(Id, Node)], edges: &[(Node, Node)]) -> Vec<Node> {
    let mut order: Vec<Node> = Vec::with_capacity(nodes.len());
    while order.len() < nodes.len() {
        let waiting = || nodes.iter().map(|(_, n)| *n).filter(|n| !order.contains(n));
        let ready = waiting().find(|node| {
            edges
                .iter()
                .filter(|(from, to)| to == node && from != to)
                .all(|(from, _)| order.contains(from))
        });
        match ready.or_else(|| waiting().next()) {
            Some(node) => order.push(node),
            None => break,
        }
    }
    order
}

fn resolve_routing(plan: &mut RenderPlan, project: &Project, plugins: &PluginMap) {
    let nodes: Vec<(Id, Node)> = plan
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id, Node::Track(i)))
        .chain(
            plan.audio_tracks
                .iter()
                .enumerate()
                .map(|(i, t)| (t.id, Node::Audio(i))),
        )
        .collect();
    let node_of = |id: Id| nodes.iter().find(|(n, _)| *n == id).map(|(_, node)| *node);
    let mut edges = Vec::new();
    let mut routes = Vec::new();
    let mut keys = Vec::new();
    for (id, node) in &nodes {
        let Some(track) = project.track(*id) else {
            continue;
        };
        if let Output::Track(target) = track.output
            && target != *id
            && let Some(Node::Audio(j)) = node_of(target)
        {
            edges.push((*node, Node::Audio(j)));
            routes.push((*node, j));
        }
        for (slot, source) in sidechains(track, plugins) {
            if let Some(source) = node_of(source) {
                edges.push((source, *node));
                keys.push((*node, slot, source));
            }
        }
    }
    let order = processing_order(&nodes, &edges);
    let pos = |n: Node| order.iter().position(|o| *o == n);
    for (source, target) in routes {
        if pos(source) < pos(Node::Audio(target)) {
            match source {
                Node::Track(i) => plan.tracks[i].strip.route = Some(target),
                Node::Audio(i) => plan.audio_tracks[i].strip.route = Some(target),
            }
        }
    }
    for (user, slot, source) in keys {
        if pos(source) < pos(user) {
            match user {
                Node::Track(i) => plan.tracks[i].keys.push((slot, source)),
                Node::Audio(i) => plan.audio_tracks[i].keys.push((slot, source)),
            }
        }
    }
    for group in &mut plan.groups {
        if let Some(track) = project.track(group.id) {
            group.keys = sidechains(track, plugins)
                .into_iter()
                .filter_map(|(slot, source)| Some((slot, node_of(source)?)))
                .collect();
        }
    }
    plan.order = order;
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
    pub group: Option<usize>,
    pub route: Option<usize>,
    pub meter: usize,
    pub inserts: Inserts,
}

pub const MAX_GROUPS: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub struct GroupPlan {
    pub id: Id,
    pub audible: bool,
    pub strip: StripPlan,
    pub envelopes: Envelopes,
    pub device_automation: Vec<DeviceAutomation>,
    pub keys: Vec<(usize, Node)>,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
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
            group: None,
            route: None,
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
        let mut sends = self.sends;
        for (target, lane) in sends.iter_mut().zip(&envelopes.sends) {
            if !lane.is_empty() {
                let db = envelope_at(lane, sample, 0.0);
                *target = db_to_gain(if db <= -60.0 { -144.0 } else { db });
            }
        }
        StripPlan {
            left: gain * left,
            right: gain * right,
            sends,
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
        .filter(|(_, t)| t.kind == TrackKind::Audio || t.frozen.is_some())
        .map(|(index, track)| {
            let audible = project.is_track_audible(track);
            if let Some(file) = track.frozen {
                return frozen_plan(project, track, index, file, timeline, pool, plugins);
            }
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
                    let warp = region.warp.map(|w| {
                        let map = fnoob_core::model::WarpMap::new(
                            w,
                            &region.warp_markers,
                            timeline.sample_rate,
                        );
                        ClipWarp {
                            repitch: w.mode == fnoob_core::model::WarpMode::Repitch,
                            start_tick: region.start_tick as f64,
                            origin: map.tick(offset_frames as f64),
                            loop_ticks: region.loop_ticks.map_or(0.0, |t| t as f64),
                            map,
                            timeline,
                        }
                    });
                    (end > start && offset_frames < data.frames).then(|| Clip {
                        start,
                        end,
                        offset: offset_frames,
                        gain: 10f32.powf(gain_db / 20.0),
                        fade_in: fade(region.fade_in_tick),
                        fade_out: fade(region.fade_out_tick),
                        loop_len: if warp.is_some() {
                            0
                        } else {
                            region.loop_ticks.map_or(0, fade)
                        },
                        data: Arc::clone(data),
                        warp,
                        fade_in_curve: region.fade_in_curve,
                        fade_out_curve: region.fade_out_curve,
                        xfade_in: 0,
                        xfade_from: 0,
                        xfade_out: 0,
                    })
                })
                .collect();
            clips.sort_by_key(|c| c.start);
            if project.crossfade_overlaps {
                crossfade(&mut clips);
            }
            AudioTrackPlan {
                id: track.id,
                audible,
                strip: member_strip(project, track, index, plugins),
                envelopes: Envelopes::for_track(track, timeline),
                device_automation: device_automation(track, timeline, plugins),
                keys: Vec::new(),
                clips,
                session: Session::build(track, timeline, pool, project.time_signature),
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

fn frozen_plan(
    project: &Project,
    track: &fnoob_core::model::Track,
    index: usize,
    file: Id,
    timeline: Timeline,
    pool: &AudioPool,
    plugins: &PluginMap,
) -> AudioTrackPlan {
    let audible = project.is_track_audible(track);
    let clips = pool
        .get(&file)
        .filter(|_| audible)
        .map(|data| Clip {
            start: 0,
            end: data.frames,
            offset: 0,
            gain: 1.0,
            fade_in: 0,
            fade_out: 0,
            loop_len: 0,
            data: Arc::clone(data),
            warp: None,
            fade_in_curve: 0.0,
            fade_out_curve: 0.0,
            xfade_in: 0,
            xfade_from: 0,
            xfade_out: 0,
        })
        .into_iter()
        .collect();
    let mut strip = member_strip(project, track, index, plugins);
    strip.inserts = [None; MAX_INSERTS];
    AudioTrackPlan {
        id: track.id,
        audible,
        strip,
        envelopes: Envelopes::for_track(track, timeline),
        device_automation: device_automation(track, timeline, plugins),
        keys: Vec::new(),
        clips,
        session: Session::default(),
        cursor: 0,
        input: AudioInput::default(),
        monitor: false,
        record: false,
        left: vec![0.0; MAX_BLOCK],
        right: vec![0.0; MAX_BLOCK],
    }
}

fn group_ids(project: &Project) -> Vec<Id> {
    let mut groups: Vec<(usize, Id)> = project
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Group)
        .map(|t| (project.depth(t), t.id))
        .collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.0));
    groups
        .into_iter()
        .map(|(_, id)| id)
        .take(MAX_GROUPS)
        .collect()
}

fn member_strip(
    project: &Project,
    track: &fnoob_core::model::Track,
    index: usize,
    plugins: &PluginMap,
) -> StripPlan {
    let mut strip = StripPlan::new(&track.strip, index, plugins, track.output != Output::None);
    strip.group = track
        .group
        .and_then(|g| group_ids(project).iter().position(|id| *id == g));
    strip
}

fn group_plans(project: &Project, timeline: Timeline, plugins: &PluginMap) -> Vec<GroupPlan> {
    let ids = group_ids(project);
    let mut plans = group_plans_unsorted(project, timeline, plugins, &ids);
    plans.sort_by_key(|g| ids.iter().position(|id| *id == g.id).unwrap_or(usize::MAX));
    plans
}

fn group_plans_unsorted(
    project: &Project,
    timeline: Timeline,
    plugins: &PluginMap,
    ids: &[Id],
) -> Vec<GroupPlan> {
    project
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| ids.contains(&t.id))
        .map(|(index, track)| GroupPlan {
            id: track.id,
            audible: project.is_track_audible(track),
            strip: member_strip(project, track, index, plugins),
            envelopes: Envelopes::for_track(track, timeline),
            device_automation: device_automation(track, timeline, plugins),
            keys: Vec::new(),
            left: vec![0.0; MAX_BLOCK],
            right: vec![0.0; MAX_BLOCK],
        })
        .collect()
}

pub fn apply_midi_fx(
    effects: &[fnoob_core::midi_fx::MidiDevice],
    notes: Vec<PlannedNote>,
) -> Vec<PlannedNote> {
    if effects.is_empty() {
        return notes;
    }
    let mut out = [0u8; fnoob_core::midi_fx::MAX_OUT];
    let mut result = Vec::with_capacity(notes.len());
    for note in notes {
        let count = fnoob_core::midi_fx::pitches(effects, note.pitch, &mut out);
        for &pitch in &out[..count] {
            result.push(PlannedNote { pitch, ..note });
        }
    }
    result
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
        .filter(|(_, t)| t.kind == TrackKind::Instrument && t.frozen.is_none())
        .map(|(index, track)| {
            let instrument = match &track.instrument {
                Some(Instrument::Vst3(slot)) => plugins.get(&slot.id).copied(),
                _ => None,
            };
            let audible = project.is_track_audible(track);
            let notes: Vec<PlannedNote> = if audible {
                track
                    .regions
                    .iter()
                    .filter(|r| !r.muted)
                    .flat_map(|region| {
                        let span = region.loop_ticks.unwrap_or(region.length_tick).max(1);
                        let passes = region.length_tick.div_ceil(span);
                        (0..passes).flat_map(move |pass| {
                            let offset = pass * span;
                            region
                                .notes()
                                .iter()
                                .filter(move |n| {
                                    n.start_tick < span
                                        && offset + n.start_tick < region.length_tick
                                })
                                .map(move |n| {
                                    let start = region.start_tick + offset + n.start_tick;
                                    let stop = if region.loop_ticks.is_some() {
                                        region.start_tick + offset + span
                                    } else {
                                        region.end_tick()
                                    };
                                    let end =
                                        (start + n.length_tick).min(region.end_tick()).min(stop);
                                    PlannedNote {
                                        start: timeline.tick_to_sample(start),
                                        end: timeline.tick_to_sample(end),
                                        pitch: n.pitch,
                                        velocity: n.velocity,
                                        chance: n.chance,
                                    }
                                })
                        })
                    })
                    .filter(|n| n.end > n.start)
                    .collect()
            } else {
                Vec::new()
            };
            let notes = apply_midi_fx(&track.midi_effects, notes);
            let mut notes = notes;
            notes.sort_unstable_by_key(|n| (n.start, n.pitch));
            let arp = fnoob_core::midi_fx::arpeggiator(&track.midi_effects).map(
                |(style, rate, octaves, gate)| {
                    let step = fnoob_core::time::grid_ticks(rate, project.time_signature) as f64
                        * timeline.samples_per_tick();
                    crate::arp::Arp::new(style, step.round() as u64, octaves, gate)
                },
            );
            TrackPlan {
                midi_fx: track.midi_effects.clone(),
                arp,
                id: track.id,
                audible,
                strip: member_strip(project, track, index, plugins),
                envelopes: Envelopes::for_track(track, timeline),
                device_automation: device_automation(track, timeline, plugins),
                keys: Vec::new(),
                notes,
                session: Session::build(track, timeline, pool, project.time_signature),
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
        let mut plan = Self {
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
            groups: group_plans(project, timeline, plugins),
            order: Vec::new(),
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
        };
        resolve_routing(&mut plan, project, plugins);
        plan
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
            chance: 100,
        }
    }

    #[test]
    fn send_automation_sets_the_send_level() {
        let mut p = Project::new("t", 48_000);
        fnoob_core::edit::add_return(&mut p, "A", None);
        let inst = add_track(&mut p, TrackKind::Instrument);
        fnoob_core::edit::add_automation_point(
            &mut p,
            inst,
            fnoob_core::model::AutomationParam::Send(0),
            0,
            0.0,
        );
        let plan = RenderPlan::from_project(&p, 48_000);
        let track = &plan.tracks[0];
        assert_eq!(track.strip.sends[0], 0.0);
        let strip = track.strip.at(&track.envelopes, 100);
        assert!((strip.sends[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn overridden_automation_is_ignored() {
        let mut p = Project::new("t", 48_000);
        let inst = add_track(&mut p, TrackKind::Instrument);
        fnoob_core::edit::add_automation_point(
            &mut p,
            inst,
            fnoob_core::model::AutomationParam::Volume,
            0,
            -12.0,
        );
        assert!(
            !RenderPlan::from_project(&p, 48_000).tracks[0]
                .envelopes
                .is_empty()
        );
        p.track_mut(inst).unwrap().automation_off = true;
        assert!(
            RenderPlan::from_project(&p, 48_000).tracks[0]
                .envelopes
                .is_empty()
        );
    }

    #[test]
    fn midi_effects_shape_the_planned_notes() {
        use fnoob_core::midi_fx::{MidiDevice, MidiEffect};
        let mut p = Project::new("t", 48_000);
        let inst = add_track(&mut p, TrackKind::Instrument);
        let region = add_midi_region(&mut p, inst, 0, 960).unwrap();
        add_note(&mut p, region, note(61, 0, 100));
        p.track_mut(inst).unwrap().midi_effects = MidiEffect::all()
            .into_iter()
            .map(|effect| MidiDevice {
                effect,
                bypass: false,
            })
            .collect();
        p.track_mut(inst).unwrap().midi_effects.swap(0, 1);
        let plan = RenderPlan::from_project(&p, 48_000);
        let pitches: Vec<u8> = plan.tracks[0].notes.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![60, 64, 67]);
        assert!(plan.tracks[0].arp.is_some());
    }

    #[test]
    fn looped_clips_repeat_their_notes() {
        let mut p = Project::new("t", 48_000);
        let inst = add_track(&mut p, TrackKind::Instrument);
        let region = add_midi_region(&mut p, inst, 0, 2400).unwrap();
        add_note(&mut p, region, note(60, 100, 2000));
        fnoob_core::edit::set_region_loop(&mut p, region, Some(960));
        let plan = RenderPlan::from_project(&p, 48_000);
        let spt = plan.samples_per_tick;
        let starts: Vec<u64> = plan.tracks[0].notes.iter().map(|n| n.start).collect();
        let at = |t: f64| (t * spt).round() as u64;
        assert_eq!(starts, vec![at(100.0), at(1060.0), at(2020.0)]);
        assert_eq!(plan.tracks[0].notes[0].end, at(960.0));
        assert_eq!(plan.tracks[0].notes[2].end, at(2400.0));
    }

    #[test]
    fn frozen_tracks_play_their_frozen_audio() {
        let mut p = Project::new("t", 48_000);
        let inst = add_track(&mut p, TrackKind::Instrument);
        add_midi_region(&mut p, inst, 0, 960).unwrap();
        let file = fnoob_core::edit::add_audio_file(&mut p, "f.wav".into(), 1, 100);
        p.track_mut(inst).unwrap().frozen = Some(file);
        let mut pool = AudioPool::new();
        pool.insert(
            file,
            Arc::new(crate::audio_data::AudioData::from_samples(
                1,
                vec![0.25; 100],
            )),
        );
        let plan = RenderPlan::with_audio(&p, 48_000, &pool);
        assert!(plan.tracks.is_empty());
        assert_eq!(plan.audio_tracks.len(), 1);
        assert_eq!(plan.audio_tracks[0].clips[0].end, 100);
        assert!(
            plan.audio_tracks[0]
                .strip
                .inserts
                .iter()
                .all(Option::is_none)
        );
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
