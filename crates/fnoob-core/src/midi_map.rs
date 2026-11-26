use crate::model::{Id, MAX_FADER_DB, Project, SILENT_DB};
use serde::{Deserialize, Serialize};

const FLOOR_DB: f32 = -60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapTarget {
    TrackVolume(Id),
    TrackPan(Id),
    TrackSend(Id, u8),
    TrackMute(Id),
    TrackSolo(Id),
    TrackArm(Id),
    ReturnVolume(u8),
    ReturnPan(u8),
    ReturnMute(u8),
    MasterVolume,
    Device(Id, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiMapping {
    pub channel: u8,
    pub controller: u8,
    pub target: MapTarget,
}

impl MapTarget {
    pub fn is_switch(self) -> bool {
        matches!(
            self,
            Self::TrackMute(_) | Self::TrackSolo(_) | Self::TrackArm(_) | Self::ReturnMute(_)
        )
    }
}

pub fn find(project: &Project, target: MapTarget) -> Option<MidiMapping> {
    project
        .midi_map
        .iter()
        .copied()
        .find(|m| m.target == target)
}

pub fn assign(project: &mut Project, channel: u8, controller: u8, target: MapTarget) {
    project
        .midi_map
        .retain(|m| m.target != target && !(m.channel == channel && m.controller == controller));
    project.midi_map.push(MidiMapping {
        channel: channel & 0x0f,
        controller: controller & 0x7f,
        target,
    });
}

pub fn unassign(project: &mut Project, target: MapTarget) -> bool {
    let before = project.midi_map.len();
    project.midi_map.retain(|m| m.target != target);
    project.midi_map.len() != before
}

fn level(value: u8) -> f32 {
    if value == 0 {
        SILENT_DB
    } else {
        FLOOR_DB + (MAX_FADER_DB - FLOOR_DB) * f32::from(value.min(127)) / 127.0
    }
}

fn pan(value: u8) -> f32 {
    let centered = (f32::from(value.min(127)) - 64.0) / 63.0;
    centered.clamp(-1.0, 1.0)
}

fn slot_mut(project: &mut Project, slot: Id) -> Option<&mut crate::model::PluginSlot> {
    project
        .tracks
        .iter_mut()
        .flat_map(|t| t.strip.inserts.iter_mut())
        .chain(
            project
                .returns
                .iter_mut()
                .flat_map(|r| r.inserts.iter_mut()),
        )
        .find(|s| s.id == slot)
}

pub fn apply(project: &mut Project, target: MapTarget, value: u8, previous: Option<u8>) -> bool {
    if target.is_switch() {
        let pressed = value >= 64 && previous.is_none_or(|p| p < 64);
        if !pressed {
            return false;
        }
    }
    match target {
        MapTarget::TrackVolume(id) => project
            .track_mut(id)
            .map(|t| t.strip.volume_db = level(value)),
        MapTarget::TrackPan(id) => project.track_mut(id).map(|t| t.strip.pan = pan(value)),
        MapTarget::TrackSend(id, i) => project
            .track_mut(id)
            .map(|t| t.strip.send_mut(usize::from(i)).level_db = level(value)),
        MapTarget::TrackMute(id) => project.track_mut(id).map(|t| t.mute = !t.mute),
        MapTarget::TrackSolo(id) => project.track_mut(id).map(|t| t.solo = !t.solo),
        MapTarget::TrackArm(id) => project
            .track_mut(id)
            .map(|t| t.record_enabled = !t.record_enabled),
        MapTarget::ReturnVolume(i) => project
            .returns
            .get_mut(usize::from(i))
            .map(|r| r.volume_db = level(value)),
        MapTarget::ReturnPan(i) => project
            .returns
            .get_mut(usize::from(i))
            .map(|r| r.pan = pan(value)),
        MapTarget::ReturnMute(i) => project
            .returns
            .get_mut(usize::from(i))
            .map(|r| r.mute = !r.mute),
        MapTarget::MasterVolume => {
            project.master.volume_db = level(value);
            Some(())
        }
        MapTarget::Device(slot, param) => slot_mut(project, slot).and_then(|s| {
            let effect = s.builtin.as_mut()?;
            effect.set_normal(param, f32::from(value.min(127)) / 127.0);
            Some(())
        }),
    }
    .is_some()
}

pub fn describe(project: &Project, target: MapTarget) -> String {
    let track = |id: Id| {
        project
            .track(id)
            .map_or_else(|| "Removed track".to_owned(), |t| t.name.clone())
    };
    let ret = |i: u8| {
        project
            .returns
            .get(usize::from(i))
            .map_or_else(|| "Removed return".to_owned(), |r| r.name.clone())
    };
    match target {
        MapTarget::TrackVolume(id) => format!("{} Volume", track(id)),
        MapTarget::TrackPan(id) => format!("{} Pan", track(id)),
        MapTarget::TrackSend(id, i) => {
            format!("{} Send {}", track(id), char::from(b'A' + i.min(25)))
        }
        MapTarget::TrackMute(id) => format!("{} On/Off", track(id)),
        MapTarget::TrackSolo(id) => format!("{} Solo", track(id)),
        MapTarget::TrackArm(id) => format!("{} Arm", track(id)),
        MapTarget::ReturnVolume(i) => format!("{} Volume", ret(i)),
        MapTarget::ReturnPan(i) => format!("{} Pan", ret(i)),
        MapTarget::ReturnMute(i) => format!("{} On/Off", ret(i)),
        MapTarget::MasterVolume => "Main Volume".to_owned(),
        MapTarget::Device(slot, param) => project
            .tracks
            .iter()
            .flat_map(|t| t.strip.inserts.iter().map(move |s| (t.name.as_str(), s)))
            .chain(
                project
                    .returns
                    .iter()
                    .flat_map(|r| r.inserts.iter().map(move |s| (r.name.as_str(), s))),
            )
            .find(|(_, s)| s.id == slot)
            .and_then(|(owner, s)| {
                let effect = s.builtin?;
                let info = effect.params().get(usize::from(param))?;
                Some(format!("{owner} {} {}", effect.label(), info.name))
            })
            .unwrap_or_else(|| "Removed device".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::add_track;
    use crate::model::TrackKind;

    #[test]
    fn controllers_drive_their_targets() {
        let mut p = Project::new("t", 48_000);
        let t = add_track(&mut p, TrackKind::Audio);
        assign(&mut p, 0, 7, MapTarget::TrackVolume(t));
        assign(&mut p, 0, 10, MapTarget::TrackPan(t));
        assign(&mut p, 0, 20, MapTarget::TrackMute(t));
        assert!(apply(&mut p, MapTarget::TrackVolume(t), 127, None));
        assert_eq!(p.track(t).unwrap().strip.volume_db, MAX_FADER_DB);
        apply(&mut p, MapTarget::TrackVolume(t), 0, None);
        assert_eq!(p.track(t).unwrap().strip.volume_db, SILENT_DB);
        apply(&mut p, MapTarget::TrackPan(t), 0, None);
        assert_eq!(p.track(t).unwrap().strip.pan, -1.0);
        apply(&mut p, MapTarget::TrackPan(t), 64, None);
        assert_eq!(p.track(t).unwrap().strip.pan, 0.0);
        assert!(apply(&mut p, MapTarget::TrackMute(t), 127, Some(0)));
        assert!(p.track(t).unwrap().mute);
        assert!(!apply(&mut p, MapTarget::TrackMute(t), 127, Some(127)));
        assert!(!apply(&mut p, MapTarget::TrackMute(t), 0, Some(127)));
        assert!(p.track(t).unwrap().mute);
        assert!(!apply(&mut p, MapTarget::TrackVolume(999), 10, None));
    }

    #[test]
    fn one_controller_and_one_target_per_mapping() {
        let mut p = Project::new("t", 48_000);
        let t = add_track(&mut p, TrackKind::Audio);
        assign(&mut p, 0, 7, MapTarget::TrackVolume(t));
        assign(&mut p, 0, 7, MapTarget::TrackPan(t));
        assert_eq!(p.midi_map.len(), 1);
        assign(&mut p, 1, 8, MapTarget::TrackPan(t));
        assert_eq!(p.midi_map.len(), 1);
        assert_eq!(find(&p, MapTarget::TrackPan(t)).unwrap().controller, 8);
        assert!(unassign(&mut p, MapTarget::TrackPan(t)));
        assert!(p.midi_map.is_empty());
    }

    #[test]
    fn device_parameters_follow_the_controller() {
        let mut p = Project::new("t", 48_000);
        let t = add_track(&mut p, TrackKind::Audio);
        let slot = p.alloc_id();
        let effect = crate::effects::BuiltinEffect::all()[1];
        p.track_mut(t)
            .unwrap()
            .strip
            .inserts
            .push(crate::model::PluginSlot::builtin(slot, effect));
        assert!(apply(&mut p, MapTarget::Device(slot, 0), 0, None));
        let now = p.track(t).unwrap().strip.inserts[0].builtin.unwrap();
        assert_eq!(now.normal(0), Some(0.0));
        assert!(describe(&p, MapTarget::Device(slot, 0)).contains("Threshold"));
    }
}
