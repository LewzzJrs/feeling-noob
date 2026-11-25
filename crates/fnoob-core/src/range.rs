use crate::model::{Id, Project, Region};
use crate::takes;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RangeClip {
    pub length: u64,
    pub lanes: Vec<Vec<Region>>,
}

impl RangeClip {
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

pub fn copy_range(project: &Project, tracks: &[Id], start: u64, end: u64) -> RangeClip {
    if end <= start {
        return RangeClip::default();
    }
    let lanes = ordered(project, tracks)
        .into_iter()
        .map(|id| {
            let track = project.track(id).expect("ordered returns existing tracks");
            let mut pieces: Vec<Region> = track
                .regions
                .iter()
                .filter_map(|r| takes::piece(project, r, start, end))
                .map(|mut r| {
                    r.start_tick -= start;
                    r
                })
                .collect();
            pieces.sort_by_key(|r| r.start_tick);
            pieces
        })
        .collect();
    RangeClip {
        length: end - start,
        lanes,
    }
}

pub fn clear_range(project: &mut Project, tracks: &[Id], start: u64, end: u64) {
    for id in ordered(project, tracks) {
        takes::clear_range(project, id, start, end);
    }
}

pub fn paste_range(project: &mut Project, first: Id, at: u64, clip: &RangeClip) -> Vec<Id> {
    let Some(first_index) = project.tracks.iter().position(|t| t.id == first) else {
        return Vec::new();
    };
    let mut created = Vec::new();
    for (offset, lane) in clip.lanes.iter().enumerate() {
        let Some(track_id) = project.tracks.get(first_index + offset).map(|t| t.id) else {
            break;
        };
        let kind = project.tracks[first_index + offset].kind;
        let fitting: Vec<&Region> = lane.iter().filter(|r| r.kind() == kind).collect();
        if fitting.is_empty() {
            continue;
        }
        takes::clear_range(project, track_id, at, at + clip.length);
        for region in fitting {
            let mut copy = region.clone();
            copy.id = project.alloc_id();
            copy.start_tick += at;
            created.push(copy.id);
            project.tracks[first_index + offset].regions.push(copy);
        }
    }
    created
}

pub fn insert_time(project: &mut Project, at: u64, length: u64) {
    if length == 0 {
        return;
    }
    let all: Vec<Id> = project
        .tracks
        .iter()
        .flat_map(|t| t.regions.iter().map(|r| r.id))
        .collect();
    crate::edit::split_regions_at(project, &all, at);
    let shift = |tick: &mut u64| {
        if *tick >= at {
            *tick += length;
        }
    };
    for track in &mut project.tracks {
        for region in &mut track.regions {
            shift(&mut region.start_tick);
        }
        for lane in &mut track.takes {
            for region in &mut lane.regions {
                shift(&mut region.start_tick);
            }
        }
        for point in track
            .automation
            .volume
            .iter_mut()
            .chain(track.automation.pan.iter_mut())
        {
            shift(&mut point.tick);
        }
    }
    for marker in &mut project.markers {
        shift(&mut marker.tick);
    }
    for change in &mut project.tempo_changes {
        if change.tick > 0 {
            shift(&mut change.tick);
        }
    }
}

pub fn delete_time(project: &mut Project, start: u64, end: u64) {
    if end <= start {
        return;
    }
    let length = end - start;
    let all: Vec<Id> = project.tracks.iter().map(|t| t.id).collect();
    clear_range(project, &all, start, end);
    let pull = |tick: &mut u64| {
        if *tick >= end {
            *tick -= length;
        }
    };
    let inside = |tick: u64| tick >= start && tick < end;
    for track in &mut project.tracks {
        for region in &mut track.regions {
            pull(&mut region.start_tick);
        }
        for lane in &mut track.takes {
            lane.regions
                .retain(|r| r.end_tick() <= start || r.start_tick >= end);
            for region in &mut lane.regions {
                pull(&mut region.start_tick);
            }
        }
        for points in [&mut track.automation.volume, &mut track.automation.pan] {
            points.retain(|p| !inside(p.tick));
            for point in points.iter_mut() {
                pull(&mut point.tick);
            }
        }
    }
    project.markers.retain(|m| !inside(m.tick));
    for marker in &mut project.markers {
        pull(&mut marker.tick);
    }
    project
        .tempo_changes
        .retain(|c| c.tick == 0 || !inside(c.tick));
    for change in &mut project.tempo_changes {
        if change.tick > 0 {
            pull(&mut change.tick);
        }
    }
}

fn ordered(project: &Project, tracks: &[Id]) -> Vec<Id> {
    project
        .tracks
        .iter()
        .map(|t| t.id)
        .filter(|id| tracks.contains(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{add_marker, add_midi_region, add_note, add_track};
    use crate::model::{Note, TrackKind};

    fn spans(project: &Project, track: Id) -> Vec<(u64, u64)> {
        let mut spans: Vec<(u64, u64)> = project
            .track(track)
            .unwrap()
            .regions
            .iter()
            .map(|r| (r.start_tick, r.end_tick()))
            .collect();
        spans.sort_unstable();
        spans
    }

    fn setup() -> (Project, Id, Id) {
        let mut p = Project::new("t", 48_000);
        let a = add_track(&mut p, TrackKind::Instrument);
        let b = add_track(&mut p, TrackKind::Instrument);
        let region = add_midi_region(&mut p, a, 0, 4000).unwrap();
        add_note(
            &mut p,
            region,
            Note {
                pitch: 60,
                velocity: 100,
                start_tick: 2500,
                length_tick: 100,
            },
        );
        add_midi_region(&mut p, b, 1000, 1000).unwrap();
        (p, a, b)
    }

    #[test]
    fn clearing_cuts_a_hole_only_in_the_chosen_tracks() {
        let (mut p, a, b) = setup();
        clear_range(&mut p, &[a], 1000, 2000);
        assert_eq!(spans(&p, a), vec![(0, 1000), (2000, 4000)]);
        assert_eq!(spans(&p, b), vec![(1000, 2000)]);
    }

    #[test]
    fn copy_then_paste_lands_on_the_same_track_order() {
        let (mut p, a, b) = setup();
        let clip = copy_range(&p, &[b, a], 2000, 3000);
        assert_eq!(clip.length, 1000);
        assert_eq!(clip.lanes.len(), 2);
        assert_eq!(clip.lanes[0][0].start_tick, 0);
        let created = paste_range(&mut p, a, 6000, &clip);
        assert_eq!(created.len(), 1);
        assert_eq!(spans(&p, a).last(), Some(&(6000, 7000)));
        let pasted = p.track(a).unwrap().regions.last().unwrap().clone();
        match pasted.content {
            crate::model::RegionContent::Midi { notes } => assert_eq!(notes[0].start_tick, 500),
            _ => panic!("expected MIDI"),
        }
    }

    #[test]
    fn pasting_overwrites_what_was_there() {
        let (mut p, a, _) = setup();
        let clip = copy_range(&p, &[a], 0, 500);
        paste_range(&mut p, a, 1000, &clip);
        assert_eq!(spans(&p, a), vec![(0, 1000), (1000, 1500), (1500, 4000)]);
    }

    #[test]
    fn inserting_time_pushes_everything_later() {
        let (mut p, a, b) = setup();
        add_marker(&mut p, 3000);
        insert_time(&mut p, 1500, 1000);
        assert_eq!(spans(&p, a), vec![(0, 1500), (2500, 5000)]);
        assert_eq!(spans(&p, b), vec![(1000, 1500), (2500, 3000)]);
        assert_eq!(p.markers[0].tick, 4000);
    }

    #[test]
    fn deleting_time_closes_the_gap() {
        let (mut p, a, b) = setup();
        add_marker(&mut p, 1500);
        add_marker(&mut p, 3000);
        delete_time(&mut p, 1000, 2000);
        assert_eq!(spans(&p, a), vec![(0, 1000), (1000, 3000)]);
        assert!(spans(&p, b).is_empty());
        assert_eq!(p.markers.len(), 1);
        assert_eq!(p.markers[0].tick, 2000);
    }
}
