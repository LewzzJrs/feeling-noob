use crate::model::{
    AudioFile, AudioInput, AutomationParam, AutomationPoint, ChannelStrip, Id, Instrument,
    MAX_GAIN_DB, MIN_GAIN_DB, Marker, Note, Project, Region, RegionContent, TRACK_COLORS, Track,
    TrackKind,
};
use crate::time::{Timeline, snap_floor, snap_round};

pub fn add_track(project: &mut Project, kind: TrackKind) -> Id {
    let number = project.tracks.len() + 1;
    let id = project.alloc_id();
    let color = (project.tracks.len() % usize::from(TRACK_COLORS)) as u8;
    project.tracks.push(Track {
        id,
        name: format!("{number} {}", kind.default_name()),
        kind,
        color,
        mute: false,
        solo: false,
        record_enabled: false,
        input_monitoring: false,
        monitor: Default::default(),
        output: Default::default(),
        audio_input: (kind == TrackKind::Audio).then(AudioInput::default),
        instrument: (kind == TrackKind::Instrument).then(Instrument::default),
        strip: ChannelStrip::default(),
        automation: Default::default(),
        regions: Vec::new(),
        session: Vec::new(),
        takes: Vec::new(),
        group: None,
        folded: false,
        frozen: None,
        automation_off: false,
        no_stop: Vec::new(),
        midi_effects: Vec::new(),
    });
    id
}

pub fn flatten_track(project: &mut Project, track: Id) -> Option<Id> {
    let file = project.track(track)?.frozen?;
    let frames = project.audio_file(file)?.frames;
    let length_tick = frames_to_ticks_at(project, 0, frames);
    let name = project.track(track)?.name.clone();
    let id = project.alloc_id();
    let target = project.track_mut(track)?;
    target.kind = TrackKind::Audio;
    target.instrument = None;
    target.audio_input = Some(AudioInput::default());
    target.strip.inserts.clear();
    target.frozen = None;
    target.session.clear();
    target.takes.clear();
    target.regions = vec![Region {
        loop_ticks: None,
        id,
        name,
        start_tick: 0,
        length_tick,
        muted: false,
        fade_in_tick: 0,
        fade_out_tick: 0,
        content: RegionContent::Audio {
            file_id: file,
            offset_frames: 0,
            gain_db: 0.0,
        },
    }];
    Some(id)
}

pub fn strip_audio(project: &mut Project) {
    project.audio_files.clear();
    for track in &mut project.tracks {
        track
            .regions
            .retain(|r| !matches!(r.content, RegionContent::Audio { .. }));
        track.takes.clear();
        track.frozen = None;
        for slot in &mut track.session {
            if slot
                .as_ref()
                .is_some_and(|c| matches!(c.region.content, RegionContent::Audio { .. }))
            {
                *slot = None;
            }
        }
        match &mut track.instrument {
            Some(crate::model::Instrument::Sampler(params)) => params.file = None,
            Some(crate::model::Instrument::DrumSampler(params)) => {
                for pad in &mut params.pads {
                    pad.file = None;
                }
            }
            _ => {}
        }
    }
}

pub fn name_after_instrument(track: &mut Track, old_label: &str) -> bool {
    let new_label = track.instrument.as_ref().map_or_else(
        || TrackKind::Instrument.default_name().to_owned(),
        |i| i.label().to_owned(),
    );
    let Some((number, rest)) = track.name.split_once(' ') else {
        return false;
    };
    let numbered = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
    if numbered && (rest == TrackKind::Instrument.default_name() || rest == old_label) {
        track.name = format!("{number} {new_label}");
        return true;
    }
    false
}

pub fn set_region_loop(project: &mut Project, id: Id, loop_ticks: Option<u64>) -> bool {
    let Some(region) = region_mut(project, id) else {
        return false;
    };
    let value = loop_ticks
        .filter(|l| *l > 0)
        .map(|l| l.min(region.length_tick));
    let changed = region.loop_ticks != value;
    region.loop_ticks = value;
    changed
}

pub fn duplicate_loop(project: &mut Project, id: Id) -> bool {
    let Some(region) = region_mut(project, id) else {
        return false;
    };
    let span = region.loop_ticks.unwrap_or(region.length_tick);
    let RegionContent::Midi { notes } = &mut region.content else {
        return false;
    };
    let copies: Vec<Note> = notes
        .iter()
        .filter(|n| n.start_tick < span)
        .map(|n| Note {
            start_tick: n.start_tick + span,
            ..*n
        })
        .collect();
    notes.retain(|n| n.start_tick < span);
    notes.extend(copies);
    notes.sort_by_key(|n| (n.start_tick, n.pitch));
    if region.loop_ticks.is_some() {
        region.loop_ticks = Some(span * 2);
    }
    region.length_tick = region.length_tick.max(span * 2);
    true
}

fn spread(seed: u64, index: usize) -> f32 {
    let mut x = seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    (x >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
}

pub fn randomize_velocities(
    project: &mut Project,
    region: Id,
    indices: &[usize],
    amount: u8,
    seed: u64,
) -> bool {
    let Some(notes) = notes_of(project, region) else {
        return false;
    };
    let all: Vec<usize> = (0..notes.len()).collect();
    let chosen = if indices.is_empty() { &all } else { indices };
    for &i in chosen {
        if let Some(note) = notes.get_mut(i) {
            let shift = (spread(seed, i) * f32::from(amount)).round() as i32;
            note.velocity = (i32::from(note.velocity) + shift).clamp(1, 127) as u8;
        }
    }
    !chosen.is_empty()
}

pub fn ramp_velocities(
    project: &mut Project,
    region: Id,
    indices: &[usize],
    from: u8,
    to: u8,
) -> bool {
    let Some(notes) = notes_of(project, region) else {
        return false;
    };
    let all: Vec<usize> = (0..notes.len()).collect();
    let chosen: Vec<usize> = if indices.is_empty() {
        all
    } else {
        indices.to_vec()
    };
    let starts: Vec<u64> = chosen
        .iter()
        .filter_map(|&i| notes.get(i).map(|n| n.start_tick))
        .collect();
    let (Some(&first), Some(&last)) = (starts.iter().min(), starts.iter().max()) else {
        return false;
    };
    for &i in &chosen {
        if let Some(note) = notes.get_mut(i) {
            let t = if last > first {
                (note.start_tick - first) as f32 / (last - first) as f32
            } else {
                0.0
            };
            let value = f32::from(from) + (f32::from(to) - f32::from(from)) * t;
            note.velocity = value.round().clamp(1.0, 127.0) as u8;
        }
    }
    true
}

pub fn group_tracks(project: &mut Project, ids: &[Id]) -> Option<Id> {
    let parent = project
        .tracks
        .iter()
        .find(|t| ids.contains(&t.id))
        .and_then(|t| t.group);
    let members: Vec<Id> = project
        .tracks
        .iter()
        .filter(|t| ids.contains(&t.id) && t.group == parent)
        .map(|t| t.id)
        .collect();
    if members.is_empty() {
        return None;
    }
    let carried: Vec<Id> = project
        .tracks
        .iter()
        .filter(|t| members.contains(&t.id) || members.iter().any(|m| project.is_inside(t, *m)))
        .map(|t| t.id)
        .collect();
    let first = project
        .tracks
        .iter()
        .position(|t| members.contains(&t.id))?;
    let number = project
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Group)
        .count()
        + 1;
    let group = add_track(project, TrackKind::Group);
    let mut track = project.tracks.pop()?;
    track.name = format!("{number} Group");
    track.color = project.tracks[first].color;
    track.group = parent;
    let mut moved: Vec<Track> = Vec::new();
    project.tracks.retain(|t| {
        if carried.contains(&t.id) {
            moved.push(t.clone());
            false
        } else {
            true
        }
    });
    for member in &mut moved {
        if members.contains(&member.id) {
            member.group = Some(group);
        }
    }
    let at = first.min(project.tracks.len());
    project.tracks.insert(at, track);
    for (offset, member) in moved.into_iter().enumerate() {
        project.tracks.insert(at + 1 + offset, member);
    }
    Some(group)
}

