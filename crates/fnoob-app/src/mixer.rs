use crate::app::App;
use crate::plugins::{InstrumentChoice, Owner};
use crate::timeline::track_color;
use eframe::egui::{self, Color32, CornerRadius, Rect, RichText, Sense, Vec2};
use fnoob_core::catalog::CatalogEntry;
use fnoob_core::model::{
    Id, Instrument, MAX_FADER_DB, MAX_INSERTS, PluginSlot, Project, SILENT_DB, TrackKind,
};
use fnoob_engine::plan::{BUS_METER, MASTER_METER, METER_SLOTS, TRACK_METERS};

const STRIP_WIDTH: f32 = 96.0;
const FADER_FLOOR_DB: f32 = -60.0;
const METER_DECAY: f32 = 0.88;
const NAME_CHARS: usize = 11;

pub struct Meters {
    levels: Vec<f32>,
}

impl Default for Meters {
    fn default() -> Self {
        Self {
            levels: vec![0.0; METER_SLOTS],
        }
    }
}

impl Meters {
    pub fn update(&mut self, take: impl Fn(usize) -> f32) {
        for (slot, level) in self.levels.iter_mut().enumerate() {
            *level = take(slot).max(*level * METER_DECAY);
            if *level < 1e-5 {
                *level = 0.0;
            }
        }
    }

    fn level(&self, slot: usize) -> f32 {
        self.levels.get(slot).copied().unwrap_or(0.0)
    }
}

fn level_db(level: f32) -> f32 {
    if level <= 0.0 {
        SILENT_DB
    } else {
        20.0 * level.log10()
    }
}

fn db_label(db: f32) -> String {
    if db <= FADER_FLOOR_DB {
        "-inf".to_owned()
    } else {
        format!("{db:+.1}")
    }
}

fn meter_fraction(db: f32) -> f32 {
    ((db - FADER_FLOOR_DB) / (MAX_FADER_DB - FADER_FLOOR_DB)).clamp(0.0, 1.0)
}

fn draw_meter(ui: &mut egui::Ui, level: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(2), Color32::from_gray(30));
    let db = level_db(level);
    let top = rect.bottom() - rect.height() * meter_fraction(db);
    let color = if db >= 0.0 {
        Color32::from_rgb(230, 70, 60)
    } else if db >= -6.0 {
        Color32::from_rgb(230, 200, 70)
    } else {
        Color32::from_rgb(90, 200, 110)
    };
    painter.rect_filled(
        Rect::from_min_max(egui::pos2(rect.left(), top), rect.max),
        CornerRadius::same(2),
        color,
    );
    let zero = rect.bottom() - rect.height() * meter_fraction(0.0);
    painter.hline(
        rect.x_range(),
        zero,
        egui::Stroke::new(1.0, Color32::from_gray(120)),
    );
}

fn fader_value(db: f32) -> f32 {
    db.max(FADER_FLOOR_DB)
}

fn from_fader(value: f32) -> f32 {
    if value <= FADER_FLOOR_DB {
        SILENT_DB
    } else {
        value
    }
}

enum Target {
    Track(Id),
    Bus,
    Master,
}

impl App {
    pub(crate) fn toggle_mixer(&mut self) {
        self.mixer_open = !self.mixer_open;
        if self.mixer_open {
            self.editor.open = false;
            self.devices_open = false;
        }
    }

    pub(crate) fn update_meters(&mut self) {
        if let Some((_, engine)) = &self.audio {
            self.meters.update(|slot| engine.take_peak(slot));
        }
    }

