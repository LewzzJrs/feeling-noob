use crate::app::App;
use crate::ui_kit::{hint, knob};
use eframe::egui::{self, Color32, CornerRadius, Painter, Rect, RichText, Ui};
use fnoob_core::catalog::CatalogEntry;
use fnoob_core::model::{
    Id, MAX_FADER_DB, Monitor, Output, Project, SILENT_DB, TrackKind, return_letter,
};
use fnoob_engine::plan::{MASTER_METER, METER_SLOTS, TRACK_METERS, return_meter};

const FADER_FLOOR_DB: f32 = -60.0;
const METER_DECAY: f32 = 0.88;
const ARM: Color32 = Color32::from_rgb(230, 70, 70);
const ACTIVE: Color32 = Color32::from_rgb(235, 170, 60);
const SOLO: Color32 = Color32::from_rgb(70, 150, 230);

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

fn meter_fraction(db: f32) -> f32 {
    ((db - FADER_FLOOR_DB) / (MAX_FADER_DB - FADER_FLOOR_DB)).clamp(0.0, 1.0)
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

fn db_text(db: f64) -> String {
    if db <= f64::from(FADER_FLOOR_DB) {
        "-inf".to_owned()
    } else {
        format!("{db:.1}")
    }
}

fn pan_text(value: f64) -> String {
    let amount = value.round() as i64;
    match amount {
        0 => "C".to_owned(),
        a if a < 0 => format!("{}L", -a),
        a => format!("{a}R"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixTarget {
    Track(Id),
    Return(usize),
    Master,
}

impl MixTarget {
    pub fn meter(self, project: &Project) -> usize {
        match self {
            Self::Track(id) => project
                .tracks
                .iter()
                .position(|t| t.id == id)
                .filter(|&i| i < TRACK_METERS)
                .unwrap_or(METER_SLOTS),
            Self::Return(index) => return_meter(index),
            Self::Master => MASTER_METER,
        }
    }
}

struct MixState {
    number: usize,
    volume: f32,
    pan: Option<f32>,
    active: bool,
    solo: Option<bool>,
    arm: Option<bool>,
}

fn state(project: &Project, target: MixTarget) -> Option<MixState> {
    Some(match target {
        MixTarget::Track(id) => {
            let index = project.tracks.iter().position(|t| t.id == id)?;
            let track = &project.tracks[index];
            MixState {
                number: index + 1,
                volume: track.strip.volume_db,
                pan: Some(track.strip.pan),
                active: !track.mute,
                solo: Some(track.solo),
                arm: Some(track.record_enabled),
            }
        }
        MixTarget::Return(index) => {
            let ret = project.returns.get(index)?;
            MixState {
                number: index,
                volume: ret.volume_db,
                pan: Some(ret.pan),
                active: !ret.mute,
                solo: None,
                arm: None,
            }
        }
        MixTarget::Master => MixState {
            number: 0,
            volume: project.master.volume_db,
            pan: None,
            active: true,
            solo: None,
            arm: None,
        },
    })
}

fn set_volume(project: &mut Project, target: MixTarget, db: f32) {
    match target {
        MixTarget::Track(id) => {
            if let Some(t) = project.track_mut(id) {
                t.strip.volume_db = db;
            }
        }
        MixTarget::Return(index) => {
            if let Some(r) = project.returns.get_mut(index) {
                r.volume_db = db;
            }
        }
        MixTarget::Master => project.master.volume_db = db,
    }
}

fn set_pan(project: &mut Project, target: MixTarget, pan: f32) {
    match target {
        MixTarget::Track(id) => {
            if let Some(t) = project.track_mut(id) {
                t.strip.pan = pan;
            }
        }
        MixTarget::Return(index) => {
            if let Some(r) = project.returns.get_mut(index) {
                r.pan = pan;
            }
        }
        MixTarget::Master => {}
    }
}

fn set_send(project: &mut Project, track: Id, index: usize, db: f32) {
    if let Some(t) = project.track_mut(track) {
        t.strip.send_mut(index).level_db = db;
    }
}

pub(crate) fn toggle(
    ui: &mut Ui,
    on: bool,
    text: &str,
    color: Color32,
    width: f32,
) -> egui::Response {
    let text = if on {
        RichText::new(text).color(Color32::BLACK).strong()
    } else {
        RichText::new(text)
    };
    let button = egui::Button::new(text)
        .fill(if on {
            color
        } else {
            ui.visuals().widgets.inactive.bg_fill
        })
        .min_size(egui::vec2(width, 16.0));
    ui.add(button)
}

impl App {
    pub(crate) fn toggle_mixer(&mut self) {
        self.mixer_open = !self.mixer_open;
    }

    pub(crate) fn update_meters(&mut self) {
        if let Some((_, engine)) = &self.audio {
            self.meters.update(|slot| engine.take_peak(slot));
        }
    }

    pub(crate) fn paint_meter(&self, painter: &Painter, rect: Rect, target: MixTarget) {
        painter.rect_filled(rect, CornerRadius::same(1), Color32::from_gray(28));
        let db = level_db(self.meters.level(target.meter(&self.project)));
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
            CornerRadius::same(1),
            color,
        );
    }

    pub(crate) fn mix_buttons(&mut self, ui: &mut Ui, target: MixTarget) {
        let Some(s) = state(&self.project, target) else {
            return;
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let label = match target {
                MixTarget::Track(_) => s.number.to_string(),
                MixTarget::Return(index) => return_letter(index).to_string(),
                MixTarget::Master => "M".to_owned(),
            };
            if target != MixTarget::Master
                && hint(
                    toggle(ui, s.active, &label, ACTIVE, 26.0),
                    "Track activator: turn the track on or off (mute).",
                )
                .clicked()
            {
                self.edit(|p| match target {
                    MixTarget::Track(id) => {
                        if let Some(t) = p.track_mut(id) {
                            t.mute = !t.mute;
                        }
                    }
                    MixTarget::Return(index) => {
                        if let Some(r) = p.returns.get_mut(index) {
                            r.mute = !r.mute;
                        }
                    }
                    MixTarget::Master => {}
                });
            }
            if let (Some(solo), MixTarget::Track(id)) = (s.solo, target)
                && hint(
                    toggle(ui, solo, "S", SOLO, 18.0),
                    "Solo: hear only soloed tracks.",
                )
                .clicked()
            {
                self.edit_track(id, |t| t.solo = !t.solo);
            }
            if let (Some(arm), MixTarget::Track(id)) = (s.arm, target)
                && hint(
                    toggle(ui, arm, "●", ARM, 18.0),
                    "Arm: record on this track. Recording also needs Record (F9).",
                )
                .clicked()
            {
                self.edit_track(id, |t| t.record_enabled = !t.record_enabled);
            }
        });
    }

    pub(crate) fn mix_levels(&mut self, ui: &mut Ui, target: MixTarget) {
        let Some(s) = state(&self.project, target) else {
            return;
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut volume = fader_value(s.volume);
            let response = ui.add(
                egui::DragValue::new(&mut volume)
                    .range(FADER_FLOOR_DB..=MAX_FADER_DB)
                    .speed(0.2)
                    .custom_formatter(|v, _| db_text(v))
                    .suffix(" dB"),
            );
            let response = hint(
                response,
                "Volume in dB. Drag up or down; double-click to type a value.",
            );
            self.live_edit(&response, |p| set_volume(p, target, from_fader(volume)));
            if let Some(pan) = s.pan {
                let mut value = pan * 50.0;
                let response = ui.add(
                    egui::DragValue::new(&mut value)
                        .range(-50.0..=50.0)
                        .speed(0.5)
                        .custom_formatter(|v, _| pan_text(v)),
                );
                let response = hint(response, "Pan: drag left or right. C is the center.");
                self.live_edit(&response, |p| set_pan(p, target, value / 50.0));
            }
        });
    }

    pub(crate) fn mix_send(&mut self, ui: &mut Ui, target: MixTarget) {
        let MixTarget::Track(id) = target else {
            return;
        };
        let Some(track) = self.project.track(id) else {
            return;
        };
        let levels: Vec<f32> = (0..self.project.returns.len())
            .map(|i| track.strip.send_at(i).level_db)
            .collect();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (index, db) in levels.into_iter().enumerate() {
                ui.label(RichText::new(return_letter(index).to_string()).small());
                let mut level = fader_value(db);
                let response = ui.add(
                    egui::DragValue::new(&mut level)
                        .range(FADER_FLOOR_DB..=MAX_FADER_DB)
                        .speed(0.2)
                        .custom_formatter(|v, _| db_text(v)),
                );
                let text = format!("Send to return track {}, in dB.", return_letter(index));
                let response = hint(response, &text);
                self.live_edit(&response, |p| set_send(p, id, index, from_fader(level)));
            }
        });
    }

    pub(crate) fn mix_strip(&mut self, ui: &mut Ui, target: MixTarget, fader_height: f32) {
        let Some(s) = state(&self.project, target) else {
            return;
        };
        if let MixTarget::Track(id) = target {
            let sends: Vec<f32> = (0..self.project.returns.len())
                .map(|i| {
                    self.project
                        .track(id)
                        .map_or(SILENT_DB, |t| t.strip.send_at(i).level_db)
                })
                .collect();
            if !sends.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for (index, db) in sends.into_iter().enumerate() {
                        let mut level = fader_value(db);
                        let letter = return_letter(index).to_string();
                        let response = knob(ui, &mut level, FADER_FLOOR_DB, MAX_FADER_DB, &letter);
                        let text = format!(
                            "Send {letter}: {} dB to return track {letter}. Drag up or down.",
                            db_text(f64::from(level))
                        );
                        let response = hint(response, &text);
                        self.live_edit(&response, |p| set_send(p, id, index, from_fader(level)));
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            let mut volume = fader_value(s.volume);
            ui.spacing_mut().slider_width = fader_height;
            let response = ui.add(
                egui::Slider::new(&mut volume, FADER_FLOOR_DB..=MAX_FADER_DB)
                    .vertical()
                    .show_value(false),
            );
            let text = format!(
                "Volume: {} dB. Double-click for 0 dB.",
                db_text(f64::from(volume))
            );
            let response = hint(response, &text);
            if response.double_clicked() {
                self.edit(|p| set_volume(p, target, 0.0));
            } else {
                self.live_edit(&response, |p| set_volume(p, target, from_fader(volume)));
            }
            let (meter, _) =
                ui.allocate_exact_size(egui::vec2(6.0, fader_height), egui::Sense::hover());
            self.paint_meter(ui.painter(), meter, target);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(db_text(f64::from(volume)))
                        .small()
                        .monospace(),
                );
                if let Some(pan) = s.pan {
                    let mut value = pan * 50.0;
                    let response = knob(ui, &mut value, -50.0, 50.0, "Pan");
                    let text = format!("Pan {}. Drag up or down.", pan_text(f64::from(value)));
                    let response = hint(response, &text);
                    self.live_edit(&response, |p| set_pan(p, target, value / 50.0));
                }
            });
        });
        self.mix_buttons(ui, target);
    }

    pub(crate) fn track_io(&mut self, ui: &mut Ui, id: Id) {
        let Some(track) = self.project.track(id) else {
            return;
        };
        let (kind, monitor, output) = (track.kind, track.monitor, track.output);
        match kind {
            TrackKind::Audio => {
                let current = track.audio_input.unwrap_or_default();
                egui::ComboBox::from_id_salt(("io_input", id))
                    .width(70.0)
                    .selected_text(crate::timeline::input_label(current))
                    .show_ui(ui, |ui| {
                        for option in crate::timeline::input_options(self.input_channels()) {
                            if ui
                                .selectable_label(
                                    option == current,
                                    crate::timeline::input_label(option),
                                )
                                .clicked()
                            {
                                self.edit_track(id, |t| t.audio_input = Some(option));
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    for mode in Monitor::ALL {
                        if hint(
                            toggle(ui, monitor == mode, mode.label(), ACTIVE, 22.0),
                            "Monitor: In always plays the input, Auto plays it while the track is armed, Off never does.",
                        )
                        .clicked()
                        {
                            self.edit_track(id, |t| t.monitor = mode);
                        }
                    }
                });
            }
            TrackKind::Instrument => {
                let name = track
                    .instrument
                    .as_ref()
                    .map_or("Basic Synth", fnoob_core::model::Instrument::label)
                    .to_owned();
                if hint(
                    ui.add(egui::Button::new(RichText::new(name).small()).truncate()),
                    "Instrument of this track. Click to show its devices.",
                )
                .clicked()
                {
                    self.selection.track = Some(id);
                    self.show_devices();
                }
            }
        }
        let mut chosen = output;
        let combo = egui::ComboBox::from_id_salt(("io_output", id))
            .width(70.0)
            .selected_text(output.label())
            .show_ui(ui, |ui| {
                for option in [Output::Main, Output::None] {
                    ui.selectable_value(&mut chosen, option, option.label());
                }
            });
        hint(combo.response, "Output: where this track is sent.");
        if chosen != output {
            self.edit_track(id, |t| t.output = chosen);
        }
    }

    pub(crate) fn plugin_choices(&self, ui: &mut Ui, instruments: bool) -> Option<CatalogEntry> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_floor_means_silence() {
        assert_eq!(from_fader(FADER_FLOOR_DB), SILENT_DB);
        assert_eq!(from_fader(-12.0), -12.0);
        assert_eq!(fader_value(SILENT_DB), FADER_FLOOR_DB);
        assert_eq!(db_text(-60.0), "-inf");
        assert_eq!(db_text(-3.04), "-3.0");
        assert_eq!(pan_text(0.2), "C");
        assert_eq!(pan_text(-12.0), "12L");
        assert_eq!(pan_text(50.0), "50R");
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
