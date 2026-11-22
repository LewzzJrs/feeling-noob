use crate::app::App;
use crate::browser::BrowserItem;
use crate::devices::{effect_controls, slot_mut};
use crate::plugins::{InstrumentChoice, Owner};
use eframe::egui::{self, RichText, Ui};
use fnoob_core::effects::BuiltinEffect;
use fnoob_core::model::{Instrument, MAX_INSERTS, PluginSlot, Project, TrackKind};

const CARD_W: f32 = 250.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChainTarget {
    #[default]
    SelectedTrack,
    Return(usize),
}

fn inserts_mut(project: &mut Project, owner: Owner) -> Option<&mut Vec<PluginSlot>> {
    match owner {
        Owner::Track(id) => project.track_mut(id).map(|t| &mut t.strip.inserts),
        Owner::Return(index) => project.returns.get_mut(index).map(|r| &mut r.inserts),
    }
}

impl App {
    pub(crate) fn reveal_device(&mut self, slot: fnoob_core::model::Id) {
        if let Some(index) = self
            .project
            .returns
            .iter()
            .position(|r| r.inserts.iter().any(|s| s.id == slot))
        {
            self.chain_target = ChainTarget::Return(index);
        } else if let Some(track) = self
            .project
            .tracks
            .iter()
            .find(|t| t.plugin_slots().any(|s| s.id == slot))
        {
            self.selection.track = Some(track.id);
            self.chain_target = ChainTarget::SelectedTrack;
        }
        self.show_devices();
    }

    fn move_insert(&mut self, owner: Owner, from: usize, to: usize) {
        self.edit(|p| {
            if let Some(inserts) = inserts_mut(p, owner) {
                fnoob_core::edit::move_insert(inserts, from, to);
            }
        });
    }

