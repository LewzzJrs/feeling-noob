use crate::app::App;
use eframe::egui::{self, Key};
use fnoob_engine::Command as EngineCommand;
use fnoob_engine::processor::MidiEvent;
use std::collections::HashMap;

const NOTE_KEYS: [(Key, u8); 17] = [
    (Key::A, 0),
    (Key::W, 1),
    (Key::S, 2),
    (Key::E, 3),
    (Key::D, 4),
    (Key::F, 5),
    (Key::T, 6),
    (Key::G, 7),
    (Key::Y, 8),
    (Key::H, 9),
    (Key::U, 10),
    (Key::J, 11),
    (Key::K, 12),
    (Key::O, 13),
    (Key::L, 14),
    (Key::P, 15),
    (Key::Semicolon, 16),
];

pub struct MusicalTyping {
    pub on: bool,
    octave: i8,
    velocity: u8,
    held: HashMap<Key, u8>,
}

impl Default for MusicalTyping {
    fn default() -> Self {
        Self {
            on: false,
            octave: 4,
            velocity: 100,
            held: HashMap::new(),
        }
    }
}

impl MusicalTyping {
    pub fn label(&self) -> String {
        if self.on {
            format!("Keys C{} v{}", self.octave, self.velocity)
        } else {
            "Keys".to_owned()
        }
    }

    pub fn pitch(&self, key: Key) -> Option<u8> {
        let offset = NOTE_KEYS.iter().find(|(k, _)| *k == key)?.1;
        let pitch = (i16::from(self.octave) + 1) * 12 + i16::from(offset);
        u8::try_from(pitch).ok().filter(|p| *p <= 127)
    }
}

enum KeyAction {
    Note { key: Key, pressed: bool },
    Octave(i8),
    Velocity(i16),
}

fn key_action(key: Key) -> Option<KeyAction> {
    match key {
        Key::Z => Some(KeyAction::Octave(-1)),
        Key::X => Some(KeyAction::Octave(1)),
        Key::C => Some(KeyAction::Velocity(-20)),
        Key::V => Some(KeyAction::Velocity(20)),
        _ if NOTE_KEYS.iter().any(|(k, _)| *k == key) => {
            Some(KeyAction::Note { key, pressed: true })
        }
        _ => None,
    }
}

impl App {
    pub(crate) fn toggle_musical_typing(&mut self) {
        self.musical.on = !self.musical.on;
        if !self.musical.on {
            self.release_musical_keys();
        }
        self.message = Some(if self.musical.on {
            "Musical typing on: A-L play notes, Z/X change octave, C/V change velocity.".into()
        } else {
            "Musical typing off.".into()
        });
    }

    fn release_musical_keys(&mut self) {
        let held: Vec<u8> = self.musical.held.drain().map(|(_, p)| p).collect();
        for pitch in held {
            self.send(EngineCommand::LiveNote(MidiEvent { pitch, velocity: 0 }));
        }
    }

    pub(crate) fn take_musical_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let mut actions = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } = event
                else {
                    return true;
                };
                if modifiers.ctrl || modifiers.alt || modifiers.command {
                    return true;
                }
                let Some(action) = key_action(*key) else {
                    return true;
                };
                if !*repeat {
                    actions.push(match action {
                        KeyAction::Note { key, .. } => KeyAction::Note {
                            key,
                            pressed: *pressed,
                        },
                        other if *pressed => other,
                        _ => return false,
                    });
                }
                false
            });
        });
        for action in actions {
            match action {
                KeyAction::Note { key, pressed: true } => {
                    if self.musical.held.contains_key(&key) {
                        continue;
                    }
                    if let Some(pitch) = self.musical.pitch(key) {
                        self.musical.held.insert(key, pitch);
                        let velocity = self.musical.velocity;
                        self.send(EngineCommand::LiveNote(MidiEvent { pitch, velocity }));
                    }
                }
                KeyAction::Note {
                    key,
                    pressed: false,
                } => {
                    if let Some(pitch) = self.musical.held.remove(&key) {
                        self.send(EngineCommand::LiveNote(MidiEvent { pitch, velocity: 0 }));
                    }
                }
                KeyAction::Octave(step) => {
                    self.release_musical_keys();
                    self.musical.octave = (self.musical.octave + step).clamp(-1, 8);
                }
                KeyAction::Velocity(step) => {
                    self.musical.velocity =
                        (i16::from(self.musical.velocity) + step).clamp(1, 127) as u8;
                }
            }
        }
    }

    pub(crate) fn marker_rename_window(&mut self, ctx: &egui::Context) {
        let Some((id, text)) = &mut self.marker_rename else {
            return;
        };
        let id = *id;
        let mut done = None;
        egui::Modal::new(egui::Id::unique("marker_rename")).show(ctx, |ui| {
            ui.heading("Rename marker");
            let edit = ui.text_edit_singleline(text);
            edit.request_focus();
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) => {
                let name = self
                    .marker_rename
                    .take()
                    .map(|(_, name)| name)
                    .unwrap_or_default();
                self.edit(|p| {
                    fnoob_core::edit::rename_marker(p, id, &name);
                });
            }
            Some(false) => self.marker_rename = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_notes_around_the_octave() {
        let mut typing = MusicalTyping::default();
        assert_eq!(typing.pitch(Key::A), Some(60));
        assert_eq!(typing.pitch(Key::K), Some(72));
        assert_eq!(typing.pitch(Key::Q), None);
        typing.octave = 8;
        assert_eq!(typing.pitch(Key::Semicolon), Some(124));
        assert_eq!(typing.pitch(Key::A), Some(108));
        typing.octave = -1;
        assert_eq!(typing.pitch(Key::A), Some(0));
    }
}
