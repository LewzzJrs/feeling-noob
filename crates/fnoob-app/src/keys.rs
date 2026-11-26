use eframe::egui;
use fnoob_core::keymap::{Command, Key, KeyCombo, lookup};

fn map_key(key: egui::Key) -> Option<Key> {
    match key {
        egui::Key::Space => Some(Key::Space),
        egui::Key::Tab => Some(Key::Tab),
        egui::Key::Home => Some(Key::Home),
        egui::Key::Plus => Some(Key::Plus),
        egui::Key::Minus => Some(Key::Minus),
        egui::Key::Equals => Some(Key::Equals),
        egui::Key::F1 => Some(Key::Function(1)),
        egui::Key::F2 => Some(Key::Function(2)),
        egui::Key::F3 => Some(Key::Function(3)),
        egui::Key::F4 => Some(Key::Function(4)),
        egui::Key::F5 => Some(Key::Function(5)),
        egui::Key::F6 => Some(Key::Function(6)),
        egui::Key::F7 => Some(Key::Function(7)),
        egui::Key::F8 => Some(Key::Function(8)),
        egui::Key::F9 => Some(Key::Function(9)),
        egui::Key::F10 => Some(Key::Function(10)),
        egui::Key::F11 => Some(Key::Function(11)),
        egui::Key::F12 => Some(Key::Function(12)),
        egui::Key::Num0 => Some(Key::Digit(0)),
        egui::Key::Num1 => Some(Key::Digit(1)),
        egui::Key::Num2 => Some(Key::Digit(2)),
        egui::Key::Num3 => Some(Key::Digit(3)),
        egui::Key::Num4 => Some(Key::Digit(4)),
        egui::Key::Num5 => Some(Key::Digit(5)),
        egui::Key::Num6 => Some(Key::Digit(6)),
        egui::Key::Num7 => Some(Key::Digit(7)),
        egui::Key::Num8 => Some(Key::Digit(8)),
        egui::Key::Num9 => Some(Key::Digit(9)),
        egui::Key::Enter => Some(Key::Enter),
        egui::Key::Comma => Some(Key::Comma),
        egui::Key::Period => Some(Key::Period),
        egui::Key::Delete => Some(Key::Delete),
        egui::Key::Backspace => Some(Key::Backspace),
        egui::Key::ArrowLeft => Some(Key::Left),
        egui::Key::ArrowRight => Some(Key::Right),
        egui::Key::ArrowUp => Some(Key::Up),
        egui::Key::ArrowDown => Some(Key::Down),
        other => {
            let mut chars = other.name().chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphabetic() => {
                    Some(Key::Letter(c.to_ascii_uppercase()))
                }
                _ => None,
            }
        }
    }
}

fn repeat_allowed(command: Command) -> bool {
    matches!(
        command,
        Command::NudgeLeft
            | Command::NudgeRight
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::MoveLeft
            | Command::MoveRight
            | Command::ExtendLeft
            | Command::ExtendRight
            | Command::ExtendUp
            | Command::ExtendDown
            | Command::LoopShorter
            | Command::LoopLonger
            | Command::SelectPreviousTrack
            | Command::SelectNextTrack
    )
}

pub fn take_commands(ctx: &egui::Context) -> Vec<Command> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    ctx.input_mut(|input| {
        let mut commands = Vec::new();
        input.events.retain(|event| {
            let clipboard = match event {
                egui::Event::Cut => Some(Command::Cut),
                egui::Event::Copy => Some(Command::Copy),
                egui::Event::Paste(_) | egui::Event::PasteImage(_) => Some(Command::Paste),
                _ => None,
            };
            if let Some(command) = clipboard {
                commands.push(command);
                return false;
            }
            let egui::Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            let Some(key) = map_key(*key) else {
                return true;
            };
            let combo = KeyCombo {
                key,
                ctrl: modifiers.ctrl,
                alt: modifiers.alt,
                shift: modifiers.shift,
            };
            match lookup(combo) {
                Some(command) => {
                    if !*repeat || repeat_allowed(command) {
                        commands.push(command);
                    }
                    false
                }
                None => true,
            }
        });
        commands
    })
}