pub fn ungroup(project: &mut Project, group: Id) -> bool {
    if project.track(group).map(|t| t.kind) != Some(TrackKind::Group) {
        return false;
    }
    let parent = project.track(group).and_then(|t| t.group);
    for track in &mut project.tracks {
        if track.group == Some(group) {
            track.group = parent;
        }
    }
    project.tracks.retain(|t| t.id != group);
    true
}

pub fn add_return(
    project: &mut Project,
    name: &str,
    effect: Option<crate::effects::BuiltinEffect>,
) -> Option<usize> {
    if project.returns.len() >= crate::model::MAX_RETURNS {
        return None;
    }
    let index = project.returns.len();
    let inserts = effect
        .map(|e| vec![crate::model::PluginSlot::builtin(project.alloc_id(), e)])
        .unwrap_or_default();
    project.returns.push(crate::model::ReturnTrack {
        name: format!("{} {name}", crate::model::return_letter(index)),
        inserts,
        ..Default::default()
    });
    Some(index)
}

pub fn delete_return(project: &mut Project, index: usize) -> bool {
    if index >= project.returns.len() {
        return false;
    }
    project.returns.remove(index);
    for track in &mut project.tracks {
        if index < track.strip.sends.len() {
            track.strip.sends.remove(index);
        }
    }
    true
}

pub fn default_set(project: &mut Project) {
    for kind in [
        TrackKind::Instrument,
        TrackKind::Instrument,
        TrackKind::Audio,
        TrackKind::Audio,
    ] {
        add_track(project, kind);
    }
    for effect in crate::effects::BuiltinEffect::all() {
        let (name, wet) = match effect {
            crate::effects::BuiltinEffect::Reverb(mut p) => {
                p.mix = 1.0;
                ("Reverb", crate::effects::BuiltinEffect::Reverb(p))
            }
            crate::effects::BuiltinEffect::Delay(mut p) => {
                p.mix = 1.0;
                ("Delay", crate::effects::BuiltinEffect::Delay(p))
            }
            _ => continue,
        };
        add_return(project, name, Some(wet));
    }
    project.returns.sort_by(|a, b| b.name.cmp(&a.name));
    for (index, ret) in project.returns.iter_mut().enumerate() {
        let name = ret.name.split_once(' ').map_or("", |(_, n)| n).to_owned();
        ret.name = format!("{} {name}", crate::model::return_letter(index));
    }
}

pub fn delete_track(project: &mut Project, track_id: Id) -> bool {
    let parent = project.track(track_id).and_then(|t| t.group);
    for track in &mut project.tracks {
        if track.group == Some(track_id) {
            track.group = parent;
        }
    }
    let before = project.tracks.len();
    project.tracks.retain(|t| t.id != track_id);
    project.tracks.len() != before
}

pub fn add_midi_region(
    project: &mut Project,
    track_id: Id,
    start_tick: u64,
    length_tick: u64,
) -> Option<Id> {
    if length_tick == 0 {
        return None;
    }
    let kind = project.track(track_id)?.kind;
    if kind != TrackKind::Instrument {
        return None;
    }
    let id = project.alloc_id();
    let track = project.track_mut(track_id)?;
    track.regions.push(Region {
        loop_ticks: None,
        id,
        name: track.name.clone(),
        start_tick,
        length_tick,
        muted: false,
        fade_in_tick: 0,
        fade_out_tick: 0,
        content: RegionContent::Midi { notes: Vec::new() },
    });
    Some(id)
}

pub fn delete_regions(project: &mut Project, ids: &[Id]) -> usize {
    let mut removed = 0;
    for track in &mut project.tracks {
        let before = track.regions.len();
        track.regions.retain(|r| !ids.contains(&r.id));
        removed += before - track.regions.len();
    }
    removed
}

pub fn move_region(project: &mut Project, region_id: Id, start_tick: u64, track_id: Id) -> bool {
    let Some((source, _)) = project.find_region(region_id) else {
        return false;
    };
    let (source_id, source_kind) = (source.id, source.kind);
    if project.track(track_id).map(|t| t.kind) != Some(source_kind) {
        return false;
    }
    let Some(source) = project.track_mut(source_id) else {
        return false;
    };
    let Some(index) = source.regions.iter().position(|r| r.id == region_id) else {
        return false;
    };
    let mut region = source.regions.remove(index);
    region.start_tick = start_tick;
    match project.track_mut(track_id) {
        Some(target) => {
            target.regions.push(region);
            true
        }
        None => false,
    }
}

pub fn nudge_regions(project: &mut Project, ids: &[Id], delta: i64) -> bool {
    let min_start = regions(project, ids).map(|r| r.start_tick).min();
    let Some(min_start) = min_start else {
        return false;
    };
    let delta = delta.max(-(min_start as i64));
    if delta == 0 {
        return false;
    }
    for region in regions_mut(project, ids) {
        region.start_tick = (region.start_tick as i64 + delta) as u64;
    }
    true
}

fn project_timeline(project: &Project) -> Timeline {
    project.timeline(project.sample_rate)
}

pub fn frames_to_ticks_at(project: &Project, start_tick: u64, frames: u64) -> u64 {
    let timeline = project_timeline(project);
    let start = timeline.tick_to_sample_f64(start_tick as f64);
    let end = timeline.sample_to_tick_f64(start + frames as f64);
    ((end - start_tick as f64).ceil() as u64).max(1)
}

pub fn set_tempo_change(project: &mut Project, tick: u64, bpm: f64) -> bool {
    if tick == 0 {
        project.tempo_bpm = bpm;
        return true;
    }
    let changes = &mut project.tempo_changes;
    match changes.binary_search_by_key(&tick, |c| c.tick) {
        Ok(index) => changes[index].bpm = bpm,
        Err(index) => {
            if changes.len() >= crate::time::MAX_TEMPO_CHANGES {
                return false;
            }
            changes.insert(
                index,
                crate::model::TempoChange {
                    tick,
                    bpm,
                    ramp: false,
                },
            );
        }
    }
    true
}

pub fn set_tempo_ramp(project: &mut Project, tick: u64, ramp: bool) -> bool {
    match project.tempo_changes.iter_mut().find(|c| c.tick == tick) {
        Some(change) => {
            change.ramp = ramp;
            true
        }
        None => false,
    }
}

pub fn set_meter_change(
    project: &mut Project,
    bar: u64,
    signature: crate::model::TimeSignature,
) -> bool {
    if bar <= 1 {
        project.time_signature = signature;
        return true;
    }
    let changes = &mut project.meter_changes;
    match changes.binary_search_by_key(&bar, |c| c.bar) {
        Ok(index) => changes[index].signature = signature,
        Err(index) => {
            if changes.len() >= crate::time::MAX_METER_CHANGES {
                return false;
            }
            changes.insert(index, crate::model::MeterChange { bar, signature });
        }
    }
    true
}

pub fn delete_meter_change(project: &mut Project, bar: u64) -> bool {
    let before = project.meter_changes.len();
    project.meter_changes.retain(|c| c.bar != bar);
    project.meter_changes.len() != before
}

pub fn delete_tempo_change(project: &mut Project, tick: u64) -> bool {
    let before = project.tempo_changes.len();
    project.tempo_changes.retain(|c| c.tick != tick);
    project.tempo_changes.len() != before
}

pub fn ticks_to_frames(project: &Project, start_tick: u64, ticks: u64) -> u64 {
    let timeline = project_timeline(project);
    timeline.tick_to_sample(start_tick + ticks) - timeline.tick_to_sample(start_tick)
}

pub fn frames_to_ticks(project: &Project, frames: u64) -> u64 {
    let ticks = frames as f64 / project_timeline(project).samples_per_tick();
    (ticks.ceil() as u64).max(1)
}

