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
    ExportMidiClip,
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
    ContinuePlay,
    BackToArrangement,
    ShowClipView,
    ShowDeviceView,
    ToggleReturns,
    ToggleInOut,
    ToggleSends,
    OpenSettings,
    Quit,
    QuantizeOff,
    QuantizeSixteenth,
    QuantizeEighth,
    QuantizeQuarter,
    QuantizeBar,
    NarrowGrid,
    WidenGrid,
    InsertMidiClip,
    LaunchSelected,
    StopSelectedTrack,
    TrackActivator(u8),
    ShowTakeLanes,
    SetLoopStart,
    SetLoopEnd,
    LoopShorter,
    LoopLonger,
    LoopHalve,
    LoopDouble,
    MoveLeft,
    MoveRight,
    PlayFromInsert,
    InsertToPlayhead,
    ZoomToSelection,
    ZoomBack,
    FitSong,
    CutTime,
    PasteTime,
    DuplicateTime,
    DeleteTime,
    Consolidate,
    Crop,
    Reverse,
    CreateFade,
    GroupTracks,
    Ungroup,
    FreezeTrack,
    FlattenTrack,
    ExtendLeft,
    ExtendRight,
    ExtendUp,
    ExtendDown,
    SessionRecord,
    CaptureScene,
    TripletGrid,
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

