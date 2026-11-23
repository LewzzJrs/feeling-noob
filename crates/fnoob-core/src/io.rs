use crate::fsutil::{write_atomic, write_atomic_with};
use crate::model::{
    AutomationParam, FORMAT_VERSION, Id, Instrument, MAX_FADER_DB, MAX_GAIN_DB, MAX_INSERTS,
    MIN_GAIN_DB, PluginSlot, Project, RegionContent, SILENT_DB, Track,
};
use crate::time::{MAX_BPM, MIN_BPM};
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROJECT_FILE: &str = "project.json";
pub const PROJECT_EXTENSION: &str = "fnoob";
pub const AUDIO_DIR: &str = "audio";
pub const BACKUP_DIR: &str = "backups";
pub const AUTOSAVE_DIR: &str = "autosave";
pub const MAX_BACKUPS: usize = 10;

#[derive(Debug)]
pub enum ProjectError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedVersion(u32),
    Invalid(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "project file error: {e}"),
            Self::Json(e) => write!(f, "project file is damaged: {e}"),
            Self::UnsupportedVersion(v) => write!(
                f,
                "project format version {v} is newer than this app supports ({FORMAT_VERSION})"
            ),
            Self::Invalid(reason) => write!(f, "project file is invalid: {reason}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<io::Error> for ProjectError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[derive(Deserialize)]
struct VersionProbe {
    format_version: u32,
}

pub fn save(dir: &Path, project: &Project) -> Result<(), ProjectError> {
    validate(project).map_err(ProjectError::Invalid)?;
    let bytes = serde_json::to_vec_pretty(project)?;
    let target = dir.join(PROJECT_FILE);
    write_atomic_with(&target, &bytes, || backup(dir, &target))?;
    Ok(())
}

fn civil_time(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn backup(dir: &Path, current: &Path) -> io::Result<()> {
    if !current.exists() {
        return Ok(());
    }
    let folder = dir.join(BACKUP_DIR);
    std::fs::create_dir_all(&folder)?;
    let stamp = civil_time(SystemTime::now());
    let mut target = folder.join(format!("project-{stamp}.json"));
    let mut n = 1;
    while target.exists() {
        target = folder.join(format!("project-{stamp}-{n:03}.json"));
        n += 1;
    }
    std::fs::copy(current, &target)?;
    let mut backups: Vec<_> = std::fs::read_dir(&folder)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("project-") && n.ends_with(".json"))
        })
        .collect();
    backups.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    while backups.len() > MAX_BACKUPS {
        let _ = std::fs::remove_file(backups.remove(0));
    }
    Ok(())
}

fn autosave_file(dir: &Path) -> std::path::PathBuf {
    dir.join(AUTOSAVE_DIR).join(PROJECT_FILE)
}

pub fn save_autosave(dir: &Path, project: &Project) -> Result<(), ProjectError> {
    validate(project).map_err(ProjectError::Invalid)?;
    write_atomic(&autosave_file(dir), &serde_json::to_vec_pretty(project)?)?;
    Ok(())
}

pub fn load_autosave(dir: &Path) -> Result<Project, ProjectError> {
    from_json(&std::fs::read(autosave_file(dir))?)
}

pub fn remove_autosave(dir: &Path) {
    let _ = std::fs::remove_file(autosave_file(dir));
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn newer_autosave(dir: &Path) -> Option<SystemTime> {
    let autosave = modified(&autosave_file(dir))?;
    match modified(&dir.join(PROJECT_FILE)) {
        Some(saved) if saved >= autosave => None,
        _ => Some(autosave),
    }
}

pub fn load(dir: &Path) -> Result<Project, ProjectError> {
    let bytes = std::fs::read(dir.join(PROJECT_FILE))?;
    from_json(&bytes)
}

pub fn from_json(bytes: &[u8]) -> Result<Project, ProjectError> {
    let probe: VersionProbe = serde_json::from_slice(bytes)?;
    if probe.format_version > FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion(probe.format_version));
    }
    let mut project: Project = serde_json::from_slice(bytes)?;
    project.migrate();
    validate(&project).map_err(ProjectError::Invalid)?;
    Ok(project)
}