fn split_content(region: &mut Region, cut: u64, cut_frames: u64) -> RegionContent {
    match &mut region.content {
        RegionContent::Audio {
            file_id,
            offset_frames,
            gain_db,
        } => RegionContent::Audio {
            file_id: *file_id,
            offset_frames: *offset_frames + cut_frames,
            gain_db: *gain_db,
        },
        RegionContent::Midi { notes } => {
            let mut right = Vec::new();
            notes.retain_mut(|note| {
                if note.start_tick >= cut {
                    let mut moved = *note;
                    moved.start_tick -= cut;
                    right.push(moved);
                    false
                } else {
                    note.length_tick = note.length_tick.min(cut - note.start_tick);
                    true
                }
            });
            RegionContent::Midi { notes: right }
        }
    }
}

pub fn split_regions_at(project: &mut Project, ids: &[Id], tick: u64) -> Vec<Id> {
    let mut created = Vec::new();
    for track_index in 0..project.tracks.len() {
        let mut new_regions = Vec::new();
        let candidates: Vec<usize> = project.tracks[track_index]
            .regions
            .iter()
            .enumerate()
            .filter(|(_, r)| ids.contains(&r.id) && r.start_tick < tick && tick < r.end_tick())
            .map(|(i, _)| i)
            .collect();
        for index in candidates {
            let id = project.alloc_id();
            let start = project.tracks[track_index].regions[index].start_tick;
            let cut_frames = ticks_to_frames(project, start, tick - start);
            let region = &mut project.tracks[track_index].regions[index];
            let cut = tick - region.start_tick;
            let content = split_content(region, cut, cut_frames);
            let right = Region {
                loop_ticks: None,
                id,
                name: region.name.clone(),
                start_tick: tick,
                length_tick: region.length_tick - cut,
                muted: region.muted,
                fade_in_tick: 0,
                fade_out_tick: region.fade_out_tick.min(region.length_tick - cut),
                content,
            };
            region.length_tick = cut;
            region.fade_out_tick = 0;
            region.fade_in_tick = region.fade_in_tick.min(cut);
            new_regions.push(right);
            created.push(id);
        }
        project.tracks[track_index].regions.extend(new_regions);
    }
    created
}

pub fn repeat_regions(project: &mut Project, ids: &[Id]) -> Vec<Id> {
    let mut created = Vec::new();
    for track_index in 0..project.tracks.len() {
        let copies: Vec<Region> = project.tracks[track_index]
            .regions
            .iter()
            .filter(|r| ids.contains(&r.id))
            .cloned()
            .collect();
        for mut copy in copies {
            copy.id = project.alloc_id();
            copy.start_tick = copy.end_tick();
            created.push(copy.id);
            project.tracks[track_index].regions.push(copy);
        }
    }
    created
}

pub fn copy_regions(project: &Project, ids: &[Id]) -> Vec<Region> {
    let mut copied: Vec<Region> = regions(project, ids).cloned().collect();
    copied.sort_by_key(|r| r.start_tick);
    copied
}

pub fn paste_regions(
    project: &mut Project,
    track_id: Id,
    at_tick: u64,
    clip: &[Region],
) -> Vec<Id> {
    let Some(first) = clip.iter().map(|r| r.start_tick).min() else {
        return Vec::new();
    };
    let Some(kind) = project.track(track_id).map(|t| t.kind) else {
        return Vec::new();
    };
    let mut created = Vec::new();
    let pasted: Vec<Region> = clip
        .iter()
        .filter(|r| r.kind() == kind)
        .map(|r| {
            let mut copy = r.clone();
            copy.id = project.alloc_id();
            copy.start_tick = at_tick + (r.start_tick - first);
            created.push(copy.id);
            copy
        })
        .collect();
    if let Some(track) = project.track_mut(track_id) {
        track.regions.extend(pasted);
    }
    created
}

pub fn toggle_regions_muted(project: &mut Project, ids: &[Id]) {
    let mute = regions(project, ids).any(|r| !r.muted);
    for region in regions_mut(project, ids) {
        region.muted = mute;
    }
}

pub fn set_loop(project: &mut Project, start_tick: u64, end_tick: u64) -> bool {
    if end_tick <= start_tick {
        return false;
    }
    project.loop_range.start_tick = start_tick;
    project.loop_range.end_tick = end_tick;
    project.loop_range.enabled = true;
    true
}

pub fn resize_region(project: &mut Project, id: Id, length_tick: u64) -> bool {
    if length_tick == 0 {
        return false;
    }
    match region_mut(project, id) {
        Some(region) if region.length_tick != length_tick => {
            region.length_tick = length_tick;
            true
        }
        _ => false,
    }
}

pub fn region_mut(project: &mut Project, id: Id) -> Option<&mut Region> {
    project
        .tracks
        .iter_mut()
        .flat_map(|t| {
            t.regions
                .iter_mut()
                .chain(t.session.iter_mut().flatten().map(|c| &mut c.region))
                .chain(t.takes.iter_mut().flat_map(|l| l.regions.iter_mut()))
        })
        .find(|r| r.id == id)
}

fn notes_of(project: &mut Project, region_id: Id) -> Option<&mut Vec<Note>> {
    region_mut(project, region_id)?.notes_mut()
}

fn is_valid_note(note: &Note) -> bool {
    note.pitch <= 127 && (1..=127).contains(&note.velocity) && note.length_tick > 0
}

pub fn add_note(project: &mut Project, region_id: Id, note: Note) -> Option<usize> {
    if !is_valid_note(&note) {
        return None;
    }
    let notes = notes_of(project, region_id)?;
    notes.push(note);
    Some(notes.len() - 1)
}

pub fn add_audio_file(project: &mut Project, path: String, channels: u16, frames: u64) -> Id {
    let id = project.alloc_id();
    project.audio_files.push(AudioFile {
        id,
        path,
        channels,
        frames,
    });
    id
}

pub fn add_audio_region(
    project: &mut Project,
    track_id: Id,
    start_tick: u64,
    file_id: Id,
    offset_frames: u64,
) -> Option<Id> {
    let frames = project.audio_file(file_id)?.frames;
    if offset_frames >= frames || project.track(track_id)?.kind != TrackKind::Audio {
        return None;
    }
    let length_tick = frames_to_ticks_at(project, start_tick, frames - offset_frames);
    let name = project.audio_file(file_id).map_or_else(String::new, |f| {
        std::path::Path::new(&f.path)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    });
    let id = project.alloc_id();
    project.track_mut(track_id)?.regions.push(Region {
        loop_ticks: None,
        id,
        name,
        start_tick,
        length_tick,
        muted: false,
        fade_in_tick: 0,
        fade_out_tick: 0,
        content: RegionContent::Audio {
            file_id,
            offset_frames,
            gain_db: 0.0,
        },
    });
    Some(id)
}

pub fn delete_notes(project: &mut Project, region_id: Id, indices: &[usize]) -> usize {
    let Some(notes) = notes_of(project, region_id) else {
        return 0;
    };
    let before = notes.len();
    let mut index = 0;
    notes.retain(|_| {
        let keep = !indices.contains(&index);
        index += 1;
        keep
    });
    before - notes.len()
}

fn selected_notes<'a>(
    notes: &'a mut [Note],
    indices: &'a [usize],
) -> impl Iterator<Item = &'a mut Note> {
    notes
        .iter_mut()
        .enumerate()
        .filter(|(i, _)| indices.contains(i))
        .map(|(_, n)| n)
}

pub fn move_notes(
    project: &mut Project,
    region_id: Id,
    indices: &[usize],
    delta_ticks: i64,
    delta_pitch: i32,
) -> bool {
    let Some(notes) = notes_of(project, region_id) else {
        return false;
    };
    let picked: Vec<Note> = notes
        .iter()
        .enumerate()
        .filter(|(i, _)| indices.contains(i))
        .map(|(_, n)| *n)
        .collect();
    let (Some(min_start), Some(min_pitch), Some(max_pitch)) = (
        picked.iter().map(|n| n.start_tick).min(),
        picked.iter().map(|n| n.pitch).min(),
        picked.iter().map(|n| n.pitch).max(),
    ) else {
        return false;
    };
    let delta_ticks = delta_ticks.max(-(min_start as i64));
    let delta_pitch = delta_pitch.clamp(-i32::from(min_pitch), 127 - i32::from(max_pitch));
    if delta_ticks == 0 && delta_pitch == 0 {
        return false;
    }
    for note in selected_notes(notes, indices) {
        note.start_tick = (note.start_tick as i64 + delta_ticks) as u64;
        note.pitch = (i32::from(note.pitch) + delta_pitch) as u8;
    }
    true
}

