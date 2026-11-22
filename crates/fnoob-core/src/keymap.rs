#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    PlayStop,
    Record,
    GoToBeginning,
    Rewind,
    Forward,
    ToggleLoop,
    ToggleMetronome,
    ToggleCountIn,
    ToggleMixer,
    TogglePianoRoll,
    ToggleEditor,
    ToggleTrackMute,
    ToggleTrackSolo,
    ToggleTrackRecord,
    NewTracks,
    SplitAtPlayhead,
    RepeatRegions,
    Quantize,
    ToggleSnap,
    MuteRegions,
    NudgeLeft,
    NudgeRight,
    Bounce,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Delete,
    ZoomOut,
    ZoomIn,
    SelectPreviousTrack,
    SelectNextTrack,
    NewProject,
    Open,
    Save,
    SaveAs,
    AddMarker,
    PreviousMarker,
    NextMarker,
    ToggleAutomation,
    ToggleMusicalTyping,
    DuplicateTrack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Space,
    Enter,
    Comma,
    Period,
    Delete,
    Backspace,
    Left,
    Right,
    Up,
    Down,
    Letter(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

const fn plain(key: Key) -> KeyCombo {
    KeyCombo {
        key,
        ctrl: false,
        alt: false,
        shift: false,
    }
}

const fn ctrl(key: Key) -> KeyCombo {
    KeyCombo {
        key,
        ctrl: true,
        alt: false,
        shift: false,
    }
}

const fn shift(key: Key) -> KeyCombo {
    KeyCombo {
        key,
        ctrl: false,
        alt: false,
        shift: true,
    }
}

const fn alt(key: Key) -> KeyCombo {
    KeyCombo {
        key,
        ctrl: false,
        alt: true,
        shift: false,
    }
}

const fn ctrl_alt(key: Key) -> KeyCombo {
    KeyCombo {
        key,
        ctrl: true,
        alt: true,
        shift: false,
    }
}

const fn ctrl_shift(key: Key) -> KeyCombo {
    KeyCombo {
        key,
        ctrl: true,
        alt: false,
        shift: true,
    }
}

const fn letter(c: char) -> Key {
    Key::Letter(c)
}

pub const BINDINGS: &[(KeyCombo, Command)] = &[
    (alt(letter('M')), Command::AddMarker),
    (alt(Key::Comma), Command::PreviousMarker),
    (alt(Key::Period), Command::NextMarker),
    (plain(letter('A')), Command::ToggleAutomation),
    (ctrl(letter('K')), Command::ToggleMusicalTyping),
    (ctrl(letter('D')), Command::DuplicateTrack),
    (plain(Key::Space), Command::PlayStop),
    (plain(letter('R')), Command::Record),
    (plain(Key::Enter), Command::GoToBeginning),
    (plain(Key::Comma), Command::Rewind),
    (plain(Key::Period), Command::Forward),
    (plain(letter('C')), Command::ToggleLoop),
    (plain(letter('K')), Command::ToggleMetronome),
    (shift(letter('K')), Command::ToggleCountIn),
    (plain(letter('X')), Command::ToggleMixer),
    (plain(letter('P')), Command::TogglePianoRoll),
    (plain(letter('E')), Command::ToggleEditor),
    (plain(letter('M')), Command::ToggleTrackMute),
    (plain(letter('S')), Command::ToggleTrackSolo),
    (ctrl_alt(letter('R')), Command::ToggleTrackRecord),
    (ctrl_alt(letter('N')), Command::NewTracks),
    (ctrl(letter('T')), Command::SplitAtPlayhead),
    (ctrl(letter('R')), Command::RepeatRegions),
    (plain(letter('Q')), Command::Quantize),
    (ctrl(letter('G')), Command::ToggleSnap),
    (ctrl_alt(letter('M')), Command::MuteRegions),
    (alt(Key::Left), Command::NudgeLeft),
    (alt(Key::Right), Command::NudgeRight),
    (ctrl(letter('B')), Command::Bounce),
    (ctrl(letter('Z')), Command::Undo),
    (ctrl_shift(letter('Z')), Command::Redo),
    (ctrl(letter('X')), Command::Cut),
    (ctrl(letter('C')), Command::Copy),
    (ctrl(letter('V')), Command::Paste),
    (ctrl(letter('A')), Command::SelectAll),
    (plain(Key::Delete), Command::Delete),
    (plain(Key::Backspace), Command::Delete),
    (ctrl(Key::Left), Command::ZoomOut),
    (ctrl(Key::Right), Command::ZoomIn),
    (plain(Key::Up), Command::SelectPreviousTrack),
    (plain(Key::Down), Command::SelectNextTrack),
    (ctrl_shift(letter('N')), Command::NewProject),
    (ctrl(letter('O')), Command::Open),
    (ctrl(letter('S')), Command::Save),
    (ctrl_shift(letter('S')), Command::SaveAs),
];

pub fn lookup(combo: KeyCombo) -> Option<Command> {
    BINDINGS
        .iter()
        .find(|(c, _)| *c == combo)
        .map(|&(_, command)| command)
}

fn key_label(key: Key) -> String {
    match key {
        Key::Space => "Space".into(),
        Key::Enter => "Enter".into(),
        Key::Comma => ",".into(),
        Key::Period => ".".into(),
        Key::Delete => "Delete".into(),
        Key::Backspace => "Backspace".into(),
        Key::Left => "Left".into(),
        Key::Right => "Right".into(),
        Key::Up => "Up".into(),
        Key::Down => "Down".into(),
        Key::Letter(c) => c.to_string(),
    }
}

pub fn shortcut_label(command: Command) -> Option<String> {
    let (combo, _) = BINDINGS.iter().find(|(_, c)| *c == command)?;
    let mut label = String::new();
    if combo.ctrl {
        label.push_str("Ctrl+");
    }
    if combo.alt {
        label.push_str("Alt+");
    }
    if combo.shift {
        label.push_str("Shift+");
    }
    label.push_str(&key_label(combo.key));
    Some(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_combo_is_bound_twice() {
        let mut seen = HashSet::new();
        for (combo, _) in BINDINGS {
            assert!(seen.insert(*combo), "{combo:?} bound twice");
        }
    }

    #[test]
    fn letters_are_uppercase() {
        for (combo, _) in BINDINGS {
            if let Key::Letter(c) = combo.key {
                assert!(c.is_ascii_uppercase());
            }
        }
    }

    #[test]
    fn lookup_respects_modifiers() {
        assert_eq!(lookup(plain(letter('R'))), Some(Command::Record));
        assert_eq!(lookup(ctrl(letter('R'))), Some(Command::RepeatRegions));
        assert_eq!(
            lookup(ctrl_alt(letter('R'))),
            Some(Command::ToggleTrackRecord)
        );
        assert_eq!(lookup(shift(letter('R'))), None);
    }

    #[test]
    fn labels() {
        assert_eq!(
            shortcut_label(Command::Redo).as_deref(),
            Some("Ctrl+Shift+Z")
        );
        assert_eq!(
            shortcut_label(Command::ToggleTrackRecord).as_deref(),
            Some("Ctrl+Alt+R")
        );
        assert_eq!(shortcut_label(Command::PlayStop).as_deref(), Some("Space"));
    }
}
