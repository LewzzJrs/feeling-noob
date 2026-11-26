use crate::app::App;
use crate::mixer::MixTarget;
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind};
use fnoob_core::midi_map::{self, MapTarget};
use fnoob_engine::midi_in::ControlEvent;
use std::collections::HashMap;

const OVERLAY: Color32 = Color32::from_rgba_premultiplied(40, 90, 170, 150);
const LEARNING: Color32 = Color32::from_rgb(240, 200, 60);

#[derive(Default)]
pub struct MidiMapState {
    pub open: bool,
    pub learn: Option<MapTarget>,
    last: HashMap<(u8, u8), u8>,
    seen: Option<ControlEvent>,
}

pub fn volume_target(target: MixTarget) -> MapTarget {
    match target {
        MixTarget::Track(id) => MapTarget::TrackVolume(id),
        MixTarget::Return(i) => MapTarget::ReturnVolume(i as u8),
        MixTarget::Master => MapTarget::MasterVolume,
    }
}

pub fn pan_target(target: MixTarget) -> Option<MapTarget> {
    match target {
        MixTarget::Track(id) => Some(MapTarget::TrackPan(id)),
        MixTarget::Return(i) => Some(MapTarget::ReturnPan(i as u8)),
        MixTarget::Master => None,
    }
}

pub fn mute_target(target: MixTarget) -> Option<MapTarget> {
    match target {
        MixTarget::Track(id) => Some(MapTarget::TrackMute(id)),
        MixTarget::Return(i) => Some(MapTarget::ReturnMute(i as u8)),
        MixTarget::Master => None,
    }
}

impl App {
    pub(crate) fn toggle_midi_map(&mut self) {
        self.midi_map.open = !self.midi_map.open;
        self.midi_map.learn = None;
    }

    pub(crate) fn poll_controls(&mut self) {
        let Some(connection) = self.midi.as_mut() else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(event) = connection.controls.pop() {
            events.push(event);
        }
        let mut changed = false;
        for event in events {
            let key = (event.channel, event.controller);
            let previous = self.midi_map.last.insert(key, event.value);
            self.midi_map.seen = Some(event);
            if self.midi_map.open {
                if let Some(target) = self.midi_map.learn.take() {
                    self.edit(|p| midi_map::assign(p, event.channel, event.controller, target));
                    self.message = Some(format!(
                        "{} is now on CC {} (channel {}).",
                        midi_map::describe(&self.project, target),
                        event.controller,
                        event.channel + 1
                    ));
                }
                continue;
            }
            let targets: Vec<MapTarget> = self
                .project
                .midi_map
                .iter()
                .filter(|m| m.channel == event.channel && m.controller == event.controller)
                .map(|m| m.target)
                .collect();
            for target in targets {
                changed |= midi_map::apply(&mut self.project, target, event.value, previous);
            }
        }
        if changed {
            self.sync_plan();
        }
    }

    pub(crate) fn map_overlay(&mut self, ui: &egui::Ui, rect: Rect, target: MapTarget) {
        if !self.midi_map.open {
            return;
        }
        let response = ui
            .interact(rect, ui.scope_id().with(("midi_map", target)), Sense::click())
            .on_hover_text(format!(
                "{}: click, then move a control on your MIDI controller. Right-click removes the mapping.",
                midi_map::describe(&self.project, target)
            ));
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, OVERLAY);
        if self.midi_map.learn == Some(target) {
            painter.rect_stroke(rect, 2.0, Stroke::new(2.0, LEARNING), StrokeKind::Inside);
        }
        if let Some(mapping) = midi_map::find(&self.project, target) {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                mapping.controller.to_string(),
                FontId::proportional(10.0),
                Color32::WHITE,
            );
        }
        if response.clicked() {
            self.midi_map.learn = Some(target);
        }
        if response.secondary_clicked() {
            self.edit(|p| {
                midi_map::unassign(p, target);
            });
        }
    }

    pub(crate) fn device_map_row(
        &mut self,
        ui: &mut egui::Ui,
        slot: fnoob_core::model::PluginSlot,
    ) {
        if !self.midi_map.open {
            return;
        }
        let Some(effect) = slot.builtin else {
            return;
        };
        ui.separator();
        ui.label(RichText::new("MIDI Map").small().strong());
        ui.horizontal_wrapped(|ui| {
            for (index, info) in effect.params().iter().enumerate() {
                let target = MapTarget::Device(slot.id, index as u8);
                let text = match midi_map::find(&self.project, target) {
                    Some(m) => format!("{} · {}", info.name, m.controller),
                    None => info.name.to_owned(),
                };
                let response = ui
                    .selectable_label(self.midi_map.learn == Some(target), text)
                    .on_hover_text(
                        "Click, then move a control on your MIDI controller. Right-click removes the mapping.",
                    );
                if response.clicked() {
                    self.midi_map.learn = Some(target);
                }
                if response.secondary_clicked() {
                    self.edit(|p| {
                        midi_map::unassign(p, target);
                    });
                }
            }
        });
    }

    pub(crate) fn midi_map_window(&mut self, ctx: &egui::Context) {
        if !self.midi_map.open {
            return;
        }
        let mut open = true;
        let mut remove = None;
        egui::Window::new("MIDI Map")
            .open(&mut open)
            .default_width(280.0)
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -48.0])
            .resizable(false)
            .show(ctx, |ui| {
                let status = match self.midi_map.learn {
                    Some(target) => format!(
                        "Move a control on your MIDI controller for {}.",
                        midi_map::describe(&self.project, target)
                    ),
                    None => "Click a blue control, then move a knob, fader or button on your MIDI controller.".to_owned(),
                };
                ui.label(status);
                if self.midi.is_none() {
                    ui.label(
                        RichText::new("No MIDI input is open. Choose one in Audio and MIDI Settings.")
                            .weak(),
                    );
                }
                if let Some(seen) = self.midi_map.seen {
                    ui.label(
                        RichText::new(format!(
                            "Last received: CC {} = {} on channel {}",
                            seen.controller,
                            seen.value,
                            seen.channel + 1
                        ))
                        .small(),
                    );
                }
                ui.separator();
                if self.project.midi_map.is_empty() {
                    ui.label(RichText::new("No mappings yet.").weak());
                }
                egui::Grid::new("midi_map_list").striped(true).show(ui, |ui| {
                    for mapping in &self.project.midi_map {
                        ui.label(format!("Ch {} CC {}", mapping.channel + 1, mapping.controller));
                        ui.label(midi_map::describe(&self.project, mapping.target));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(mapping.target);
                        }
                        ui.end_row();
                    }
                });
            });
        if let Some(target) = remove {
            self.edit(|p| {
                midi_map::unassign(p, target);
            });
        }
        if !open {
            self.toggle_midi_map();
        }
    }
}
