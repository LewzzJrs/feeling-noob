use crate::fsutil::write_atomic;
use crate::model::{
    FORMAT_VERSION, Instrument, MAX_GAIN_DB, MIN_GAIN_DB, Project, RegionContent, Track,
};
use crate::time::{MAX_BPM, MIN_BPM};
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::Path;

pub const PROJECT_FILE: &str = "project.json";
pub const PROJECT_EXTENSION: &str = "fnoob";
pub const AUDIO_DIR: &str = "audio";

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
    write_atomic(&dir.join(PROJECT_FILE), &bytes)?;
    Ok(())
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
    let project: Project = serde_json::from_slice(bytes)?;
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
        for region in &track.regions {
            if !ids.insert(region.id) {
                return Err(format!("duplicate id {}", region.id));
            }
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

fn validate_track(project: &Project, track: &Track) -> Result<(), String> {
    if let Some(Instrument::BasicSynth(params)) = &track.instrument
        && !params.is_valid()
    {
        return Err(format!("track {} has invalid synth settings", track.id));
    }
    if let Some(input) = track.audio_input
        && !(1..=2).contains(&input.channels)
    {
        return Err(format!("track {} has an invalid audio input", track.id));
    }
    for region in &track.regions {
        if region.length_tick == 0 {
            return Err(format!("region {} has zero length", region.id));
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
    use crate::model::{AudioInput, Note, TrackKind};

    fn sample_project() -> Project {
        let mut p = Project::new("Song", 48_000);
        let track = add_track(&mut p, TrackKind::Instrument);
        add_midi_region(&mut p, track, 0, 3840);
        let audio = add_track(&mut p, TrackKind::Audio);
        let file = add_audio_file(&mut p, "audio/Audio 1-01.wav".into(), 1, 48_000);
        add_audio_region(&mut p, audio, 0, file, 0);
        p
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
    fn invalid_project_is_not_saved() {
        let dir = scratch_dir("invalid");
        let mut p = sample_project();
        p.tempo_bpm = 0.0;
        assert!(save(&dir, &p).is_err());
        assert!(!dir.join(PROJECT_FILE).exists());
    }
}