pub fn resize_notes(project: &mut Project, region_id: Id, indices: &[usize], delta: i64) -> bool {
    let Some(notes) = notes_of(project, region_id) else {
        return false;
    };
    let mut changed = false;
    for note in selected_notes(notes, indices) {
        let length = (note.length_tick as i64 + delta).max(1) as u64;
        changed |= length != note.length_tick;
        note.length_tick = length;
    }
    changed
}

pub fn set_notes_velocity(project: &mut Project, region_id: Id, indices: &[usize], velocity: u8) {
    let velocity = velocity.clamp(1, 127);
    if let Some(notes) = notes_of(project, region_id) {
        for note in selected_notes(notes, indices) {
            note.velocity = velocity;
        }
    }
}

fn quantize_region(region: &mut Region, indices: Option<&[usize]>, grid: u64) -> bool {
    let start = region.start_tick;
    let mut changed = false;
    let Some(notes) = region.notes_mut() else {
        return false;
    };
    for (i, note) in notes.iter_mut().enumerate() {
        if indices.is_some_and(|picked| !picked.contains(&i)) {
            continue;
        }
        let snapped = snap_round(start + note.start_tick, grid).saturating_sub(start);
        changed |= snapped != note.start_tick;
        note.start_tick = snapped;
    }
    changed
}

pub fn quantize_notes(project: &mut Project, region_id: Id, indices: &[usize], grid: u64) -> bool {
    region_mut(project, region_id).is_some_and(|r| quantize_region(r, Some(indices), grid))
}

pub fn quantize_regions(project: &mut Project, ids: &[Id], grid: u64) -> bool {
    let mut changed = false;
    for region in regions_mut(project, ids) {
        changed |= quantize_region(region, None, grid);
    }
    changed
}

pub fn copy_notes(project: &Project, region_id: Id, indices: &[usize]) -> Vec<Note> {
    let Some((_, region)) = project.find_region(region_id) else {
        return Vec::new();
    };
    let mut copied: Vec<Note> = region
        .notes()
        .iter()
        .enumerate()
        .filter(|(i, _)| indices.contains(i))
        .map(|(_, n)| *n)
        .collect();
    copied.sort_by_key(|n| (n.start_tick, n.pitch));
    copied
}

pub fn paste_notes(
    project: &mut Project,
    region_id: Id,
    at_tick: u64,
    clip: &[Note],
) -> Vec<usize> {
    let Some(first) = clip.iter().map(|n| n.start_tick).min() else {
        return Vec::new();
    };
    let Some(notes) = notes_of(project, region_id) else {
        return Vec::new();
    };
    let start = notes.len();
    notes.extend(clip.iter().map(|n| Note {
        start_tick: at_tick + (n.start_tick - first),
        ..*n
    }));
    (start..notes.len()).collect()
}

pub fn add_recorded_take(
    project: &mut Project,
    track_id: Id,
    notes: &[Note],
    loop_range: Option<(u64, u64)>,
    bar: u64,
) -> Option<Id> {
    let (start, end) = match loop_range {
        Some(range) => range,
        None => {
            let first = notes.iter().map(|n| n.start_tick).min()?;
            let last = notes.iter().map(Note::end_tick).max()?;
            let start = snap_floor(first, bar);
            let end = snap_floor(last + bar - 1, bar).max(start + bar);
            (start, end)
        }
    };
    let relative: Vec<Note> = notes
        .iter()
        .filter(|n| is_valid_note(n) && n.start_tick >= start && n.start_tick < end)
        .map(|n| Note {
            start_tick: n.start_tick - start,
            length_tick: n.length_tick.min(end - n.start_tick),
            ..*n
        })
        .collect();
    if relative.is_empty() {
        return None;
    }
    let id = add_midi_region(project, track_id, start, end - start)?;
    *notes_of(project, id)? = relative;
    Some(id)
}

pub fn set_region_fades(project: &mut Project, id: Id, fade_in: u64, fade_out: u64) -> bool {
    let Some(region) = region_mut(project, id) else {
        return false;
    };
    let fade_in = fade_in.min(region.length_tick);
    let fade_out = fade_out.min(region.length_tick - fade_in);
    let changed = (region.fade_in_tick, region.fade_out_tick) != (fade_in, fade_out);
    region.fade_in_tick = fade_in;
    region.fade_out_tick = fade_out;
    changed
}

pub fn rename_region(project: &mut Project, id: Id, name: &str) -> bool {
    let name = name.trim();
    match region_mut(project, id) {
        Some(region) if !name.is_empty() && region.name != name => {
            region.name = name.to_owned();
            true
        }
        _ => false,
    }
}

pub fn set_region_gain(project: &mut Project, id: Id, gain: f32) -> bool {
    let Some(region) = region_mut(project, id) else {
        return false;
    };
    match &mut region.content {
        RegionContent::Audio { gain_db, .. } => {
            *gain_db = gain.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
            true
        }
        RegionContent::Midi { .. } => false,
    }
}

pub fn trim_region_start(project: &mut Project, id: Id, new_start: u64) -> bool {
    let Some((_, region)) = project.find_region(id) else {
        return false;
    };
    let (start, end) = (region.start_tick, region.end_tick());
    let new_start = match region.content {
        RegionContent::Audio { offset_frames, .. } => {
            let earliest = start.saturating_sub(frames_to_ticks(project, offset_frames));
            new_start.max(earliest)
        }
        RegionContent::Midi { .. } => new_start,
    };
    if new_start >= end || new_start == start {
        return false;
    }
    let frames_shift = if new_start > start {
        ticks_to_frames(project, start, new_start - start) as i64
    } else {
        -(ticks_to_frames(project, new_start, start - new_start) as i64)
    };
    let Some(region) = region_mut(project, id) else {
        return false;
    };
    let delta = new_start as i64 - start as i64;
    match &mut region.content {
        RegionContent::Audio { offset_frames, .. } => {
            *offset_frames = (*offset_frames as i64 + frames_shift).max(0) as u64;
        }
        RegionContent::Midi { notes } => {
            notes.retain_mut(|note| {
                let begin = note.start_tick as i64 - delta;
                let finish = begin + note.length_tick as i64;
                if finish <= 0 {
                    return false;
                }
                note.start_tick = begin.max(0) as u64;
                note.length_tick = (finish - begin.max(0)) as u64;
                true
            });
        }
    }
    region.start_tick = new_start;
    region.length_tick = end - new_start;
    region.fade_in_tick = region.fade_in_tick.min(region.length_tick);
    region.fade_out_tick = region
        .fade_out_tick
        .min(region.length_tick - region.fade_in_tick);
    true
}

fn automation_lane(
    project: &mut Project,
    track: Id,
    param: AutomationParam,
) -> Option<&mut Vec<AutomationPoint>> {
    Some(project.track_mut(track)?.automation.lane_mut(param))
}

pub fn add_automation_point(
    project: &mut Project,
    track: Id,
    param: AutomationParam,
    tick: u64,
    value: f32,
) -> Option<usize> {
    if !param.is_valid(value) {
        return None;
    }
    let lane = automation_lane(project, track, param)?;
    let index = lane.partition_point(|p| p.tick < tick);
    if lane.get(index).is_some_and(|p| p.tick == tick) {
        lane[index].value = value;
    } else {
        lane.insert(
            index,
            AutomationPoint {
                tick,
                value,
                curve: 0.0,
            },
        );
    }
    Some(index)
}

