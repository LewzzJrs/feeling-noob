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
                        let height = ui.available_height().clamp(60.0, 220.0);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(240.0, height), egui::Sense::hover());
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Drop Audio Effects Here",
                            egui::FontId::proportional(13.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                })
            })
            .inner
            .response;
        self.chain_rect = Some(response.rect);
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

    fn accent(&self, owner: Owner) -> egui::Color32 {
        match owner {
            Owner::Track(id) => self.project.track(id).map_or(egui::Color32::GRAY, |t| {
                crate::timeline::track_color(t.color)
            }),
            Owner::Return(_) => egui::Color32::from_gray(160),
        }
    }

    fn title_bar(
        ui: &mut Ui,
        name: &str,
        on: Option<bool>,
        accent: egui::Color32,
    ) -> (bool, Option<egui::Response>) {
        let mut toggled = false;
        let mut title = None;
        let fill = if on == Some(false) {
            accent.gamma_multiply(0.35)
        } else {
            accent.gamma_multiply(0.85)
        };
        egui::Frame::new()
            .fill(fill)
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    if let Some(on) = on {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
                        let color = if on {
                            egui::Color32::from_rgb(255, 170, 60)
                        } else {
                            egui::Color32::from_gray(90)
                        };
                        ui.painter().circle_filled(rect.center(), 5.5, color);
                        ui.painter().circle_stroke(
                            rect.center(),
                            5.5,
                            egui::Stroke::new(1.0, egui::Color32::BLACK),
                        );
                        let response = crate::ui_kit::hint(
                            response,
                            "Device on/off: click to switch this device on or off.",
                        );
                        if response.clicked() {
                            toggled = true;
                        }
                    }
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 16.0),
                        egui::Sense::click(),
                    );
                    ui.painter_at(rect).text(
                        rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(13.5),
                        egui::Color32::BLACK,
                    );
                    title = Some(response);
                });
            });
        (toggled, title)
    }

    fn card<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.spacing_mut().slider_width = 110.0;
            ui.set_min_width(width);
            ui.set_max_width(width.max(CARD_W));
            ui.vertical(add).inner
        })
    }

    pub(crate) fn chain_owner(&self) -> Option<Owner> {
        match self.chain_target {
            ChainTarget::Return(index) => {
                (index < self.project.returns.len()).then_some(Owner::Return(index))
            }
            ChainTarget::SelectedTrack => self.selection.track.map(Owner::Track),
        }
    }

    pub(crate) fn device_command(&mut self, command: fnoob_core::keymap::Command) -> bool {
        use fnoob_core::keymap::Command;
        if !self.device_focus
            || !matches!(
                command,
                Command::Cut
                    | Command::Copy
                    | Command::Paste
                    | Command::Duplicate
                    | Command::Delete
            )
        {
            return false;
        }
        let selected = self
            .selected_device
            .filter(|(owner, _)| Some(*owner) == self.chain_owner());
        let Some(owner) = selected.map(|(o, _)| o).or_else(|| self.chain_owner()) else {
            return false;
        };
        self.capture_plugin_states();
        let slot = selected.and_then(|(o, index)| {
            let inserts = match o {
                Owner::Track(id) => self.project.track(id).map(|t| &t.strip.inserts),
                Owner::Return(i) => self.project.returns.get(i).map(|r| &r.inserts),
            };
            inserts.and_then(|i| i.get(index)).cloned()
        });
        let index = selected.map(|(_, i)| i);
        match command {
            Command::Copy => {
                if let Some(slot) = slot {
                    self.system_clipboard = Some(slot.name.clone());
                    self.device_clipboard = Some(slot);
                }
            }
            Command::Cut | Command::Delete => {
                let (Some(slot), Some(index)) = (slot, index) else {
                    return true;
                };
                if command == Command::Cut {
                    self.system_clipboard = Some(slot.name.clone());
                    self.device_clipboard = Some(slot);
                }
                let mut left = 0;
                self.edit(|p| {
                    if let Some(inserts) = inserts_mut(p, owner) {
                        if index < inserts.len() {
                            inserts.remove(index);
                        }
                        left = inserts.len();
                    }
                });
                self.selected_device = (left > 0).then(|| (owner, index.min(left - 1)));
            }
            Command::Duplicate => {
                if let (Some(slot), Some(index)) = (slot, index) {
                    self.insert_device(owner, index + 1, slot);
                }
            }
            _ => {
                let Some(slot) = self.device_clipboard.clone() else {
                    self.message = Some("Copy or cut a device first.".into());
                    return true;
                };
                let at = index.map_or(usize::MAX, |i| i + 1);
                self.insert_device(owner, at, slot);
            }
        }
        true
    }

    fn insert_device(&mut self, owner: Owner, at: usize, slot: PluginSlot) {
        let mut placed = None;
        self.edit(|p| {
            let id = p.alloc_id();
            if let Some(inserts) = inserts_mut(p, owner)
                && inserts.len() < MAX_INSERTS
            {
                let at = at.min(inserts.len());
                inserts.insert(at, PluginSlot { id, ..slot });
                placed = Some(at);
            }
        });
        match placed {
            Some(at) => self.selected_device = Some((owner, at)),
            None => self.message = Some("All insert slots are full.".into()),
        }
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
        let accent = self.accent(Owner::Track(track));
        Self::card(ui, width, |ui| {
            Self::title_bar(ui, instrument.label(), None, accent);
            ui.horizontal(|ui| {
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
        let accent = self.accent(owner);
        let mut chosen = None;
        let card = Self::card(ui, CARD_W, |ui| {
            let (toggled, title) = Self::title_bar(ui, &slot.name, Some(!slot.bypass), accent);
            if toggled {
                self.toggle_bypass(owner, index);
            }
            if let Some(title) = title {
                let title = crate::ui_kit::hint(
                    title,
                    "Device title: click to select it, then Delete, Ctrl+X, Ctrl+C, Ctrl+V or Ctrl+D. Right-click for more.",
                );
                if title.clicked() || title.secondary_clicked() {
                    chosen = Some(None);
                }
                title.context_menu(|ui| {
                    use fnoob_core::keymap::{Command, shortcut_label};
                    for (text, command) in [
                        ("Cut", Command::Cut),
                        ("Copy", Command::Copy),
                        ("Paste", Command::Paste),
                        ("Duplicate", Command::Duplicate),
                        ("Delete", Command::Delete),
                    ] {
                        let mut button = egui::Button::new(text);
                        if let Some(label) = shortcut_label(command) {
                            button = button.shortcut_text(label);
                        }
                        let enabled = command != Command::Paste || self.device_clipboard.is_some();
                        if ui.add_enabled(enabled, button).clicked() {
                            chosen = Some(Some(command));
                            ui.close();
                        }
                    }
                });
            }
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
        if self.selected_device == Some((owner, index)) {
            ui.painter().rect_stroke(
                card.response.rect,
                4.0,
                egui::Stroke::new(1.5, egui::Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(command) = chosen {
            self.selected_device = Some((owner, index));
            self.device_focus = true;
            if let Some(command) = command {
                self.run(command);
            }
        }
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