    pub(crate) fn mixer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mixer").strong());
            if ui.button("Close").clicked() {
                self.mixer_open = false;
            }
        });
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let tracks: Vec<(usize, Id)> = self
                    .project
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (i, t.id))
                    .collect();
                for (index, id) in tracks {
                    let slot = if index < TRACK_METERS {
                        index
                    } else {
                        METER_SLOTS
                    };
                    self.strip(ui, Target::Track(id), slot);
                    ui.separator();
                }
                self.strip(ui, Target::Bus, BUS_METER);
                ui.separator();
                self.strip(ui, Target::Master, MASTER_METER);
            });
        });
    }

    fn strip(&mut self, ui: &mut egui::Ui, target: Target, slot: usize) {
        let (name, color, volume, pan, mute, solo, send) = match &target {
            Target::Track(id) => {
                let Some(track) = self.project.track(*id) else {
                    return;
                };
                (
                    track.name.clone(),
                    track_color(track.color),
                    track.strip.volume_db,
                    Some(track.strip.pan),
                    Some(track.mute),
                    Some(track.solo),
                    Some(track.strip.send),
                )
            }
            Target::Bus => (
                self.project.bus.name.clone(),
                Color32::from_gray(160),
                self.project.bus.volume_db,
                Some(self.project.bus.pan),
                Some(self.project.bus.mute),
                None,
                None,
            ),
            Target::Master => (
                "Master".to_owned(),
                Color32::from_gray(220),
                self.project.master.volume_db,
                None,
                None,
                None,
                None,
            ),
        };
        ui.allocate_ui(Vec2::new(STRIP_WIDTH, ui.available_height()), |ui| {
            ui.set_width(STRIP_WIDTH);
            ui.vertical(|ui| {
                let (bar, _) = ui.allocate_exact_size(Vec2::new(STRIP_WIDTH, 4.0), Sense::hover());
                ui.painter().rect_filled(bar, 0.0, color);
                ui.label(RichText::new(name).strong());
                if let Target::Track(id) = target {
                    self.instrument_button(ui, id);
                }
                let owner = match target {
                    Target::Track(id) => Some(Owner::Track(id)),
                    Target::Bus => Some(Owner::Bus),
                    Target::Master => None,
                };
                if let Some(owner) = owner {
                    let slots: Vec<PluginSlot> = match owner {
                        Owner::Track(id) => self
                            .project
                            .track(id)
                            .map(|t| t.strip.inserts.clone())
                            .unwrap_or_default(),
                        Owner::Bus => self.project.bus.inserts.clone(),
                    };
                    for index in 0..MAX_INSERTS {
                        self.insert_button(ui, owner, index, slots.get(index));
                    }
                }
                if let Some(send) = send {
                    ui.label("Send to Bus 1");
                    let mut level = fader_value(send.level_db);
                    let response = ui.add(
                        egui::Slider::new(&mut level, FADER_FLOOR_DB..=MAX_FADER_DB)
                            .show_value(false),
                    );
                    let response = response.on_hover_text(db_label(level));
                    self.live_edit(&response, |p| {
                        if let Some(t) = track_of(p, &target) {
                            t.strip.send.level_db = from_fader(level);
                        }
                    });
                    let mut pre = send.pre_fader;
                    if ui.checkbox(&mut pre, "Pre fader").changed() {
                        self.edit(|p| {
                            if let Some(t) = track_of(p, &target) {
                                t.strip.send.pre_fader = pre;
                            }
                        });
                    }
                }
                if let Some(pan) = pan {
                    let mut value = pan;
                    let response = ui
                        .add(egui::Slider::new(&mut value, -1.0..=1.0).show_value(false))
                        .on_hover_text(format!("Pan {value:+.2}. Double-click to center."));
                    let value = if response.double_clicked() {
                        0.0
                    } else {
                        value
                    };
                    if response.double_clicked() {
                        self.edit(|p| set_pan(p, &target, 0.0));
                    } else {
                        self.live_edit(&response, |p| set_pan(p, &target, value));
                    }
                }
                let height = (ui.available_height() - 60.0).max(80.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = height;
                    let mut value = fader_value(volume);
                    let response = ui.add(
                        egui::Slider::new(&mut value, FADER_FLOOR_DB..=MAX_FADER_DB)
                            .vertical()
                            .show_value(false),
                    );
                    if response.double_clicked() {
                        self.edit(|p| set_volume(p, &target, 0.0));
                    } else {
                        self.live_edit(&response, |p| set_volume(p, &target, from_fader(value)));
                    }
                    draw_meter(ui, self.meters.level(slot), height);
                });
                ui.label(RichText::new(db_label(fader_value(volume))).monospace());
                ui.horizontal(|ui| {
                    if let Some(muted) = mute
                        && ui
                            .selectable_label(muted, "M")
                            .on_hover_text("Mute")
                            .clicked()
                    {
                        self.edit(|p| match &target {
                            Target::Track(id) => {
                                if let Some(t) = p.track_mut(*id) {
                                    t.mute = !t.mute;
                                }
                            }
                            Target::Bus => p.bus.mute = !p.bus.mute,
                            Target::Master => {}
                        });
                    }
                    if let (Some(soloed), Target::Track(id)) = (solo, &target)
                        && ui
                            .selectable_label(soloed, "S")
                            .on_hover_text("Solo")
                            .clicked()
                    {
                        let id = *id;
                        self.edit_track(id, |t| t.solo = !t.solo);
                    }
                });
            });
        });
    }
}

fn short(name: &str) -> String {
    if name.chars().count() > NAME_CHARS {
        let cut: String = name.chars().take(NAME_CHARS - 1).collect();
        format!("{cut}…")
    } else {
        name.to_owned()
    }
}