const fn alt_shift(key: Key) -> KeyCombo {
    KeyCombo {
        key,
        ctrl: false,
        alt: true,
        shift: true,
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
    (shift(Key::Space), Command::ContinuePlay),
    (ctrl(Key::Space), Command::PlayFromInsert),
    (ctrl_shift(Key::Space), Command::InsertToPlayhead),
    (plain(letter('Z')), Command::ZoomToSelection),
    (plain(letter('X')), Command::ZoomBack),
    (plain(letter('W')), Command::FitSong),
    (plain(Key::Function(9)), Command::Record),
    (plain(Key::Function(10)), Command::BackToArrangement),
    (plain(Key::Home), Command::GoToBeginning),
    (plain(Key::Left), Command::MoveLeft),
    (shift(Key::Left), Command::ExtendLeft),
    (shift(Key::Right), Command::ExtendRight),
    (shift(Key::Up), Command::ExtendUp),
    (shift(Key::Down), Command::ExtendDown),
    (plain(Key::Right), Command::MoveRight),
    (alt(Key::Left), Command::NudgeLeft),
    (alt(Key::Right), Command::NudgeRight),
    (ctrl(letter('L')), Command::ToggleLoop),
    (ctrl(Key::Function(10)), Command::SetLoopStart),
    (ctrl(Key::Function(11)), Command::SetLoopEnd),
    (ctrl(Key::Left), Command::LoopShorter),
    (ctrl(Key::Right), Command::LoopLonger),
    (ctrl(Key::Down), Command::LoopHalve),
    (ctrl(Key::Up), Command::LoopDouble),
    (plain(letter('O')), Command::ToggleMetronome),
    (plain(Key::Tab), Command::ToggleClipGrid),
    (shift(Key::Tab), Command::SwitchDetail),
    (plain(Key::Function(12)), Command::SwitchDetail),
    (ctrl_alt(letter('L')), Command::ToggleDetail),
    (ctrl_alt(Key::Digit(3)), Command::ShowClipView),
    (ctrl_alt(Key::Digit(4)), Command::ShowDeviceView),
    (ctrl_alt(letter('B')), Command::ToggleBrowser),
    (ctrl_alt(Key::Digit(5)), Command::ToggleBrowser),
    (ctrl_alt(letter('M')), Command::ToggleMixer),
    (ctrl_alt(letter('I')), Command::ToggleInOut),
    (ctrl_alt(letter('S')), Command::ToggleSends),
    (ctrl_alt(letter('R')), Command::ToggleReturns),
    (ctrl_alt(letter('U')), Command::ShowTakeLanes),
    (ctrl(Key::Comma), Command::OpenSettings),
    (ctrl(letter('Q')), Command::Quit),
    (plain(Key::Digit(0)), Command::MuteRegions),
    (plain(letter('S')), Command::ToggleTrackSolo),
    (plain(letter('C')), Command::ToggleTrackRecord),
    (plain(Key::Function(1)), Command::TrackActivator(0)),
    (plain(Key::Function(2)), Command::TrackActivator(1)),
    (plain(Key::Function(3)), Command::TrackActivator(2)),
    (plain(Key::Function(4)), Command::TrackActivator(3)),
    (plain(Key::Function(5)), Command::TrackActivator(4)),
    (plain(Key::Function(6)), Command::TrackActivator(5)),
    (plain(Key::Function(7)), Command::TrackActivator(6)),
    (plain(Key::Function(8)), Command::TrackActivator(7)),
    (ctrl(letter('T')), Command::NewAudioTrack),
    (ctrl_shift(letter('T')), Command::NewMidiTrack),
    (ctrl_alt(letter('T')), Command::NewReturnTrack),
    (ctrl_shift(letter('M')), Command::InsertMidiClip),
    (ctrl(letter('I')), Command::InsertScene),
    (ctrl_shift(letter('I')), Command::CaptureScene),
    (ctrl_shift(Key::Function(9)), Command::SessionRecord),
    (plain(Key::Enter), Command::LaunchSelected),
    (ctrl(Key::Enter), Command::StopSelectedTrack),
    (ctrl(letter('E')), Command::SplitAtPlayhead),
    (ctrl(letter('D')), Command::Duplicate),
    (ctrl(letter('U')), Command::Quantize),
    (ctrl(Key::Digit(1)), Command::NarrowGrid),
    (ctrl(Key::Digit(2)), Command::WidenGrid),
    (ctrl(Key::Digit(3)), Command::TripletGrid),
    (ctrl(Key::Digit(4)), Command::ToggleSnap),
    (ctrl(Key::Digit(6)), Command::QuantizeSixteenth),
    (ctrl(Key::Digit(7)), Command::QuantizeEighth),
    (ctrl(Key::Digit(8)), Command::QuantizeQuarter),
    (ctrl(Key::Digit(9)), Command::QuantizeBar),
    (ctrl(Key::Digit(0)), Command::QuantizeOff),
    (ctrl_shift(letter('R')), Command::Bounce),
    (ctrl_shift(letter('E')), Command::ExportMidiClip),
    (ctrl(letter('Z')), Command::Undo),
    (ctrl(letter('Y')), Command::Redo),
    (ctrl(letter('X')), Command::Cut),
    (ctrl_shift(letter('X')), Command::CutTime),
    (ctrl_shift(letter('V')), Command::PasteTime),
    (ctrl_shift(letter('D')), Command::DuplicateTime),
    (ctrl_shift(Key::Delete), Command::DeleteTime),
    (ctrl(letter('J')), Command::Consolidate),
    (ctrl_shift(letter('J')), Command::Crop),
    (plain(letter('R')), Command::Reverse),
    (ctrl_alt(letter('F')), Command::CreateFade),
    (ctrl(letter('G')), Command::GroupTracks),
    (
        KeyCombo {
            key: letter('F'),
            ctrl: true,
            alt: true,
            shift: true,
        },
        Command::FreezeTrack,
    ),
    (ctrl_shift(letter('G')), Command::Ungroup),
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
    (plain(letter('B')), Command::ToggleDrawMode),
    (plain(letter('A')), Command::ToggleAutomation),
    (plain(letter('M')), Command::ToggleMusicalTyping),
    (ctrl_shift(letter('F')), Command::ToggleFollow),
    (alt_shift(letter('F')), Command::ToggleFollow),
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
        assert_eq!(lookup(plain(letter('R'))), Some(Command::Reverse));
        assert_eq!(lookup(plain(letter('Q'))), None);
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