pub fn validate(project: &Project) -> Result<(), String> {
    if !(8_000..=384_000).contains(&project.sample_rate) {
        return Err(format!(
            "sample rate {} is out of range",
            project.sample_rate
        ));
    }
    if !(MIN_BPM..=MAX_BPM).contains(&project.tempo_bpm) {
        return Err(format!("tempo {} is out of range", project.tempo_bpm));
    }
    if !project.time_signature.is_valid() {
        return Err("time signature is invalid".into());
    }
    if project.loop_range.end_tick <= project.loop_range.start_tick {
        return Err("loop end must be after loop start".into());
    }
    let fader = SILENT_DB..=MAX_FADER_DB;
    if !project.scale.is_valid() {
        return Err("the scale root must be 0 to 11".into());
    }
    if !fader.contains(&project.master.volume_db) {
        return Err("main settings are invalid".into());
    }
    if project.returns.len() > crate::model::MAX_RETURNS {
        return Err("too many return tracks".into());
    }
    for ret in &project.returns {
        if !fader.contains(&ret.volume_db) || !(-1.0..=1.0).contains(&ret.pan) {
            return Err(format!("return track {} has invalid settings", ret.name));
        }
        if ret.inserts.len() > MAX_INSERTS {
            return Err(format!("return track {} has too many inserts", ret.name));
        }
    }
    if project.scenes.is_empty() || project.scenes.len() > crate::session::MAX_SCENES {
        return Err("the clip grid must have 1 to 999 scenes".into());
    }
    if !(1..=8).contains(&project.metronome.count_in_bars) {
        return Err("count-in must be 1 to 8 bars".into());
    }
    let mut ids = HashSet::new();
    for file in &project.audio_files {
        if !ids.insert(file.id) {
            return Err(format!("duplicate id {}", file.id));
        }
        if !is_safe_relative_path(&file.path) {
            return Err(format!(
                "audio file path {:?} points outside the project folder",
                file.path
            ));
        }
        if file.channels == 0 || file.frames == 0 {
            return Err(format!("audio file {} is empty", file.id));
        }
    }
    for track in &project.tracks {
        if !ids.insert(track.id) {
            return Err(format!("duplicate id {}", track.id));
        }
        validate_track(project, track)?;
        validate_slots(track.plugin_slots(), &mut ids)?;
        for region in track.all_regions() {
            if !ids.insert(region.id) {
                return Err(format!("duplicate id {}", region.id));
            }
        }
    }
    validate_slots(
        project.returns.iter().flat_map(|r| r.inserts.iter()),
        &mut ids,
    )?;
    for marker in &project.markers {
        if !ids.insert(marker.id) {
            return Err(format!("duplicate id {}", marker.id));
        }
    }
    if ids.iter().any(|&id| id >= project.next_id) {
        return Err("next_id is lower than an existing id".into());
    }
    Ok(())
}

pub fn is_safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn validate_slots<'a>(
    slots: impl Iterator<Item = &'a PluginSlot>,
    ids: &mut HashSet<Id>,
) -> Result<(), String> {
    for slot in slots {
        if !slot.is_valid() {
            return Err(format!("plugin slot {} has an invalid class id", slot.id));
        }
        if !ids.insert(slot.id) {
            return Err(format!("duplicate id {}", slot.id));
        }
    }
    Ok(())
}

