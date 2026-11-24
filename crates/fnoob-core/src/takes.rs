use crate::edit::{split_regions_at, ticks_to_frames};
use crate::model::{Id, Project, Region, RegionContent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeLane {
    pub id: Id,
    pub name: String,
    pub regions: Vec<Region>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pass {
    pub at: u64,
    pub offset: u64,
    pub frames: u64,
}

pub fn take_passes(
    start: u64,
    latency: u64,
    frames: u64,
    loop_range: Option<(u64, u64)>,
) -> Vec<Pass> {
    let first = if start >= latency {
        Pass {
            at: start - latency,
            offset: 0,
            frames,
        }
    } else {
        Pass {
            at: 0,
            offset: latency - start,
            frames: frames.saturating_sub(latency - start),
        }
    };
    let Some((loop_start, loop_end)) = loop_range.filter(|(s, e)| e > s && start < *e) else {
        return if first.frames > 0 {
            vec![first]
        } else {
            Vec::new()
        };
    };
    let length = loop_end - loop_start;
    let mut passes = Vec::new();
    let first_end = (loop_end - start + latency).min(frames);
    if first_end > first.offset {
        passes.push(Pass {
            frames: first_end - first.offset,
            ..first
        });
    }
    let mut base = loop_end - start + latency;
    while base < frames {
        passes.push(Pass {
            at: loop_start,
            offset: base,
            frames: length.min(frames - base),
        });
        base += length;
    }
    passes
}

pub fn add_take_lane(project: &mut Project, track: Id, regions: Vec<Region>) -> Option<usize> {
    let id = project.alloc_id();
    let t = project.track_mut(track)?;
    let name = format!("Take {}", t.takes.len() + 1);
    t.takes.push(TakeLane { id, name, regions });
    Some(t.takes.len() - 1)
}

pub fn delete_take_lane(project: &mut Project, track: Id, lane: usize) -> bool {
    match project.track_mut(track) {
        Some(t) if lane < t.takes.len() => {
            t.takes.remove(lane);
            true
        }
        _ => false,
    }
}

pub fn clear_range(project: &mut Project, track: Id, start: u64, end: u64) {
    if end <= start {
        return;
    }
    for cut in [start, end] {
        let ids: Vec<Id> = project
            .track(track)
            .map(|t| t.regions.iter().map(|r| r.id).collect())
            .unwrap_or_default();
        split_regions_at(project, &ids, cut);
    }
    if let Some(t) = project.track_mut(track) {
        t.regions
            .retain(|r| r.end_tick() <= start || r.start_tick >= end);
    }
}

fn piece(project: &Project, region: &Region, start: u64, end: u64) -> Option<Region> {
    let from = start.max(region.start_tick);
    let to = end.min(region.end_tick());
    if to <= from {
        return None;
    }
    let mut copy = region.clone();
    let cut = from - region.start_tick;
    if cut > 0 {
        match &mut copy.content {
            RegionContent::Audio { offset_frames, .. } => {
                *offset_frames += ticks_to_frames(project, region.start_tick, cut);
            }
            RegionContent::Midi { notes } => {
                notes.retain(|n| n.start_tick >= cut);
                for note in notes.iter_mut() {
                    note.start_tick -= cut;
                }
            }
        }
    }
    copy.start_tick = from;
    copy.length_tick = to - from;
    copy.fade_in_tick = 0;
    copy.fade_out_tick = 0;
    Some(copy)
}

pub fn comp(project: &mut Project, track: Id, lane: usize, start: u64, end: u64) -> Vec<Id> {
    let Some(source) = project
        .track(track)
        .and_then(|t| t.takes.get(lane))
        .map(|l| l.regions.clone())
    else {
        return Vec::new();
    };
    let pieces: Vec<Region> = source
        .iter()
        .filter_map(|r| piece(project, r, start, end))
        .collect();
    if pieces.is_empty() {
        return Vec::new();
    }
    clear_range(project, track, start, end);
    let mut created = Vec::new();
    for mut region in pieces {
        region.id = project.alloc_id();
        created.push(region.id);
        if let Some(t) = project.track_mut(track) {
            t.regions.push(region);
        }
    }
    created
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{add_audio_file, add_audio_region, add_track};
    use crate::model::TrackKind;

    #[test]
    fn one_pass_without_a_loop() {
        assert_eq!(
            take_passes(1000, 100, 5000, None),
            vec![Pass {
                at: 900,
                offset: 0,
                frames: 5000
            }]
        );
        assert_eq!(take_passes(50, 100, 5000, None)[0].offset, 50);
    }

    #[test]
    fn loop_recording_is_cut_into_passes() {
        let passes = take_passes(1000, 100, 10_000, Some((0, 4000)));
        assert_eq!(
            passes,
            vec![
                Pass {
                    at: 900,
                    offset: 0,
                    frames: 3100
                },
                Pass {
                    at: 0,
                    offset: 3100,
                    frames: 4000
                },
                Pass {
                    at: 0,
                    offset: 7100,
                    frames: 2900
                },
            ]
        );
        assert_eq!(take_passes(5000, 0, 3000, Some((0, 4000))).len(), 1);
    }

    #[test]
    fn comping_copies_a_range_from_a_take() {
        let mut p = Project::new("t", 48_000);
        let track = add_track(&mut p, TrackKind::Audio);
        let file = add_audio_file(&mut p, "audio/a.wav".into(), 1, 96_000 * 4);
        let main = add_audio_region(&mut p, track, 0, file, 0).unwrap();
        let take = p.find_region(main).unwrap().1.clone();
        let take_id = p.alloc_id();
        let lane = add_take_lane(
            &mut p,
            track,
            vec![Region {
                id: take_id,
                ..take
            }],
        )
        .unwrap();
        assert_eq!(p.track(track).unwrap().takes[lane].name, "Take 1");
        let created = comp(&mut p, track, lane, 960, 1920);
        assert_eq!(created.len(), 1);
        let t = p.track(track).unwrap();
        assert_eq!(t.regions.len(), 3);
        let piece = t.regions.iter().find(|r| r.id == created[0]).unwrap();
        assert_eq!((piece.start_tick, piece.length_tick), (960, 960));
        let RegionContent::Audio { offset_frames, .. } = piece.content else {
            panic!("audio region expected");
        };
        assert_eq!(offset_frames, 24_000);
        assert!(
            t.regions
                .iter()
                .all(|r| r.id == created[0] || r.end_tick() <= 960 || r.start_tick >= 1920)
        );
        crate::io::validate(&p).unwrap();
        assert!(delete_take_lane(&mut p, track, lane));
        assert!(comp(&mut p, track, 0, 0, 100).is_empty());
    }
}