impl App {
    pub(crate) fn plugin_choices(
        &self,
        ui: &mut egui::Ui,
        instruments: bool,
    ) -> Option<CatalogEntry> {
        let mut chosen = None;
        let entries: Vec<&CatalogEntry> = self
            .plugins
            .catalog
            .plugins
            .iter()
            .filter(|e| e.class.instrument == instruments)
            .collect();
        if entries.is_empty() {
            ui.label("No plugins found. Open File > Plugin Manager.");
            return None;
        }
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                for entry in entries {
                    let text = if entry.class.vendor.is_empty() {
                        entry.class.name.clone()
                    } else {
                        format!("{}  ({})", entry.class.name, entry.class.vendor)
                    };
                    if ui.button(text).clicked() {
                        chosen = Some(entry.clone());
                    }
                }
            });
        chosen
    }

    fn insert_button(
        &mut self,
        ui: &mut egui::Ui,
        owner: Owner,
        index: usize,
        slot: Option<&PluginSlot>,
    ) {
        let label = match slot {
            Some(slot) => {
                let mut text = short(&slot.name);
                if slot.bypass {
                    text = format!("({text})");
                }
                if !self.is_plugin_loaded(slot.id) {
                    text = format!("! {text}");
                }
                text
            }
            None => "Insert".to_owned(),
        };
        let response = ui.menu_button(label, |ui| {
            if let Some(slot) = slot {
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
                    ui.close();
                }
                if ui
                    .button(if slot.bypass { "Turn on" } else { "Bypass" })
                    .clicked()
                {
                    self.toggle_bypass(owner, index);
                    ui.close();
                }
                if ui.button("Remove").clicked() {
                    self.set_insert(owner, index, None);
                    ui.close();
                }
                ui.separator();
                ui.label("Replace with");
            }
            ui.label(RichText::new("Built-in").weak());
            for effect in fnoob_core::effects::BuiltinEffect::all() {
                if ui.button(effect.label()).clicked() {
                    self.set_builtin_insert(owner, index, effect);
                    ui.close();
                }
            }
            ui.separator();
            ui.label(RichText::new("Plugins").weak());
            if let Some(entry) = self.plugin_choices(ui, false) {
                self.set_insert(owner, index, Some(&entry));
                ui.close();
            }
        });
        if let Some(slot) = slot {
            response.response.on_hover_text(&slot.name);
        }
    }

    fn instrument_button(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(track) = self.project.track(id) else {
            return;
        };
        if track.kind != TrackKind::Instrument {
            return;
        }
        let slot = match &track.instrument {
            Some(Instrument::Vst3(slot)) => Some(slot.clone()),
            _ => None,
        };
        let label = short(
            track
                .instrument
                .as_ref()
                .map_or("Basic Synth", Instrument::label),
        );
        ui.menu_button(RichText::new(label).strong(), |ui| {
            match &slot {
                Some(slot) => {
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
                        ui.close();
                    }
                }
                None => {
                    if ui.button("Instrument settings...").clicked() {
                        self.open_instrument(id);
                        ui.close();
                    }
                }
            }
            ui.separator();
            ui.label(RichText::new("Built-in").weak());
            for choice in InstrumentChoice::BUILTIN {
                if ui.button(choice.label()).clicked() {
                    self.set_instrument(id, &choice);
                    ui.close();
                }
            }
            ui.separator();
            ui.label("Instrument plugins");
            if let Some(entry) = self.plugin_choices(ui, true) {
                self.set_instrument(id, &InstrumentChoice::Vst3(entry));
                ui.close();
            }
        });
    }
}

fn track_of<'a>(
    project: &'a mut Project,
    target: &Target,
) -> Option<&'a mut fnoob_core::model::Track> {
    match target {
        Target::Track(id) => project.track_mut(*id),
        Target::Bus | Target::Master => None,
    }
}

fn set_volume(project: &mut Project, target: &Target, db: f32) {
    match target {
        Target::Track(id) => {
            if let Some(t) = project.track_mut(*id) {
                t.strip.volume_db = db;
            }
        }
        Target::Bus => project.bus.volume_db = db,
        Target::Master => project.master.volume_db = db,
    }
}

fn set_pan(project: &mut Project, target: &Target, pan: f32) {
    match target {
        Target::Track(id) => {
            if let Some(t) = project.track_mut(*id) {
                t.strip.pan = pan;
            }
        }
        Target::Bus => project.bus.pan = pan,
        Target::Master => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_floor_means_silence() {
        assert_eq!(from_fader(FADER_FLOOR_DB), SILENT_DB);
        assert_eq!(from_fader(-12.0), -12.0);
        assert_eq!(fader_value(SILENT_DB), FADER_FLOOR_DB);
        assert_eq!(db_label(SILENT_DB), "-inf");
        assert_eq!(db_label(-3.04), "-3.0");
    }

    #[test]
    fn meters_hold_peaks_and_decay() {
        let mut meters = Meters::default();
        meters.update(|slot| if slot == 3 { 0.5 } else { 0.0 });
        assert_eq!(meters.level(3), 0.5);
        meters.update(|_| 0.0);
        assert!((meters.level(3) - 0.5 * METER_DECAY).abs() < 1e-6);
        assert_eq!(meters.level(METER_SLOTS + 5), 0.0);
        assert!((meter_fraction(0.0) - 60.0 / 66.0).abs() < 1e-6);
    }
}
