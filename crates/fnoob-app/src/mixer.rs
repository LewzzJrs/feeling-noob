use crate::app::App;
use crate::ui_kit::hint;
use eframe::egui::{self, Color32, CornerRadius, Painter, Rect, RichText, Ui};
use fnoob_core::catalog::CatalogEntry;
use fnoob_core::model::{Id, MAX_FADER_DB, Project, SILENT_DB, TrackKind};
use fnoob_engine::plan::{BUS_METER, MASTER_METER, METER_SLOTS, TRACK_METERS};

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
    Bus,
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
            Self::Bus => BUS_METER,
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
    send: Option<f32>,
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
                send: Some(track.strip.send.level_db),
            }
        }
        MixTarget::Bus => MixState {
            number: 0,
            volume: project.bus.volume_db,
            pan: Some(project.bus.pan),
            active: !project.bus.mute,
            solo: None,
            arm: None,
            send: None,
        },
        MixTarget::Master => MixState {
            number: 0,
            volume: project.master.volume_db,
            pan: None,
            active: true,
            solo: None,
            arm: None,
            send: None,
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
        MixTarget::Bus => project.bus.volume_db = db,
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
        MixTarget::Bus => project.bus.pan = pan,
        MixTarget::Master => {}
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
                MixTarget::Bus => "A".to_owned(),
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
                    MixTarget::Bus => p.bus.mute = !p.bus.mute,
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
        let (Some(s), MixTarget::Track(id)) = (state(&self.project, target), target) else {
            return;
        };
        let Some(send) = s.send else {
            return;
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.label(RichText::new("A").small());
            let mut level = fader_value(send);
            let response = ui.add(
                egui::DragValue::new(&mut level)
                    .range(FADER_FLOOR_DB..=MAX_FADER_DB)
                    .speed(0.2)
                    .custom_formatter(|v, _| db_text(v)),
            );
            let response = hint(response, "Send to Bus 1 (return A), in dB.");
            self.live_edit(&response, |p| {
                if let Some(t) = p.track_mut(id) {
                    t.strip.send.level_db = from_fader(level);
                }
            });
        });
    }

    pub(crate) fn track_io(&mut self, ui: &mut Ui, id: Id) {
        let Some(track) = self.project.track(id) else {
            return;
        };
        match track.kind {
            TrackKind::Audio => {
                let monitor = track.input_monitoring;
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
                if hint(
                    toggle(ui, monitor, "In", ACTIVE, 26.0),
                    "Monitor: hear the input through this track.",
                )
                .clicked()
                {
                    self.edit_track(id, |t| t.input_monitoring = !t.input_monitoring);
                }
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
