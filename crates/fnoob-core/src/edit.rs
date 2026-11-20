use crate::model::{Id, Project, Region, RegionContent, TRACK_COLORS, Track, TrackKind};

pub fn add_track(project: &mut Project, kind: TrackKind) -> Id {
    let number = project.tracks.iter().filter(|t| t.kind == kind).count() + 1;
    let id = project.alloc_id();
    let color = (project.tracks.len() % usize::from(TRACK_COLORS)) as u8;
    project.tracks.push(Track {
        id,
        name: format!("{} {number}", kind.default_name()),
        kind,
        color,
        mute: false,
        solo: false,
        record_enabled: false,
        regions: Vec::new(),
    });
    id
}

pub fn delete_track(project: &mut Project, track_id: Id) -> bool {
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
        id,
        name: track.name.clone(),
        start_tick,
        length_tick,
        muted: false,
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

fn split_notes(region: &mut Region, cut: u64) -> RegionContent {
    match &mut region.content {
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
            let region = &mut project.tracks[track_index].regions[index];
            let cut = tick - region.start_tick;
            let content = split_notes(region, cut);
            let right = Region {
                id,
                name: region.name.clone(),
                start_tick: tick,
                length_tick: region.length_tick - cut,
                muted: region.muted,
                content,
            };
            region.length_tick = cut;
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
    if project.track(track_id).map(|t| t.kind) != Some(TrackKind::Instrument) {
        return Vec::new();
    }
    let mut created = Vec::new();
    let pasted: Vec<Region> = clip
        .iter()
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
    use crate::model::Note;

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
    fn track_names_count_per_kind() {
        let mut p = Project::new("t", 48_000);
        add_track(&mut p, TrackKind::Audio);
        add_track(&mut p, TrackKind::Instrument);
        add_track(&mut p, TrackKind::Audio);
        let names: Vec<_> = p.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Audio 1", "Inst 1", "Audio 2"]);
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
        let RegionContent::Midi { notes } = &mut p.tracks[0].regions[0].content;
        notes.push(Note {
            pitch: 60,
            velocity: 100,
            start_tick: 0,
            length_tick: 2000,
        });
        notes.push(Note {
            pitch: 62,
            velocity: 100,
            start_tick: 2500,
            length_tick: 100,
        });
        let created = split_regions_at(&mut p, &[id], 3840 + 1920);
        assert_eq!(created.len(), 1);
        let left = region(&p, id);
        assert_eq!((left.start_tick, left.length_tick), (3840, 1920));
        let RegionContent::Midi { notes } = &left.content;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].length_tick, 1920);
        let right = region(&p, created[0]);
        assert_eq!((right.start_tick, right.length_tick), (5760, 1920));
        let RegionContent::Midi { notes } = &right.content;
        assert_eq!(notes[0].start_tick, 580);
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
}