    pub(crate) fn device_chain(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Devices").strong());
            ui.separator();
            ui.selectable_value(
                &mut self.chain_target,
                ChainTarget::SelectedTrack,
                "Selected track",
            );
            let names: Vec<String> = self
                .project
                .returns
                .iter()
                .map(|r| r.name.clone())
                .collect();
            for (index, name) in names.into_iter().enumerate() {
                ui.selectable_value(&mut self.chain_target, ChainTarget::Return(index), name);
            }
            if ui.button("Close").clicked() {
                self.devices_open = false;
            }
        });
        let owner = match self.chain_target {
            ChainTarget::Return(index) => {
                (index < self.project.returns.len()).then_some(Owner::Return(index))
            }
            ChainTarget::SelectedTrack => self.selection.track.map(Owner::Track),
        };
        let Some(owner) = owner else {
            ui.label(RichText::new("Select a track to see its devices.").weak());
            return;
        };
        let (title, kind, inserts) = match owner {
            Owner::Track(id) => match self.project.track(id) {
                Some(t) => (t.name.clone(), Some(t.kind), t.strip.inserts.clone()),
                None => return,
            },
            Owner::Return(index) => match self.project.returns.get(index) {
                Some(r) => (r.name.clone(), None, r.inserts.clone()),
                None => return,
            },
        };
        ui.label(
            RichText::new(format!(
                "{title}: signal flows left to right. Drag effects or instruments here from the browser."
            ))
            .small()
            .weak(),
        );
        let response = egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    if let (Owner::Track(id), Some(TrackKind::Instrument)) = (owner, kind) {
                        self.instrument_card(ui, id);
                    }
                    let count = inserts.len();
                    for (index, slot) in inserts.iter().enumerate() {
                        self.effect_card(ui, owner, index, count, slot);
                    }
                    if count < MAX_INSERTS {
                        self.add_card(ui, owner, count);
                    }
                })
            })
            .inner
            .response;
        let area = ui.interact(
            response.rect,
            ui.scope_id().with("chain_drop"),
            egui::Sense::hover(),
        );
        if area.dnd_hover_payload::<BrowserItem>().is_some() {
            ui.painter().rect_stroke(
                response.rect,
                2.0,
                egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(item) = area.dnd_release_payload::<BrowserItem>() {
            self.drop_on_chain(&item, owner, inserts.len());
        }
    }

    fn drop_on_chain(&mut self, item: &BrowserItem, owner: Owner, count: usize) {
        if count >= MAX_INSERTS && matches!(item, BrowserItem::Effect(_) | BrowserItem::Builtin(_))
        {
            self.message = Some("All insert slots are full.".into());
            return;
        }
        match (item, owner) {
            (BrowserItem::Builtin(effect), _) => self.set_builtin_insert(owner, count, *effect),
            (BrowserItem::Effect(entry), _) => self.set_insert(owner, count, Some(entry)),
            (BrowserItem::Instrument(choice), Owner::Track(id)) => {
                if self
                    .project
                    .track(id)
                    .is_some_and(|t| t.kind == TrackKind::Instrument)
                {
                    self.set_instrument(id, choice);
                } else {
                    self.message = Some("Instruments go on instrument tracks.".into());
                }
            }
            _ => self.message = Some("Drop effects or instruments here.".into()),
        }
    }

    fn card<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.spacing_mut().slider_width = 110.0;
                ui.set_min_width(width);
                ui.set_max_width(width.max(CARD_W));
                ui.vertical(add).inner
            })
            .inner
    }

    fn instrument_card(&mut self, ui: &mut Ui, track: fnoob_core::model::Id) {
        let Some(instrument) = self
            .project
            .track(track)
            .map(|t| t.instrument.clone().unwrap_or_default())
        else {
            return;
        };
        let width = if matches!(instrument, Instrument::DrumSampler(_)) {
            560.0
        } else {
            CARD_W
        };
        Self::card(ui, width, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(instrument.label()).strong());
                ui.menu_button("Change", |ui| {
                    for choice in InstrumentChoice::BUILTIN {
                        if ui.button(choice.label()).clicked() {
                            self.set_instrument(track, &choice);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if let Some(entry) = self.plugin_choices(ui, true) {
                        self.set_instrument(track, &InstrumentChoice::Vst3(entry));
                        ui.close();
                    }
                });
            });
            ui.separator();
            match instrument {
                Instrument::BasicSynth(params) => self.synth_controls(ui, track, params),
                Instrument::Sampler(params) => self.sampler_controls(ui, track, params),
                Instrument::DrumSampler(params) => self.drum_controls(ui, track, *params),
                Instrument::Vst3(slot) => self.plugin_body(ui, &slot),
            }
        });
    }

    fn plugin_body(&mut self, ui: &mut Ui, slot: &PluginSlot) {
        ui.label(RichText::new("VST3 plugin").small().weak());
        if let Some(problem) = self.plugins.problem(slot.id) {
            ui.colored_label(ui.visuals().warn_fg_color, problem);
        }
        if ui
            .add_enabled(
                self.is_plugin_loaded(slot.id),
                egui::Button::new("Open editor"),
            )
            .clicked()
        {
            self.open_plugin_editor(slot.id);
        }
    }

    fn effect_card(
        &mut self,
        ui: &mut Ui,
        owner: Owner,
        index: usize,
        count: usize,
        slot: &PluginSlot,
    ) {
        Self::card(ui, CARD_W, |ui| {
            ui.horizontal(|ui| {
                let mut on = !slot.bypass;
                if ui
                    .checkbox(&mut on, "")
                    .on_hover_text("Turn the device on or off")
                    .changed()
                {
                    self.toggle_bypass(owner, index);
                }
                let name = RichText::new(&slot.name).strong();
                ui.label(if slot.bypass { name.weak() } else { name });
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(index > 0, egui::Button::new("<").small())
                    .on_hover_text("Move left")
                    .clicked()
                {
                    self.move_insert(owner, index, index - 1);
                }
                if ui
                    .add_enabled(index + 1 < count, egui::Button::new(">").small())
                    .on_hover_text("Move right")
                    .clicked()
                {
                    self.move_insert(owner, index, index + 1);
                }
                ui.menu_button("Replace", |ui| {
                    for effect in BuiltinEffect::all() {
                        if ui.button(effect.label()).clicked() {
                            self.set_builtin_insert(owner, index, effect);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if let Some(entry) = self.plugin_choices(ui, false) {
                        self.set_insert(owner, index, Some(&entry));
                        ui.close();
                    }
                });
                if ui.small_button("Remove").clicked() {
                    self.set_insert(owner, index, None);
                }
            });
            ui.separator();
            match slot.builtin {
                Some(effect) => {
                    let id = slot.id;
                    let mut next = effect;
                    let responses = effect_controls(ui, &mut next);
                    for response in &responses {
                        self.live_edit(response, |p| {
                            if let Some(target) = slot_mut(p, id) {
                                target.builtin = Some(next);
                            }
                        });
                    }
                }
                None => self.plugin_body(ui, slot),
            }
        });
    }

    fn add_card(&mut self, ui: &mut Ui, owner: Owner, index: usize) {
        Self::card(ui, 140.0, |ui| {
            ui.label(RichText::new("Add effect").weak());
            ui.menu_button("+ Built-in", |ui| {
                for effect in BuiltinEffect::all() {
                    if ui.button(effect.label()).clicked() {
                        self.set_builtin_insert(owner, index, effect);
                        ui.close();
                    }
                }
            });
            ui.menu_button("+ Plugin", |ui| {
                if let Some(entry) = self.plugin_choices(ui, false) {
                    self.set_insert(owner, index, Some(&entry));
                    ui.close();
                }
            });
        });
    }
}