pub fn move_automation_point(
    project: &mut Project,
    track: Id,
    param: AutomationParam,
    index: usize,
    tick: u64,
    value: f32,
) -> Option<usize> {
    let lane = automation_lane(project, track, param)?;
    if index >= lane.len() {
        return None;
    }
    lane.remove(index);
    add_automation_point(project, track, param, tick, value)
}

pub fn set_automation_curve(
    project: &mut Project,
    track: Id,
    param: AutomationParam,
    index: usize,
    curve: f32,
) -> bool {
    match automation_lane(project, track, param).and_then(|lane| lane.get_mut(index)) {
        Some(point) => {
            point.curve = curve.clamp(-1.0, 1.0);
            true
        }
        None => false,
    }
}

pub fn delete_automation_point(
    project: &mut Project,
    track: Id,
    param: AutomationParam,
    index: usize,
) -> bool {
    match automation_lane(project, track, param) {
        Some(lane) if index < lane.len() => {
            lane.remove(index);
            true
        }
        _ => false,
    }
}

pub fn add_marker(project: &mut Project, tick: u64) -> Id {
    let id = project.alloc_id();
    let number = project.markers.len() + 1;
    let index = project.markers.partition_point(|m| m.tick <= tick);
    project.markers.insert(
        index,
        Marker {
            id,
            tick,
            name: format!("Marker {number}"),
        },
    );
    id
}

pub fn rename_marker(project: &mut Project, id: Id, name: &str) -> bool {
    let name = name.trim();
    match project.markers.iter_mut().find(|m| m.id == id) {
        Some(marker) if !name.is_empty() => {
            marker.name = name.to_owned();
            true
        }
        _ => false,
    }
}

pub fn delete_marker(project: &mut Project, id: Id) -> bool {
    let before = project.markers.len();
    project.markers.retain(|m| m.id != id);
    project.markers.len() != before
}

fn give_new_ids(project: &mut Project, track: &mut crate::model::Track) {
    track.id = project.alloc_id();
    track.record_enabled = false;
    for region in &mut track.regions {
        region.id = project.alloc_id();
    }
    for clip in track.session.iter_mut().flatten() {
        clip.region.id = project.alloc_id();
    }
    for lane in &mut track.takes {
        lane.id = project.alloc_id();
        for region in &mut lane.regions {
            region.id = project.alloc_id();
        }
    }
    if let Some(Instrument::Vst3(slot)) = &mut track.instrument {
        slot.id = project.alloc_id();
    }
    for slot in &mut track.strip.inserts {
        slot.id = project.alloc_id();
    }
}

pub fn duplicate_track(project: &mut Project, id: Id) -> Option<Id> {
    let index = project.tracks.iter().position(|t| t.id == id)?;
    let mut copy = project.tracks[index].clone();
    give_new_ids(project, &mut copy);
    copy.name = format!("{} copy", copy.name);
    let new_id = copy.id;
    project.tracks.insert(index + 1, copy);
    Some(new_id)
}

pub fn copy_tracks(project: &Project, ids: &[Id]) -> Vec<crate::model::Track> {
    project
        .tracks
        .iter()
        .filter(|t| ids.contains(&t.id))
        .cloned()
        .collect()
}

pub fn paste_tracks(
    project: &mut Project,
    after: Option<Id>,
    tracks: &[crate::model::Track],
) -> Vec<Id> {
    let index = after
        .and_then(|id| project.tracks.iter().position(|t| t.id == id))
        .map_or(project.tracks.len(), |i| i + 1);
    let mut created = Vec::new();
    for (offset, track) in tracks.iter().enumerate() {
        let mut copy = track.clone();
        give_new_ids(project, &mut copy);
        for clip in copy
            .regions
            .iter()
            .chain(copy.takes.iter().flat_map(|l| l.regions.iter()))
        {
            if let RegionContent::Audio { file_id, .. } = clip.content
                && project.audio_file(file_id).is_none()
            {
                return created;
            }
        }
        copy.session.truncate(project.scenes.len());
        created.push(copy.id);
        project.tracks.insert(index + offset, copy);
    }
    created
}

pub fn move_insert(inserts: &mut Vec<crate::model::PluginSlot>, from: usize, to: usize) -> bool {
    if from >= inserts.len() || to >= inserts.len() || from == to {
        return false;
    }
    let slot = inserts.remove(from);
    inserts.insert(to, slot);
    true
}

pub fn move_track(project: &mut Project, id: Id, delta: i64) -> bool {
    let Some(index) = project.tracks.iter().position(|t| t.id == id) else {
        return false;
    };
    let target = (index as i64 + delta).clamp(0, project.tracks.len() as i64 - 1) as usize;
    if target == index {
        return false;
    }
    let track = project.tracks.remove(index);
    project.tracks.insert(target, track);
    true
}

pub fn all_region_ids(project: &Project) -> Vec<Id> {
    project
        .tracks
        .iter()
        .flat_map(|t| t.regions.iter().map(|r| r.id))
        .collect()
}

fn regions<'a>(project: &'a Project, ids: &'a [Id]) -> impl Iterator<Item = &'a Region> {
    project
        .tracks
        .iter()
        .flat_map(|t| t.regions.iter())
        .filter(|r| ids.contains(&r.id))
}

