mod com;
mod editor;
mod instance;
mod module;

pub use editor::Editor;
pub use instance::{Plugin, PluginProcessor, Transport, instantiate};
pub use module::{ClassInfo, Module, scan};

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    Load(String),
    NoFactory,
    Create(&'static str),
    Setup(&'static str),
    State(&'static str),
    Editor(&'static str),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(e) => write!(f, "cannot load the plugin file: {e}"),
            Self::NoFactory => write!(f, "the file is not a VST3 plugin"),
            Self::Create(what) => write!(f, "the plugin could not create its {what}"),
            Self::Setup(what) => write!(f, "the plugin refused {what}"),
            Self::State(what) => write!(f, "the plugin state could not be {what}"),
            Self::Editor(what) => write!(f, "the plugin editor failed: {what}"),
        }
    }
}

impl std::error::Error for PluginError {}

pub fn cid_to_hex(cid: &[u8; 16]) -> String {
    cid.iter().map(|b| format!("{b:02X}")).collect()
}

pub fn cid_from_hex(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 || !text.is_ascii() {
        return None;
    }
    let mut cid = [0u8; 16];
    for (i, byte) in cid.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(cid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_ids_round_trip_as_hex() {
        let cid = [0xAB, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0xFF];
        let hex = cid_to_hex(&cid);
        assert_eq!(hex.len(), 32);
        assert_eq!(cid_from_hex(&hex), Some(cid));
        assert_eq!(cid_from_hex("xyz"), None);
        assert_eq!(cid_from_hex(&"G".repeat(32)), None);
    }
}