fn validate_track(project: &Project, track: &Track) -> Result<(), String> {
    if track.strip.inserts.len() > MAX_INSERTS {
        return Err(format!("track {} has too many inserts", track.id));
    }
    if !track.strip.is_valid() {
        return Err(format!("track {} has invalid mixer settings", track.id));
    }
    for param in AutomationParam::ALL {
        let lane = track.automation.lane(param);
        let sorted = lane.windows(2).all(|w| w[0].tick < w[1].tick);
        if !sorted || !lane.iter().all(|p| param.is_valid(p.value)) {
            return Err(format!("track {} has invalid automation", track.id));
        }
    }
    let instrument_ok = match &track.instrument {
        Some(Instrument::BasicSynth(params)) => params.is_valid(),
        Some(Instrument::Sampler(params)) => params.is_valid(),
        Some(Instrument::DrumSampler(params)) => params.is_valid(),
        Some(Instrument::Vst3(_)) | None => true,
    };
    if !instrument_ok {
        return Err(format!(
            "track {} has invalid instrument settings",
            track.id
        ));
    }
    if let Some(instrument) = &track.instrument
        && instrument
            .sample_files()
            .iter()
            .any(|id| project.audio_file(*id).is_none())
    {
        return Err(format!("track {} uses a missing sample", track.id));
    }
    if let Some(input) = track.audio_input
        && !(1..=2).contains(&input.channels)
    {
        return Err(format!("track {} has an invalid audio input", track.id));
    }
    if track.session.len() > project.scenes.len() {
        return Err(format!("track {} has clips outside the scenes", track.id));
    }
    for clip in track.session.iter().flatten() {
        if clip.region.start_tick != 0 || !clip.follow.is_valid() {
            return Err(format!("clip {} has invalid settings", clip.region.id));
        }
    }
    for region in track.all_regions() {
        if region.length_tick == 0 {
            return Err(format!("region {} has zero length", region.id));
        }
        if region.fade_in_tick + region.fade_out_tick > region.length_tick {
            return Err(format!("region {} has fades longer than itself", region.id));
        }
        if region.kind() != track.kind {
            return Err(format!(
                "region {} does not match the kind of track {}",
                region.id, track.id
            ));
        }
        match &region.content {
            RegionContent::Midi { notes } => {
                for note in notes {
                    if note.pitch > 127
                        || !(1..=127).contains(&note.velocity)
                        || note.length_tick == 0
                    {
                        return Err(format!("region {} has an invalid note", region.id));
                    }
                }
            }
            RegionContent::Audio {
                file_id,
                offset_frames,
                gain_db,
            } => {
                let Some(file) = project.audio_file(*file_id) else {
                    return Err(format!(
                        "region {} uses missing audio file {file_id}",
                        region.id
                    ));
                };
                if *offset_frames >= file.frames || !(MIN_GAIN_DB..=MAX_GAIN_DB).contains(gain_db) {
                    return Err(format!("region {} has invalid audio settings", region.id));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{add_audio_file, add_audio_region, add_midi_region, add_track};
    use crate::model::{
        AudioInput, AutomationPoint, DrumParams, Marker, Note, SamplerParams, TrackKind,
    };

    fn sample_project() -> Project {
        let mut p = Project::new("Song", 48_000);
        let track = add_track(&mut p, TrackKind::Instrument);
        add_midi_region(&mut p, track, 0, 3840);
        let audio = add_track(&mut p, TrackKind::Audio);
        let file = add_audio_file(&mut p, "audio/Audio 1-01.wav".into(), 1, 48_000);
        add_audio_region(&mut p, audio, 0, file, 0);
        p
    }

    fn test_slot(id: Id) -> PluginSlot {
        PluginSlot {
            id,
            class_id: "0123456789ABCDEF0123456789ABCDEF".into(),
            name: "Verb".into(),
            path_hint: "C:/Program Files/Common Files/VST3/Verb.vst3".into(),
            bypass: false,
            component_state: "AAEC".into(),
            controller_state: None,
            builtin: None,
        }
    }

    #[test]
    fn old_projects_are_migrated_on_load() {
        let p = sample_project();
        let mut value = serde_json::to_value(&p).unwrap();
        value["bus"] = serde_json::json!({
            "name": crate::model::OLD_RETURN_NAME,
            "volume_db": -3.0,
            "pan": 0.0,
            "mute": false,
            "inserts": []
        });
        value["tracks"][1]["strip"]["send"] =
            serde_json::json!({ "level_db": -6.0, "pre_fader": true });
        value["tracks"][1]["input_monitoring"] = serde_json::json!(true);
        let loaded = from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(loaded.returns.len(), 1);
        assert_eq!(loaded.returns[0].name, crate::model::RETURN_NAME);
        assert_eq!(loaded.returns[0].volume_db, -3.0);
        assert_eq!(loaded.tracks[1].strip.sends[0].level_db, -6.0);
        assert!(loaded.tracks[1].strip.sends[0].pre_fader);
        assert_eq!(loaded.tracks[1].monitor, crate::model::Monitor::In);
        assert_eq!(loaded.tracks[0].monitor, crate::model::Monitor::Auto);
        let again = serde_json::to_vec(&loaded).unwrap();
        assert_eq!(from_json(&again).unwrap(), loaded);
    }

    #[test]
    fn plugin_slots_round_trip() {
        let mut p = sample_project();
        let id = p.alloc_id();
        p.tracks[1].strip.inserts.push(test_slot(id));
        let id = p.alloc_id();
        p.tracks[0].instrument = Some(Instrument::Vst3(test_slot(id)));
        let id = p.alloc_id();
        p.returns.push(crate::model::ReturnTrack::default());
        p.returns[0].inserts.push(test_slot(id));
        validate(&p).unwrap();
        let json = serde_json::to_vec(&p).unwrap();
        assert_eq!(from_json(&json).unwrap(), p);
        assert_eq!(p.tracks[0].synth(), None);
    }

    #[test]
    fn sampler_instruments_round_trip() {
        let mut p = sample_project();
        let file = p.audio_files[0].id;
        let mut drums = DrumParams::default();
        drums.pads[5].file = Some(file);
        drums.pads[5].pan = -0.5;
        p.tracks[0].instrument = Some(Instrument::DrumSampler(Box::new(drums)));
        let other = add_track(&mut p, TrackKind::Instrument);
        p.track_mut(other).unwrap().instrument = Some(Instrument::Sampler(SamplerParams {
            file: Some(file),
            root_pitch: 48,
            ..SamplerParams::default()
        }));
        validate(&p).unwrap();
        let json = serde_json::to_vec(&p).unwrap();
        assert_eq!(from_json(&json).unwrap(), p);
        assert_eq!(p.tracks[0].synth(), None);
        assert_eq!(DrumParams::pad_for(41), Some(5));
        assert_eq!(DrumParams::pad_for(35), None);
        assert_eq!(DrumParams::pad_for(52), None);
    }

    #[test]
    fn safe_paths_stay_inside_the_project() {
        assert!(is_safe_relative_path("audio/take.wav"));
        assert!(is_safe_relative_path(r"audio\take.wav"));
        for bad in [
            "",
            "..",
            "audio/../../x.wav",
            "C:x.wav",
            r"C:\x.wav",
            r"\\server\share\x.wav",
            "/x",
        ] {
            assert!(!is_safe_relative_path(bad), "{bad}");
        }
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join("fnoob-core-tests")
            .join(format!("io-{name}-{}", std::process::id()))
            .join("Song.fnoob");
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = scratch_dir("roundtrip");
        let project = sample_project();
        save(&dir, &project).unwrap();
        assert_eq!(load(&dir).unwrap(), project);
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn newer_format_is_rejected() {
        let mut value = serde_json::to_value(sample_project()).unwrap();
        value["format_version"] = (FORMAT_VERSION + 1).into();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            from_json(&bytes),
            Err(ProjectError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn broken_json_is_rejected() {
        assert!(matches!(
            from_json(b"{\"format_version\": 1,"),
            Err(ProjectError::Json(_))
        ));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let mut value = serde_json::to_value(sample_project()).unwrap();
        value["from_the_future"] = true.into();
        assert!(from_json(&serde_json::to_vec(&value).unwrap()).is_ok());
    }

    #[test]
    fn invalid_values_are_rejected() {
        let cases: Vec<fn(&mut Project)> = vec![
            |p| p.tempo_bpm = 1000.0,
            |p| p.sample_rate = 10,
            |p| p.time_signature.denominator = 3,
            |p| p.loop_range.end_tick = p.loop_range.start_tick,
            |p| p.next_id = 1,
            |p| p.tracks[1].id = p.tracks[0].id,
            |p| p.tracks[0].regions[0].length_tick = 0,
            |p| {
                let notes = p.tracks[0].regions[0].notes_mut().unwrap();
                notes.push(Note {
                    pitch: 200,
                    velocity: 100,
                    start_tick: 0,
                    length_tick: 1,
                });
            },
            |p| {
                if let Some(Instrument::BasicSynth(s)) = &mut p.tracks[0].instrument {
                    s.sustain = 2.0;
                }
            },
            |p| {
                if let Some(Instrument::BasicSynth(s)) = &mut p.tracks[0].instrument {
                    s.attack_ms = f32::NAN;
                }
            },
            |p| {
                p.tracks[0].instrument = Some(Instrument::Sampler(SamplerParams {
                    file: Some(p.tracks[0].id),
                    ..SamplerParams::default()
                }))
            },
            |p| {
                let mut drums = DrumParams::default();
                drums.pads[3].pan = 2.0;
                p.tracks[0].instrument = Some(Instrument::DrumSampler(Box::new(drums)));
            },
            |p| p.audio_files[0].path = "../outside.wav".into(),
            |p| p.audio_files[0].path = "C:/Windows/evil.wav".into(),
            |p| p.audio_files[0].path = "/abs.wav".into(),
            |p| p.audio_files[0].path = String::new(),
            |p| p.audio_files[0].id = p.tracks[0].id,
            |p| p.audio_files.clear(),
            |p| {
                if let RegionContent::Audio { offset_frames, .. } =
                    &mut p.tracks[1].regions[0].content
                {
                    *offset_frames = 1_000_000;
                }
            },
            |p| {
                let region = p.tracks[1].regions.remove(0);
                p.tracks[0].regions.push(region);
            },
            |p| {
                p.tracks[1].audio_input = Some(AudioInput {
                    first_channel: 0,
                    channels: 3,
                })
            },
            |p| p.tracks[0].strip.volume_db = 12.0,
            |p| p.tracks[0].strip.pan = 1.5,
            |p| p.tracks[0].strip.send_mut(0).level_db = f32::NAN,
            |p| {
                p.returns.push(crate::model::ReturnTrack::default());
                p.returns[0].volume_db = -200.0;
            },
            |p| p.master.volume_db = 7.0,
            |p| p.tracks[0].regions[0].fade_in_tick = 1_000_000,
            |p| {
                p.tracks[0].automation.volume = vec![
                    AutomationPoint {
                        tick: 10,
                        value: 0.0,
                    },
                    AutomationPoint {
                        tick: 5,
                        value: 0.0,
                    },
                ]
            },
            |p| {
                p.tracks[0].automation.pan = vec![AutomationPoint {
                    tick: 0,
                    value: 3.0,
                }]
            },
            |p| {
                p.markers.push(Marker {
                    id: p.tracks[0].id,
                    tick: 0,
                    name: "A".into(),
                })
            },
            |p| {
                let mut slot = test_slot(p.next_id);
                p.next_id += 1;
                slot.class_id = "nothex".into();
                p.tracks[0].strip.inserts.push(slot);
            },
            |p| {
                let slot = test_slot(p.tracks[0].id);
                p.returns.push(crate::model::ReturnTrack::default());
                p.returns[0].inserts.push(slot);
            },
            |p| {
                for _ in 0..5 {
                    let slot = test_slot(p.next_id);
                    p.next_id += 1;
                    p.tracks[0].strip.inserts.push(slot);
                }
            },
        ];
        for (i, break_it) in cases.into_iter().enumerate() {
            let mut p = sample_project();
            break_it(&mut p);
            assert!(validate(&p).is_err(), "case {i} should be invalid");
        }
    }

    #[test]
    fn track_without_instrument_field_still_loads() {
        let mut value = serde_json::to_value(sample_project()).unwrap();
        value["tracks"][0]
            .as_object_mut()
            .unwrap()
            .remove("instrument");
        let project = from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(project.tracks[0].instrument, None);
        assert!(project.tracks[0].synth().is_some());
    }

    #[test]
    fn saves_keep_at_most_ten_backups() {
        let dir = scratch_dir("backups");
        let mut project = sample_project();
        for i in 0..13 {
            project.name = format!("v{i}");
            save(&dir, &project).unwrap();
        }
        let backups: Vec<_> = std::fs::read_dir(dir.join(BACKUP_DIR))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(backups.len(), MAX_BACKUPS);
        assert_eq!(load(&dir).unwrap().name, "v12");
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn autosave_is_offered_only_when_newer() {
        let dir = scratch_dir("autosave");
        let mut project = sample_project();
        save(&dir, &project).unwrap();
        assert_eq!(newer_autosave(&dir), None);
        std::thread::sleep(std::time::Duration::from_millis(30));
        project.name = "changed".into();
        save_autosave(&dir, &project).unwrap();
        assert!(newer_autosave(&dir).is_some());
        assert_eq!(load_autosave(&dir).unwrap().name, "changed");
        std::thread::sleep(std::time::Duration::from_millis(30));
        save(&dir, &project).unwrap();
        assert_eq!(newer_autosave(&dir), None);
        remove_autosave(&dir);
        assert!(load_autosave(&dir).is_err());
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn backup_names_use_calendar_time() {
        let time = UNIX_EPOCH + std::time::Duration::from_secs(1_763_700_000);
        assert_eq!(civil_time(time), "20251121-044000");
        assert_eq!(civil_time(UNIX_EPOCH), "19700101-000000");
    }

    #[test]
    fn invalid_project_is_not_saved() {
        let dir = scratch_dir("invalid");
        let mut p = sample_project();
        p.tempo_bpm = 0.0;
        assert!(save(&dir, &p).is_err());
        assert!(!dir.join(PROJECT_FILE).exists());
    }
}