fn regions_mut<'a>(
    project: &'a mut Project,
    ids: &'a [Id],
) -> impl Iterator<Item = &'a mut Region> {
    project
        .tracks
        .iter_mut()
        .flat_map(|t| t.regions.iter_mut())
        .filter(|r| ids.contains(&r.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(pitch: u8, start_tick: u64, length_tick: u64) -> Note {
        Note {
            pitch,
            velocity: 100,
            start_tick,
            length_tick,
            chance: 100,
        }
    }

    fn notes(p: &Project, id: Id) -> Vec<Note> {
        region(p, id).notes().to_vec()
    }

    #[test]
    fn templates_keep_tracks_but_drop_audio() {
        let mut p = Project::new("t", 48_000);
        let audio = add_track(&mut p, TrackKind::Audio);
        let midi = add_track(&mut p, TrackKind::Instrument);
        let file = add_audio_file(&mut p, "audio/a.wav".into(), 1, 4800);
        add_audio_region(&mut p, audio, 0, file, 0).unwrap();
        add_midi_region(&mut p, midi, 0, 960).unwrap();
        strip_audio(&mut p);
        assert!(p.audio_files.is_empty());
        assert!(p.track(audio).unwrap().regions.is_empty());
        assert_eq!(p.track(midi).unwrap().regions.len(), 1);
        assert_eq!(p.tracks.len(), 2);
        assert!(crate::io::validate(&p).is_ok());
    }

    #[test]
    fn default_names_follow_the_instrument() {
        let mut p = Project::new("t", 48_000);
        let id = add_track(&mut p, TrackKind::Instrument);
        let track = p.track_mut(id).unwrap();
        track.instrument = Some(crate::model::Instrument::Sampler(Default::default()));
        assert!(name_after_instrument(track, "Basic Synth"));
        assert_eq!(track.name, "1 Sampler");
        track.instrument = Some(crate::model::Instrument::BasicSynth(Default::default()));
        assert!(name_after_instrument(track, "Sampler"));
        assert_eq!(track.name, "1 Basic Synth");
        track.instrument = None;
        assert!(name_after_instrument(track, "Basic Synth"));
        assert_eq!(track.name, "1 MIDI");
        track.name = "Lead".into();
        assert!(!name_after_instrument(track, "Basic Synth"));
        assert_eq!(track.name, "Lead");
    }

    #[test]
    fn duplicating_a_loop_doubles_its_notes() {
        let mut p = Project::new("t", 48_000);
        let t = add_track(&mut p, TrackKind::Instrument);
        let r = add_midi_region(&mut p, t, 0, 960).unwrap();
        add_note(
            &mut p,
            r,
            Note {
                pitch: 60,
                velocity: 100,
                start_tick: 100,
                length_tick: 50,
                chance: 100,
            },
        );
        assert!(set_region_loop(&mut p, r, Some(960)));
        assert!(duplicate_loop(&mut p, r));
        let region = p.find_region(r).unwrap().1.clone();
        assert_eq!(region.loop_ticks, Some(1920));
        assert_eq!(region.length_tick, 1920);
        let starts: Vec<u64> = region.notes().iter().map(|n| n.start_tick).collect();
        assert_eq!(starts, vec![100, 1060]);
    }

    #[test]
    fn velocity_tools_stay_in_range() {
        let mut p = Project::new("t", 48_000);
        let t = add_track(&mut p, TrackKind::Instrument);
        let r = add_midi_region(&mut p, t, 0, 3840).unwrap();
        for k in 0..4u64 {
            add_note(
                &mut p,
                r,
                Note {
                    pitch: 60,
                    velocity: 120,
                    start_tick: k * 960,
                    length_tick: 100,
                    chance: 100,
                },
            );
        }
        assert!(ramp_velocities(&mut p, r, &[], 20, 110));
        let v: Vec<u8> = p
            .find_region(r)
            .unwrap()
            .1
            .notes()
            .iter()
            .map(|n| n.velocity)
            .collect();
        assert_eq!(v, vec![20, 50, 80, 110]);
        assert!(randomize_velocities(&mut p, r, &[0], 64, 7));
        let after = p.find_region(r).unwrap().1.notes()[0].velocity;
        assert!((1..=84).contains(&after));
        assert_eq!(p.find_region(r).unwrap().1.notes()[1].velocity, 50);
    }

    #[test]
    fn grouping_moves_members_under_the_group() {
        let mut p = Project::new("t", 48_000);
        let a = add_track(&mut p, TrackKind::Instrument);
        let b = add_track(&mut p, TrackKind::Audio);
        let c = add_track(&mut p, TrackKind::Audio);
        let group = group_tracks(&mut p, &[a, c]).unwrap();
        let order: Vec<Id> = p.tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, vec![group, a, c, b]);
        assert_eq!(p.track(c).unwrap().group, Some(group));
        assert_eq!(p.track(b).unwrap().group, None);
        assert_eq!(p.track(group).unwrap().name, "1 Group");
        let outer = group_tracks(&mut p, &[group, b]).unwrap();
        assert_eq!(p.track(group).unwrap().group, Some(outer));
        assert_eq!(p.track(a).unwrap().group, Some(group));
        let order: Vec<Id> = p.tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, vec![outer, group, a, c, b]);
        assert_eq!(p.depth(p.track(a).unwrap()), 2);
        p.track_mut(outer).unwrap().folded = true;
        assert!(p.is_folded_away(p.track(a).unwrap()));
        p.track_mut(outer).unwrap().folded = false;
        p.track_mut(outer).unwrap().mute = true;
        assert!(!p.is_track_audible(p.track(a).unwrap()));
        p.track_mut(outer).unwrap().mute = false;
        assert!(ungroup(&mut p, outer));
        assert_eq!(p.track(group).unwrap().group, None);
        p.track_mut(group).unwrap().mute = true;
        assert!(!p.is_track_audible(p.track(a).unwrap()));
        assert!(p.is_track_audible(p.track(b).unwrap()));
        assert!(ungroup(&mut p, group));
        assert_eq!(p.tracks.len(), 3);
        assert!(p.tracks.iter().all(|t| t.group.is_none()));
    }

    #[test]
    fn inserts_move_within_bounds() {
        let mut p = Project::new("t", 48_000);
        let mut inserts: Vec<crate::model::PluginSlot> = crate::effects::BuiltinEffect::all()
            .into_iter()
            .take(3)
            .map(|e| crate::model::PluginSlot::builtin(p.alloc_id(), e))
            .collect();
        let ids: Vec<Id> = inserts.iter().map(|s| s.id).collect();
        assert!(move_insert(&mut inserts, 0, 2));
        assert_eq!(
            inserts.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![ids[1], ids[2], ids[0]]
        );
        assert!(!move_insert(&mut inserts, 1, 3));
        assert!(!move_insert(&mut inserts, 1, 1));
    }

    #[test]
    fn new_instrument_tracks_get_basic_synth() {
        let mut p = Project::new("t", 48_000);
        let inst = add_track(&mut p, TrackKind::Instrument);
        let audio = add_track(&mut p, TrackKind::Audio);
        assert_eq!(
            p.track(inst).unwrap().instrument,
            Some(Instrument::default())
        );
        assert_eq!(p.track(audio).unwrap().instrument, None);
    }

    #[test]
    fn add_and_delete_notes() {
        let (mut p, _, id) = project_with_region();
        assert_eq!(add_note(&mut p, id, note(60, 0, 240)), Some(0));
        assert_eq!(add_note(&mut p, id, note(64, 240, 240)), Some(1));
        assert_eq!(add_note(&mut p, id, note(200, 0, 240)), None);
        assert_eq!(add_note(&mut p, id, note(60, 0, 0)), None);
        assert_eq!(delete_notes(&mut p, id, &[0]), 1);
        assert_eq!(notes(&p, id), [note(64, 240, 240)]);
    }

    #[test]
    fn move_notes_is_clamped() {
        let (mut p, _, id) = project_with_region();
        add_note(&mut p, id, note(60, 100, 240));
        add_note(&mut p, id, note(120, 500, 240));
        assert!(move_notes(&mut p, id, &[0, 1], -1000, 20));
        assert_eq!(notes(&p, id), [note(67, 0, 240), note(127, 400, 240)]);
        assert!(!move_notes(&mut p, id, &[0, 1], -5, 3));
    }

    #[test]
    fn resize_keeps_minimum_length() {
        let (mut p, _, id) = project_with_region();
        add_note(&mut p, id, note(60, 0, 240));
        assert!(resize_notes(&mut p, id, &[0], 120));
        assert_eq!(notes(&p, id)[0].length_tick, 360);
        assert!(resize_notes(&mut p, id, &[0], -10_000));
        assert_eq!(notes(&p, id)[0].length_tick, 1);
    }

    #[test]
    fn velocity_is_clamped() {
        let (mut p, _, id) = project_with_region();
        add_note(&mut p, id, note(60, 0, 240));
        set_notes_velocity(&mut p, id, &[0], 0);
        assert_eq!(notes(&p, id)[0].velocity, 1);
        set_notes_velocity(&mut p, id, &[0], 200);
        assert_eq!(notes(&p, id)[0].velocity, 127);
    }

    #[test]
    fn quantize_uses_song_grid() {
        let mut p = Project::new("t", 48_000);
        let track = add_track(&mut p, TrackKind::Instrument);
        let id = add_midi_region(&mut p, track, 100, 3840).unwrap();
        add_note(&mut p, id, note(60, 10, 100));
        add_note(&mut p, id, note(62, 900, 100));
        add_note(&mut p, id, note(64, 1300, 100));
        assert!(quantize_notes(&mut p, id, &[0, 1], 240));
        let starts: Vec<u64> = notes(&p, id).iter().map(|n| n.start_tick).collect();
        assert_eq!(starts, [0, 860, 1300]);
        assert!(quantize_regions(&mut p, &[id], 240));
        assert_eq!(notes(&p, id)[2].start_tick, 1340);
        assert!(!quantize_regions(&mut p, &[id], 240));
    }

    #[test]
    fn copy_and_paste_notes() {
        let (mut p, _, id) = project_with_region();
        add_note(&mut p, id, note(64, 500, 10));
        add_note(&mut p, id, note(60, 300, 10));
        let clip = copy_notes(&p, id, &[0, 1]);
        assert_eq!(paste_notes(&mut p, id, 1000, &clip), [2, 3]);
        assert_eq!(notes(&p, id)[2..], [note(60, 1000, 10), note(64, 1200, 10)]);
    }

    #[test]
    fn region_resize_needs_length() {
        let (mut p, _, id) = project_with_region();
        assert!(!resize_region(&mut p, id, 0));
        assert!(resize_region(&mut p, id, 960));
        assert_eq!(region(&p, id).length_tick, 960);
        assert!(!resize_region(&mut p, id, 960));
    }

    #[test]
    fn recorded_take_without_loop_rounds_to_bars() {
        let mut p = Project::new("t", 48_000);
        let track = add_track(&mut p, TrackKind::Instrument);
        let take = [note(60, 4000, 500), note(62, 8000, 100)];
        let id = add_recorded_take(&mut p, track, &take, None, 3840).unwrap();
        let r = region(&p, id);
        assert_eq!((r.start_tick, r.length_tick), (3840, 2 * 3840));
        assert_eq!(r.notes(), [note(60, 160, 500), note(62, 4160, 100)]);
        assert_eq!(add_recorded_take(&mut p, track, &[], None, 3840), None);
    }

    #[test]
    fn recorded_take_with_loop_fills_loop_and_clips() {
        let mut p = Project::new("t", 48_000);
        let track = add_track(&mut p, TrackKind::Instrument);
        let take = [
            note(60, 3840, 100),
            note(62, 7000, 2000),
            note(64, 100, 100),
        ];
        let id = add_recorded_take(&mut p, track, &take, Some((3840, 7680)), 3840).unwrap();
        let r = region(&p, id);
        assert_eq!((r.start_tick, r.length_tick), (3840, 3840));
        assert_eq!(r.notes(), [note(60, 0, 100), note(62, 3160, 680)]);
    }

    fn project_with_region() -> (Project, Id, Id) {
        let mut p = Project::new("t", 48_000);
        let track = add_track(&mut p, TrackKind::Instrument);
        let region = add_midi_region(&mut p, track, 3840, 3840).unwrap();
        (p, track, region)
    }

    fn region(p: &Project, id: Id) -> &Region {
        p.find_region(id).unwrap().1
    }

    #[test]
    fn tracks_can_be_cut_and_pasted() {
        let mut p = Project::new("t", 48_000);
        let a = add_track(&mut p, TrackKind::Instrument);
        let b = add_track(&mut p, TrackKind::Audio);
        add_midi_region(&mut p, a, 0, 3840).unwrap();
        let copied = copy_tracks(&p, &[a]);
        assert!(delete_track(&mut p, a));
        let pasted = paste_tracks(&mut p, Some(b), &copied);
        assert_eq!(pasted.len(), 1);
        assert_eq!(p.tracks[1].id, pasted[0]);
        assert_ne!(pasted[0], a);
        assert_eq!(p.tracks[1].regions.len(), 1);
        let again = paste_tracks(&mut p, None, &copied);
        assert_eq!(p.tracks.last().unwrap().id, again[0]);
        assert_ne!(p.tracks[1].regions[0].id, p.tracks[2].regions[0].id);
        crate::io::validate(&p).unwrap();
    }

    #[test]
    fn default_set_has_two_midi_two_audio_and_two_returns() {
        let mut p = Project::new("t", 48_000);
        default_set(&mut p);
        let names: Vec<_> = p.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["1 MIDI", "2 MIDI", "3 Audio", "4 Audio"]);
        let returns: Vec<_> = p.returns.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(returns, ["A Reverb", "B Delay"]);
        assert!(p.returns.iter().all(|r| r.inserts.len() == 1));
        crate::io::validate(&p).unwrap();
        assert!(delete_return(&mut p, 0));
        assert_eq!(p.returns.len(), 1);
        assert!(add_return(&mut p, "Chorus", None).is_some());
        assert_eq!(p.returns[1].name, "B Chorus");
    }

    #[test]
    fn track_names_carry_their_position() {
        let mut p = Project::new("t", 48_000);
        add_track(&mut p, TrackKind::Audio);
        add_track(&mut p, TrackKind::Instrument);
        add_track(&mut p, TrackKind::Audio);
        let names: Vec<_> = p.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["1 Audio", "2 MIDI", "3 Audio"]);
        assert_eq!(p.tracks[2].color, 2);
    }

    #[test]
    fn midi_regions_only_on_instrument_tracks() {
        let mut p = Project::new("t", 48_000);
        let audio = add_track(&mut p, TrackKind::Audio);
        assert_eq!(add_midi_region(&mut p, audio, 0, 960), None);
        let inst = add_track(&mut p, TrackKind::Instrument);
        assert_eq!(add_midi_region(&mut p, inst, 0, 0), None);
    }

    #[test]
    fn split_divides_region_and_notes() {
        let (mut p, _, id) = project_with_region();
        let notes = p.tracks[0].regions[0].notes_mut().unwrap();
        notes.push(Note {
            pitch: 60,
            velocity: 100,
            start_tick: 0,
            length_tick: 2000,
            chance: 100,
        });
        notes.push(Note {
            pitch: 62,
            velocity: 100,
            start_tick: 2500,
            length_tick: 100,
            chance: 100,
        });
        let created = split_regions_at(&mut p, &[id], 3840 + 1920);
        assert_eq!(created.len(), 1);
        let left = region(&p, id);
        assert_eq!((left.start_tick, left.length_tick), (3840, 1920));
        let notes = left.notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].length_tick, 1920);
        let right = region(&p, created[0]);
        assert_eq!((right.start_tick, right.length_tick), (5760, 1920));
        assert_eq!(right.notes()[0].start_tick, 580);
    }

    fn project_with_audio() -> (Project, Id, Id, Id) {
        let mut p = Project::new("t", 48_000);
        let track = add_track(&mut p, TrackKind::Audio);
        let file = add_audio_file(&mut p, "audio/take.wav".into(), 1, 96_000);
        let region = add_audio_region(&mut p, track, 3840, file, 0).unwrap();
        (p, track, file, region)
    }

    #[test]
    fn audio_region_length_follows_file() {
        let (p, track, _, id) = project_with_audio();
        let r = region(&p, id);
        assert_eq!((r.start_tick, r.length_tick), (3840, 3840));
        assert_eq!(r.name, "take");
        assert_eq!(
            p.track(track).unwrap().audio_input,
            Some(AudioInput::default())
        );
    }

    #[test]
    fn audio_regions_only_on_audio_tracks_and_known_files() {
        let (mut p, _, file, _) = project_with_audio();
        let inst = add_track(&mut p, TrackKind::Instrument);
        let audio = add_track(&mut p, TrackKind::Audio);
        assert_eq!(add_audio_region(&mut p, inst, 0, file, 0), None);
        assert_eq!(add_audio_region(&mut p, audio, 0, 999, 0), None);
        assert_eq!(add_audio_region(&mut p, audio, 0, file, 96_000), None);
    }

    #[test]
    fn split_audio_moves_offset() {
        let (mut p, _, file, id) = project_with_audio();
        let created = split_regions_at(&mut p, &[id], 3840 + 960);
        let right = region(&p, created[0]);
        assert_eq!(right.start_tick, 4800);
        assert_eq!(
            right.content,
            RegionContent::Audio {
                file_id: file,
                offset_frames: 24_000,
                gain_db: 0.0
            }
        );
        assert_eq!(region(&p, id).length_tick, 960);
    }

    #[test]
    fn paste_only_onto_matching_track_kind() {
        let (mut p, audio, _, id) = project_with_audio();
        let inst = add_track(&mut p, TrackKind::Instrument);
        let midi = add_midi_region(&mut p, inst, 0, 960).unwrap();
        let clip = copy_regions(&p, &[id, midi]);
        let pasted = paste_regions(&mut p, audio, 0, &clip);
        assert_eq!(pasted.len(), 1);
        assert_eq!(region(&p, pasted[0]).kind(), TrackKind::Audio);
        assert_eq!(paste_regions(&mut p, inst, 0, &clip).len(), 1);
    }

    #[test]
    fn note_edits_ignore_audio_regions() {
        let (mut p, _, _, id) = project_with_audio();
        assert_eq!(add_note(&mut p, id, note(60, 0, 10)), None);
        assert!(!quantize_regions(&mut p, &[id], 240));
    }

    #[test]
    fn split_outside_region_does_nothing() {
        let (mut p, _, id) = project_with_region();
        assert!(split_regions_at(&mut p, &[id], 3840).is_empty());
        assert!(split_regions_at(&mut p, &[id], 7680).is_empty());
        assert!(split_regions_at(&mut p, &[id], 100).is_empty());
    }

    #[test]
    fn repeat_places_copy_after_region() {
        let (mut p, _, id) = project_with_region();
        let created = repeat_regions(&mut p, &[id]);
        let copy = region(&p, created[0]);
        assert_eq!(copy.start_tick, 7680);
        assert_ne!(copy.id, id);
    }

    #[test]
    fn nudge_never_goes_negative() {
        let (mut p, _, id) = project_with_region();
        assert!(nudge_regions(&mut p, &[id], -10_000));
        assert_eq!(region(&p, id).start_tick, 0);
        assert!(!nudge_regions(&mut p, &[id], -1));
    }

    #[test]
    fn move_between_tracks_of_same_kind_only() {
        let (mut p, _, id) = project_with_region();
        let other = add_track(&mut p, TrackKind::Instrument);
        let audio = add_track(&mut p, TrackKind::Audio);
        assert!(!move_region(&mut p, id, 0, audio));
        assert!(move_region(&mut p, id, 960, other));
        let (track, r) = p.find_region(id).unwrap();
        assert_eq!((track.id, r.start_tick), (other, 960));
    }

    #[test]
    fn copy_paste_keeps_relative_offsets() {
        let (mut p, track, id) = project_with_region();
        let second = add_midi_region(&mut p, track, 9000, 100).unwrap();
        let clip = copy_regions(&p, &[second, id]);
        let created = paste_regions(&mut p, track, 20_000, &clip);
        let starts: Vec<u64> = created.iter().map(|&c| region(&p, c).start_tick).collect();
        assert_eq!(starts, [20_000, 20_000 + 9000 - 3840]);
    }

    #[test]
    fn mute_toggle_and_delete() {
        let (mut p, _, id) = project_with_region();
        toggle_regions_muted(&mut p, &[id]);
        assert!(region(&p, id).muted);
        toggle_regions_muted(&mut p, &[id]);
        assert!(!region(&p, id).muted);
        assert_eq!(delete_regions(&mut p, &[id]), 1);
        assert!(p.find_region(id).is_none());
    }

    #[test]
    fn loop_requires_positive_length() {
        let mut p = Project::new("t", 48_000);
        assert!(!set_loop(&mut p, 100, 100));
        assert!(set_loop(&mut p, 0, 960));
        assert!(p.loop_range.enabled);
    }

    #[test]
    fn ids_are_unique_after_edits() {
        let (mut p, track, id) = project_with_region();
        repeat_regions(&mut p, &[id]);
        let all = all_region_ids(&p);
        split_regions_at(&mut p, &all, 5000);
        add_midi_region(&mut p, track, 0, 10);
        let mut ids = all_region_ids(&p);
        ids.push(track);
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    #[test]
    fn automation_points_stay_sorted_and_valid() {
        let (mut p, track, _) = project_with_region();
        let param = AutomationParam::Volume;
        assert_eq!(
            add_automation_point(&mut p, track, param, 960, -6.0),
            Some(0)
        );
        assert_eq!(add_automation_point(&mut p, track, param, 0, 0.0), Some(0));
        assert_eq!(
            add_automation_point(&mut p, track, param, 960, -3.0),
            Some(1)
        );
        assert_eq!(add_automation_point(&mut p, track, param, 0, 50.0), None);
        let lane = p.track(track).unwrap().automation.lane(param).to_vec();
        assert_eq!(lane.len(), 2);
        assert_eq!(lane[1].value, -3.0);
        assert_eq!(
            move_automation_point(&mut p, track, param, 0, 2000, -12.0),
            Some(1)
        );
        let ticks: Vec<u64> = p
            .track(track)
            .unwrap()
            .automation
            .volume
            .iter()
            .map(|q| q.tick)
            .collect();
        assert_eq!(ticks, [960, 2000]);
        assert!(delete_automation_point(&mut p, track, param, 0));
        assert!(!delete_automation_point(&mut p, track, param, 5));
    }

    #[test]
    fn automation_values_interpolate_between_points() {
        let points = [
            AutomationPoint {
                tick: 100,
                value: 0.0,
                curve: 0.0,
            },
            AutomationPoint {
                tick: 300,
                value: -10.0,
                curve: 0.0,
            },
        ];
        assert_eq!(crate::model::value_at(&points, 0, 5.0), 0.0);
        assert_eq!(crate::model::value_at(&points, 200, 5.0), -5.0);
        assert_eq!(crate::model::value_at(&points, 999, 5.0), -10.0);
        assert_eq!(crate::model::value_at(&[], 50, 5.0), 5.0);
    }

    #[test]
    fn fades_and_gain_are_clamped() {
        let (mut p, _, file, id) = project_with_audio();
        assert!(set_region_fades(&mut p, id, 10_000, 10_000));
        let r = region(&p, id);
        assert_eq!((r.fade_in_tick, r.fade_out_tick), (3840, 0));
        assert!(set_region_gain(&mut p, id, 100.0));
        assert_eq!(
            region(&p, id).content,
            RegionContent::Audio {
                file_id: file,
                offset_frames: 0,
                gain_db: MAX_GAIN_DB
            }
        );
    }

    #[test]
    fn trimming_audio_moves_the_offset_and_stops_at_the_file_start() {
        let (mut p, _, file, id) = project_with_audio();
        assert!(trim_region_start(&mut p, id, 3840 + 960));
        let r = region(&p, id);
        assert_eq!((r.start_tick, r.length_tick), (4800, 2880));
        assert_eq!(
            r.content,
            RegionContent::Audio {
                file_id: file,
                offset_frames: 24_000,
                gain_db: 0.0
            }
        );
        assert!(trim_region_start(&mut p, id, 0));
        assert_eq!(region(&p, id).start_tick, 3840);
        assert!(!trim_region_start(&mut p, id, 7680));
    }

    #[test]
    fn trimming_midi_shifts_and_cuts_notes() {
        let (mut p, _, id) = project_with_region();
        add_note(&mut p, id, note(60, 0, 100));
        add_note(&mut p, id, note(62, 400, 400));
        add_note(&mut p, id, note(64, 1000, 10));
        assert!(trim_region_start(&mut p, id, 3840 + 500));
        assert_eq!(notes(&p, id), [note(62, 0, 300), note(64, 500, 10)]);
        assert!(trim_region_start(&mut p, id, 3840));
        assert_eq!(notes(&p, id)[0], note(62, 500, 300));
    }

    #[test]
    fn markers_are_sorted_named_and_removable() {
        let mut p = Project::new("t", 48_000);
        let b = add_marker(&mut p, 3840);
        let a = add_marker(&mut p, 0);
        assert_eq!(p.markers[0].id, a);
        assert!(rename_marker(&mut p, b, "  Chorus "));
        assert!(!rename_marker(&mut p, b, "   "));
        assert_eq!(p.markers[1].name, "Chorus");
        assert!(delete_marker(&mut p, a));
        assert_eq!(p.markers.len(), 1);
    }

    #[test]
    fn duplicate_and_move_tracks() {
        let (mut p, track, id) = project_with_region();
        let other = add_track(&mut p, TrackKind::Audio);
        let copy = duplicate_track(&mut p, track).unwrap();
        let order: Vec<Id> = p.tracks.iter().map(|t| t.id).collect();
        assert_eq!(order, [track, copy, other]);
        assert_ne!(p.track(copy).unwrap().regions[0].id, id);
        assert_eq!(p.track(copy).unwrap().name, "1 MIDI copy");
        assert!(move_track(&mut p, other, -5));
        assert_eq!(p.tracks[0].id, other);
        assert!(!move_track(&mut p, other, -1));
        crate::io::validate(&p).unwrap();
    }
}
