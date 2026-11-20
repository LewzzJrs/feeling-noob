use eframe::egui;
use fnoob_core::keymap::{Command, Key, KeyCombo, lookup};

fn map_key(key: egui::Key) -> Option<Key> {
    match key {
        egui::Key::Space => Some(Key::Space),
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
