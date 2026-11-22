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
    ToggleDetail,
    SwitchDetail,
    ToggleBrowser,
    ToggleTrackMute,
    ToggleTrackSolo,
    ToggleTrackRecord,
    NewTracks,
    NewAudioTrack,
    NewMidiTrack,
    NewReturnTrack,
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
    Duplicate,
    Rename,
    InsertScene,
    ToggleDrawMode,
    ToggleFollow,
    ToggleClipGrid,
    CaptureMidi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Space,
    Tab,
    Home,
    Plus,
    Minus,
    Equals,
    Function(u8),
    Digit(u8),
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
    (plain(Key::Space), Command::PlayStop),
    (plain(Key::Function(9)), Command::Record),
    (plain(Key::Home), Command::GoToBeginning),
    (plain(Key::Left), Command::Rewind),
    (plain(Key::Right), Command::Forward),
    (ctrl(letter('L')), Command::ToggleLoop),
    (plain(letter('O')), Command::ToggleMetronome),
    (ctrl_alt(letter('M')), Command::ToggleMixer),
    (ctrl_alt(letter('L')), Command::ToggleDetail),
    (shift(Key::Tab), Command::SwitchDetail),
    (plain(Key::Tab), Command::ToggleClipGrid),
    (ctrl_alt(letter('B')), Command::ToggleBrowser),
    (plain(Key::Digit(0)), Command::MuteRegions),
    (ctrl(letter('T')), Command::NewAudioTrack),
    (ctrl_shift(letter('T')), Command::NewMidiTrack),
    (ctrl_alt(letter('T')), Command::NewReturnTrack),
    (ctrl(letter('E')), Command::SplitAtPlayhead),
    (ctrl(letter('D')), Command::Duplicate),
    (ctrl(letter('U')), Command::Quantize),
    (ctrl(Key::Digit(4)), Command::ToggleSnap),
    (alt(Key::Left), Command::NudgeLeft),
    (alt(Key::Right), Command::NudgeRight),
    (ctrl_shift(letter('R')), Command::Bounce),
    (ctrl(letter('Z')), Command::Undo),
    (ctrl(letter('Y')), Command::Redo),
    (ctrl(letter('X')), Command::Cut),
    (ctrl(letter('C')), Command::Copy),
    (ctrl(letter('V')), Command::Paste),
    (ctrl(letter('A')), Command::SelectAll),
    (plain(Key::Delete), Command::Delete),
    (plain(Key::Backspace), Command::Delete),
    (plain(Key::Plus), Command::ZoomIn),
    (plain(Key::Equals), Command::ZoomIn),
    (plain(Key::Minus), Command::ZoomOut),
    (plain(Key::Up), Command::SelectPreviousTrack),
    (plain(Key::Down), Command::SelectNextTrack),
    (ctrl(letter('N')), Command::NewProject),
    (ctrl(letter('O')), Command::Open),
    (ctrl(letter('S')), Command::Save),
    (ctrl_shift(letter('S')), Command::SaveAs),
    (ctrl(letter('R')), Command::Rename),
    (ctrl(letter('I')), Command::InsertScene),
    (plain(letter('B')), Command::ToggleDrawMode),
    (plain(letter('A')), Command::ToggleAutomation),
    (plain(letter('M')), Command::ToggleMusicalTyping),
    (ctrl_shift(letter('F')), Command::ToggleFollow),
    (ctrl_shift(letter('C')), Command::CaptureMidi),
    (alt(letter('M')), Command::AddMarker),
    (alt(Key::Comma), Command::PreviousMarker),
    (alt(Key::Period), Command::NextMarker),
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
        Key::Tab => "Tab".into(),
        Key::Home => "Home".into(),
        Key::Plus => "+".into(),
        Key::Minus => "-".into(),
        Key::Equals => "=".into(),
        Key::Function(n) => format!("F{n}"),
        Key::Digit(n) => n.to_string(),
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
        assert_eq!(lookup(plain(Key::Function(9))), Some(Command::Record));
        assert_eq!(lookup(ctrl(letter('T'))), Some(Command::NewAudioTrack));
        assert_eq!(lookup(ctrl_shift(letter('T'))), Some(Command::NewMidiTrack));
        assert_eq!(lookup(plain(letter('R'))), None);
    }

    #[test]
    fn labels() {
        assert_eq!(shortcut_label(Command::Redo).as_deref(), Some("Ctrl+Y"));
        assert_eq!(
            shortcut_label(Command::ToggleDetail).as_deref(),
            Some("Ctrl+Alt+L")
        );
        assert_eq!(shortcut_label(Command::Record).as_deref(), Some("F9"));
        assert_eq!(
            shortcut_label(Command::ToggleSnap).as_deref(),
            Some("Ctrl+4")
        );
    }
}
