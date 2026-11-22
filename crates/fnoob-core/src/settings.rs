use crate::fsutil::write_atomic;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub output_device: Option<String>,
    pub input_device: InputChoice,
    pub buffer_frames: Option<u32>,
    pub record_format: RecordFormat,
    pub midi_input: MidiInputChoice,
    pub plugin_folders: Vec<String>,
    pub recent_projects: Vec<String>,
}

pub const MAX_RECENT: usize = 10;

impl Settings {
    pub fn add_recent(&mut self, dir: &Path) {
        let entry = dir.display().to_string();
        self.recent_projects
            .retain(|p| !p.eq_ignore_ascii_case(&entry));
        self.recent_projects.insert(0, entry);
        self.recent_projects.truncate(MAX_RECENT);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordFormat {
    Int16,
    Int24,
    #[default]
    Float32,
}

impl RecordFormat {
    pub const ALL: [RecordFormat; 3] = [Self::Int16, Self::Int24, Self::Float32];

    pub fn label(self) -> &'static str {
        match self {
            Self::Int16 => "16-bit",
            Self::Int24 => "24-bit",
            Self::Float32 => "32-bit float",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputChoice {
    #[default]
    SystemDefault,
    Off,
    Device(String),
}

pub const BUFFER_SIZES: [u32; 5] = [128, 256, 512, 1024, 2048];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiInputChoice {
    #[default]
    FirstAvailable,
    Off,
    Port(String),
}

#[derive(Debug)]
pub enum SettingsError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "settings file error: {e}"),
            Self::Json(e) => write!(f, "settings file is not valid: {e}"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<io::Error> for SettingsError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub fn default_settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| {
        PathBuf::from(dir)
            .join("feeling-noob")
            .join("settings.json")
    })
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let bytes = serde_json::to_vec_pretty(self)?;
        write_atomic(path, &bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_file(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("fnoob-core-tests")
            .join(format!("settings-{name}-{}", std::process::id()))
            .join("settings.json")
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = scratch_file("missing");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
    }

    #[test]
    fn round_trip() {
        let path = scratch_file("roundtrip");
        let settings = Settings {
            output_device: Some("wasapi:{0.0.0.00000000}.{abc}".into()),
            input_device: InputChoice::Device("wasapi:{in}".into()),
            buffer_frames: Some(256),
            record_format: RecordFormat::Int24,
            midi_input: MidiInputChoice::Port("Keyboard 1".into()),
            plugin_folders: vec!["D:/Plugins".into()],
            recent_projects: vec!["D:/Songs/A.fnoob".into()],
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let s: Settings = serde_json::from_str(r#"{"output_device":"x","future":1}"#).unwrap();
        assert_eq!(s.output_device.as_deref(), Some("x"));
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let path = scratch_file("corrupt");
        write_atomic(&path, b"{not json").unwrap();
        assert!(matches!(Settings::load(&path), Err(SettingsError::Json(_))));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn recent_projects_move_to_the_front_and_are_capped() {
        let mut settings = Settings::default();
        for i in 0..12 {
            settings.add_recent(Path::new(&format!("C:/s/{i}.fnoob")));
        }
        settings.add_recent(Path::new("C:/s/5.fnoob"));
        assert_eq!(settings.recent_projects.len(), MAX_RECENT);
        assert_eq!(settings.recent_projects[0], "C:/s/5.fnoob");
        let copies = settings
            .recent_projects
            .iter()
            .filter(|p| p.as_str() == "C:/s/5.fnoob")
            .count();
        assert_eq!(copies, 1);
    }
}
