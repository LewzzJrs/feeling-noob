use crate::edit::frames_to_ticks;
use crate::model::{Id, Project, Region, RegionContent, TimeSignature, TrackKind};
use crate::time::{PPQ, ticks_per_bar};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SCENES: usize = 8;
pub const MAX_SCENES: usize = 999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LaunchMode {
    #[default]
    Trigger,
    Gate,
    Toggle,
    Repeat,
}

impl LaunchMode {
    pub const ALL: [Self; 4] = [Self::Trigger, Self::Gate, Self::Toggle, Self::Repeat];

    pub fn label(self) -> &'static str {
        match self {
            Self::Trigger => "Trigger",
            Self::Gate => "Gate",
            Self::Toggle => "Toggle",
            Self::Repeat => "Repeat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LaunchQuantize {
    None,
    Sixteenth,
    Eighth,
    Quarter,
    #[default]
    Bar,
    TwoBars,
    FourBars,
}

impl LaunchQuantize {
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::Sixteenth,
        Self::Eighth,
        Self::Quarter,
        Self::Bar,
        Self::TwoBars,
        Self::FourBars,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Sixteenth => "1/16",
            Self::Eighth => "1/8",
            Self::Quarter => "1/4",
            Self::Bar => "1 Bar",
            Self::TwoBars => "2 Bars",
            Self::FourBars => "4 Bars",
        }
    }

    pub fn ticks(self, ts: TimeSignature) -> u64 {
        match self {
            Self::None => 0,
            Self::Sixteenth => PPQ / 4,
            Self::Eighth => PPQ / 2,
            Self::Quarter => PPQ,
            Self::Bar => ticks_per_bar(ts),
            Self::TwoBars => 2 * ticks_per_bar(ts),
            Self::FourBars => 4 * ticks_per_bar(ts),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FollowAction {
    #[default]
    None,
    Stop,
    Again,
    Previous,
    Next,
    First,
    Last,
    Any,
    Other,
}

impl FollowAction {
    pub const ALL: [Self; 9] = [
        Self::None,
        Self::Stop,
        Self::Again,
        Self::Previous,
        Self::Next,
        Self::First,
        Self::Last,
        Self::Any,
        Self::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No action",
            Self::Stop => "Stop",
            Self::Again => "Play again",
            Self::Previous => "Previous",
            Self::Next => "Next",
            Self::First => "First",
            Self::Last => "Last",
            Self::Any => "Any",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FollowActions {
    pub after_tick: u64,
    pub a: FollowAction,
    pub b: FollowAction,
    pub chance_a: f32,
}

impl Default for FollowActions {
    fn default() -> Self {
        Self {
            after_tick: 0,
            a: FollowAction::Next,
            b: FollowAction::None,
            chance_a: 1.0,
        }
    }
}

impl FollowActions {
    pub fn is_valid(&self) -> bool {
        (0.0..=1.0).contains(&self.chance_a)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionClip {
    pub region: Region,
    pub looping: bool,
    #[serde(default)]
    pub launch: LaunchMode,
    #[serde(default)]
    pub follow: FollowActions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
}

pub fn default_scenes() -> Vec<Scene> {
    (1..=DEFAULT_SCENES)
        .map(|n| Scene {
            name: n.to_string(),
        })
        .collect()
}

pub fn clip(project: &Project, track: Id, scene: usize) -> Option<&SessionClip> {
    project.track(track)?.session.get(scene)?.as_ref()
}

pub fn clip_mut(project: &mut Project, track: Id, scene: usize) -> Option<&mut SessionClip> {
    project.track_mut(track)?.session.get_mut(scene)?.as_mut()
}

pub fn find_clip_mut(project: &mut Project, region: Id) -> Option<&mut SessionClip> {
    project
        .tracks
        .iter_mut()
        .flat_map(|t| t.session.iter_mut().flatten())
        .find(|c| c.region.id == region)
}

pub fn clip_position(project: &Project, region: Id) -> Option<(Id, usize)> {
    project.tracks.iter().find_map(|t| {
        t.session
            .iter()
            .position(|c| c.as_ref().is_some_and(|c| c.region.id == region))
            .map(|scene| (t.id, scene))
    })
}

fn place(project: &mut Project, track: Id, scene: usize, clip: SessionClip) -> Option<Id> {
    if scene >= project.scenes.len() {
        return None;
    }
    let id = clip.region.id;
    let t = project.track_mut(track)?;
    if t.session.len() <= scene {
        t.session.resize(scene + 1, None);
    }
    if t.session[scene].is_some() {
        return None;
    }
    t.session[scene] = Some(clip);
    Some(id)
}

fn new_clip(
    project: &mut Project,
    name: String,
    length_tick: u64,
    content: RegionContent,
) -> SessionClip {
    SessionClip {
        region: Region {
            loop_ticks: None,
            id: project.alloc_id(),
            name,
            start_tick: 0,
            length_tick: length_tick.max(1),
            muted: false,
            fade_in_tick: 0,
            fade_out_tick: 0,
            content,
        },
        looping: true,
        launch: LaunchMode::Trigger,
        follow: FollowActions::default(),
    }
}

pub fn add_midi_clip(
    project: &mut Project,
    track: Id,
    scene: usize,
    length_tick: u64,
) -> Option<Id> {
    if project.track(track)?.kind != TrackKind::Instrument || clip(project, track, scene).is_some()
    {
        return None;
    }
    let clip = new_clip(
        project,
        "MIDI Clip".to_owned(),
        length_tick,
        RegionContent::Midi { notes: Vec::new() },
    );
    place(project, track, scene, clip)
}

pub fn add_audio_clip(project: &mut Project, track: Id, scene: usize, file_id: Id) -> Option<Id> {
    let file = project.audio_file(file_id)?.clone();
    if project.track(track)?.kind != TrackKind::Audio || clip(project, track, scene).is_some() {
        return None;
    }
    let name = std::path::Path::new(&file.path)
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let length = frames_to_ticks(project, file.frames);
    let clip = new_clip(
        project,
        name,
        length,
        RegionContent::Audio {
            file_id,
            offset_frames: 0,
            gain_db: 0.0,
        },
    );
    place(project, track, scene, clip)
}

pub fn delete_clip(project: &mut Project, track: Id, scene: usize) -> bool {
    project
        .track_mut(track)
        .and_then(|t| t.session.get_mut(scene))
        .and_then(Option::take)
        .is_some()
}

pub fn move_clip(project: &mut Project, from: (Id, usize), to: (Id, usize)) -> bool {
    if from == to || to.1 >= project.scenes.len() {
        return false;
    }
    let (Some(source), Some(target)) = (project.track(from.0), project.track(to.0)) else {
        return false;
    };
    if source.kind != target.kind || clip(project, to.0, to.1).is_some() {
        return false;
    }
    let Some(moved) = project
        .track_mut(from.0)
        .and_then(|t| t.session.get_mut(from.1))
        .and_then(Option::take)
    else {
        return false;
    };
    place(project, to.0, to.1, moved).is_some()
}

pub fn insert_scene(project: &mut Project, index: usize) -> bool {
    if project.scenes.len() >= MAX_SCENES {
        return false;
    }
    let index = index.min(project.scenes.len());
    project.scenes.insert(
        index,
        Scene {
            name: (project.scenes.len() + 1).to_string(),
        },
    );
    for track in &mut project.tracks {
        if track.session.len() > index {
            track.session.insert(index, None);
        }
    }
    true
}

pub fn delete_scene(project: &mut Project, index: usize) -> bool {
    if index >= project.scenes.len() || project.scenes.len() <= 1 {
        return false;
    }
    project.scenes.remove(index);
    for track in &mut project.tracks {
        if track.session.len() > index {
            track.session.remove(index);
        }
    }
    true
}

pub fn duplicate_clip(project: &mut Project, track: Id, scene: usize) -> Option<Id> {
    let mut copy = clip(project, track, scene)?.clone();
    if clip(project, track, scene + 1).is_some() || scene + 1 >= project.scenes.len() {
        insert_scene(project, scene + 1).then_some(())?;
    }
    copy.region.id = project.alloc_id();
    place(project, track, scene + 1, copy)
}

pub fn duplicate_scene(project: &mut Project, index: usize) -> bool {
    if index >= project.scenes.len() || !insert_scene(project, index + 1) {
        return false;
    }
    project.scenes[index + 1].name = format!("{} copy", project.scenes[index].name);
    let ids: Vec<Id> = project.tracks.iter().map(|t| t.id).collect();
    for id in ids {
        if let Some(mut copy) = clip(project, id, index).cloned() {
            copy.region.id = project.alloc_id();
            place(project, id, index + 1, copy);
        }
    }
    true
}

pub fn copy_to_arrangement(project: &mut Project, region: Id, start_tick: u64) -> Option<Id> {
    let (track, _) = clip_position(project, region)?;
    let mut copy = project
        .tracks
        .iter()
        .flat_map(|t| t.session.iter().flatten())
        .find(|c| c.region.id == region)?
        .region
        .clone();
    copy.id = project.alloc_id();
    copy.start_tick = start_tick;
    let id = copy.id;
    project.track_mut(track)?.regions.push(copy);
    Some(id)
}

pub fn copy_from_arrangement(project: &mut Project, region: Id, scene: usize) -> Option<Id> {
    let (track, original) = project.find_region(region)?;
    let (track, mut copy) = (track.id, original.clone());
    if clip(project, track, scene).is_some() {
        return None;
    }
    copy.id = project.alloc_id();
    copy.start_tick = 0;
    let clip = SessionClip {
        region: copy,
        looping: true,
        launch: LaunchMode::Trigger,
        follow: FollowActions::default(),
    };
    place(project, track, scene, clip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{add_audio_file, add_midi_region, add_track};

    fn project() -> (Project, Id, Id) {
        let mut p = Project::new("t", 48_000);
        let inst = add_track(&mut p, TrackKind::Instrument);
        let audio = add_track(&mut p, TrackKind::Audio);
        (p, inst, audio)
    }

    #[test]
    fn new_projects_have_scenes_and_clips_fit_track_kinds() {
        let (mut p, inst, audio) = project();
        assert_eq!(p.scenes.len(), DEFAULT_SCENES);
        let file = add_audio_file(&mut p, "audio/a.wav".into(), 2, 96_000);
        assert!(add_midi_clip(&mut p, audio, 0, 3840).is_none());
        assert!(add_audio_clip(&mut p, inst, 0, file).is_none());
        let midi = add_midi_clip(&mut p, inst, 2, 3840).unwrap();
        assert!(add_midi_clip(&mut p, inst, 2, 3840).is_none());
        assert!(add_midi_clip(&mut p, inst, DEFAULT_SCENES, 3840).is_none());
        let wav = add_audio_clip(&mut p, audio, 0, file).unwrap();
        assert_eq!(clip(&p, audio, 0).unwrap().region.length_tick, 3840);
        assert_eq!(clip_position(&p, midi), Some((inst, 2)));
        assert_eq!(clip_position(&p, wav), Some((audio, 0)));
        assert!(p.find_region(midi).is_some());
        crate::io::validate(&p).unwrap();
    }

    #[test]
    fn scenes_insert_delete_and_duplicate_keep_rows_aligned() {
        let (mut p, inst, _) = project();
        add_midi_clip(&mut p, inst, 1, 960).unwrap();
        insert_scene(&mut p, 0);
        assert!(clip(&p, inst, 2).is_some());
        assert!(duplicate_scene(&mut p, 2));
        assert!(clip(&p, inst, 3).is_some());
        assert_ne!(
            clip(&p, inst, 2).unwrap().region.id,
            clip(&p, inst, 3).unwrap().region.id
        );
        assert!(delete_scene(&mut p, 2));
        assert!(clip(&p, inst, 2).is_some());
        assert!(clip(&p, inst, 3).is_none());
        crate::io::validate(&p).unwrap();
    }

    #[test]
    fn clips_move_duplicate_and_copy_to_and_from_the_timeline() {
        let (mut p, inst, audio) = project();
        let id = add_midi_clip(&mut p, inst, 0, 960).unwrap();
        assert!(!move_clip(&mut p, (inst, 0), (audio, 1)));
        assert!(move_clip(&mut p, (inst, 0), (inst, 3)));
        assert_eq!(clip_position(&p, id), Some((inst, 3)));
        let copy = duplicate_clip(&mut p, inst, 3).unwrap();
        assert_eq!(clip_position(&p, copy), Some((inst, 4)));
        let placed = copy_to_arrangement(&mut p, id, 1920).unwrap();
        let region = p.find_region(placed).unwrap().1;
        assert_eq!((region.start_tick, region.length_tick), (1920, 960));
        let arranged = add_midi_region(&mut p, inst, 7680, 1920).unwrap();
        let back = copy_from_arrangement(&mut p, arranged, 0).unwrap();
        assert_eq!(clip(&p, inst, 0).unwrap().region.start_tick, 0);
        assert_eq!(clip_position(&p, back), Some((inst, 0)));
        assert!(delete_clip(&mut p, inst, 0));
        crate::io::validate(&p).unwrap();
    }
}
