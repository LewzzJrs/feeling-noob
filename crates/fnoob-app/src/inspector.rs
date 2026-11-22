use crate::app::App;
use crate::timeline::track_color;
use eframe::egui::{self, Color32, RichText, Vec2};
use fnoob_core::edit;
use fnoob_core::keymap::Command;
use fnoob_core::model::{
    AutomationParam, Id, Instrument, MAX_FADER_DB, MAX_GAIN_DB, MIN_GAIN_DB, Region, RegionContent,
    SILENT_DB, TRACK_COLORS, Track, TrackKind,
};
use fnoob_core::time::{PPQ, Position};

#[derive(Default)]
pub struct Inspector {
    pub open: bool,
    region_name: Option<(Id, String)>,
    track_name: Option<(Id, String)>,
}

impl Inspector {
    pub fn shown() -> Self {
        Self {
            open: true,
            ..Self::default()
        }
    }
}

fn beats(ticks: u64) -> f64 {
    ticks as f64 / PPQ as f64
}

fn ticks(beats: f64) -> u64 {
    (beats.max(0.0) * PPQ as f64).round() as u64
}

impl App {
    pub(crate) fn inspector_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Inspector").strong());
            if ui.small_button("Close").clicked() {
                self.inspector.open = false;
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            let clip = self
                .session
                .selected
                .filter(|_| self.session.open)
                .and_then(|id| fnoob_core::session::clip_position(&self.project, id))
                .and_then(|(t, s)| fnoob_core::session::clip(&self.project, t, s))
                .cloned();
            if let Some(clip) = clip {
                egui::CollapsingHeader::new("Clip")
                    .default_open(true)
                    .show(ui, |ui| self.clip_section(ui, &clip));
            }
            let region = self
                .selection
                .regions
                .first()
                .and_then(|id| self.project.find_region(*id))
                .map(|(_, r)| r.clone());
            if let Some(region) = region {
                egui::CollapsingHeader::new("Region")
                    .default_open(true)
                    .show(ui, |ui| self.region_section(ui, &region));
            }
            let track = self
                .selection
                .track
                .and_then(|id| self.project.track(id))
                .cloned();
            match track {
                Some(track) => {
                    egui::CollapsingHeader::new("Track")
                        .default_open(true)
                        .show(ui, |ui| self.track_section(ui, &track));
                }
                None => {
                    ui.label(RichText::new("Select a track or region to see its settings.").weak());
                }
            }
            egui::CollapsingHeader::new("Markers")
                .default_open(true)
                .show(ui, |ui| self.marker_section(ui));
        });
    }

    fn region_section(&mut self, ui: &mut egui::Ui, region: &Region) {
        let id = region.id;
        let mut text = match self.inspector.region_name.take() {
            Some((rid, text)) if rid == id => text,
            _ => region.name.clone(),
        };
        let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
        if response.lost_focus() {
            let name = text.clone();
            self.edit(|p| {
                edit::rename_region(p, id, &name);
            });
        }
        self.inspector.region_name = Some((id, text));
        let ts = self.project.time_signature;
        ui.label(format!(
            "Start {}   Length {:.2} beats",
            Position::from_tick(region.start_tick, ts),
            beats(region.length_tick)
        ));
        let mut start = beats(region.start_tick);
        ui.horizontal(|ui| {
            ui.label("Start (beats)");
            let response = ui.add(
                egui::DragValue::new(&mut start)
                    .speed(0.25)
                    .range(0.0..=100_000.0),
            );
            let track = self.project.find_region(id).map(|(t, _)| t.id);
            if let Some(track) = track {
                self.live_edit(&response, |p| {
                    edit::move_region(p, id, ticks(start), track);
                });
            }
        });
        let mut length = beats(region.length_tick);
        ui.horizontal(|ui| {
            ui.label("Length (beats)");
            let response = ui.add(
                egui::DragValue::new(&mut length)
                    .speed(0.25)
                    .range(0.01..=100_000.0),
            );
            self.live_edit(&response, |p| {
                edit::resize_region(p, id, ticks(length).max(1));
            });
        });
        let mut muted = region.muted;
        if ui.checkbox(&mut muted, "Muted").changed() {
            self.edit(|p| edit::toggle_regions_muted(p, &[id]));
        }
        match region.content {
            RegionContent::Audio { gain_db, .. } => {
                let mut gain = gain_db;
                let response = ui.add(
                    egui::Slider::new(&mut gain, MIN_GAIN_DB..=MAX_GAIN_DB)
                        .suffix(" dB")
                        .text("Gain"),
                );
                self.live_edit(&response, |p| {
                    edit::set_region_gain(p, id, gain);
                });
                let max = beats(region.length_tick);
                let (mut fade_in, mut fade_out) =
                    (beats(region.fade_in_tick), beats(region.fade_out_tick));
                let response = ui.add(
                    egui::Slider::new(&mut fade_in, 0.0..=max)
                        .suffix(" beats")
                        .text("Fade in"),
                );
                let out = region.fade_out_tick;
                self.live_edit(&response, |p| {
                    edit::set_region_fades(p, id, ticks(fade_in), out);
                });
                let response = ui.add(
                    egui::Slider::new(&mut fade_out, 0.0..=max)
                        .suffix(" beats")
                        .text("Fade out"),
                );
                let fin = region.fade_in_tick;
                self.live_edit(&response, |p| {
                    edit::set_region_fades(p, id, fin, ticks(fade_out));
                });
            }
            RegionContent::Midi { ref notes } => {
                ui.label(format!("{} notes", notes.len()));
                ui.horizontal(|ui| {
                    if ui.button("Open in Piano Roll").clicked() {
                        self.open_editor(id);
                    }
                    if ui.button("Quantize").clicked() {
                        let grid = fnoob_core::time::grid_ticks(
                            self.project.snap.grid,
                            self.project.time_signature,
                        );
                        self.edit(|p| {
                            edit::quantize_regions(p, &[id], grid);
                        });
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Split at playhead").clicked() {
                let tick = self.playhead_tick();
                self.edit(|p| {
                    edit::split_regions_at(p, &[id], tick);
                });
            }
            if ui.button("Repeat").clicked() {
                self.edit(|p| {
                    edit::repeat_regions(p, &[id]);
                });
            }
            if ui.button("Delete").clicked() {
                self.edit(|p| {
                    edit::delete_regions(p, &[id]);
                });
                self.selection.regions.retain(|r| *r != id);
            }
        });
    }

    fn track_section(&mut self, ui: &mut egui::Ui, track: &Track) {
        let id = track.id;
        let mut text = match self.inspector.track_name.take() {
            Some((tid, text)) if tid == id => text,
            _ => track.name.clone(),
        };
        let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
        if response.lost_focus() {
            let name = text.trim().to_owned();
            if !name.is_empty() {
                self.edit_track(id, |t| t.name = name);
            }
        }
        self.inspector.track_name = Some((id, text));
        ui.horizontal_wrapped(|ui| {
            ui.label("Color");
            for color in 0..TRACK_COLORS {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
                ui.painter().rect_filled(rect, 3.0, track_color(color));
                if color == track.color {
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        egui::Stroke::new(2.0, Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    self.edit_track(id, |t| t.color = color);
                }
            }
        });
        ui.horizontal(|ui| {
            for (label, on, command) in [
                ("Mute", track.mute, Command::ToggleTrackMute),
                ("Solo", track.solo, Command::ToggleTrackSolo),
                ("Record", track.record_enabled, Command::ToggleTrackRecord),
            ] {
                if ui.selectable_label(on, label).clicked() {
                    self.selection.track = Some(id);
                    self.run(command);
                }
            }
        });
        let mut volume = track.strip.volume_db.max(-60.0);
        let response = ui.add(
            egui::Slider::new(&mut volume, -60.0..=MAX_FADER_DB)
                .suffix(" dB")
                .text("Volume"),
        );
        self.live_edit(&response, |p| {
            if let Some(t) = p.track_mut(id) {
                t.strip.volume_db = if volume <= -60.0 { SILENT_DB } else { volume };
            }
        });
        let mut pan = track.strip.pan;
        let response = ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).text("Pan"));
        self.live_edit(&response, |p| {
            if let Some(t) = p.track_mut(id) {
                t.strip.pan = pan;
            }
        });
        match track.kind {
            TrackKind::Instrument => {
                let label = track
                    .instrument
                    .as_ref()
                    .map_or("Basic Synth", Instrument::label)
                    .to_owned();
                ui.horizontal(|ui| {
                    ui.label(format!("Instrument: {label}"));
                    match &track.instrument {
                        Some(Instrument::Vst3(slot)) => {
                            if ui.button("Open editor").clicked() {
                                self.open_plugin_editor(slot.id);
                            }
                        }
                        _ => {
                            if ui.button("Settings").clicked() {
                                self.open_instrument(id);
                            }
                        }
                    }
                });
            }
            TrackKind::Audio => {
                let mut monitor = track.input_monitoring;
                if ui.checkbox(&mut monitor, "Input monitoring").changed() {
                    self.edit_track(id, |t| t.input_monitoring = monitor);
                }
            }
        }
        if !track.strip.inserts.is_empty() {
            ui.label("Inserts");
            for slot in &track.strip.inserts {
                ui.horizontal(|ui| {
                    ui.label(if slot.bypass {
                        format!("({})", slot.name)
                    } else {
                        slot.name.clone()
                    });
                    if ui.small_button("Edit").clicked() {
                        self.open_plugin_editor(slot.id);
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            ui.label("Automation");
            let current = self.automation_param(id);
            for param in AutomationParam::ALL {
                if ui
                    .selectable_label(current == param, param.label())
                    .clicked()
                {
                    self.automation_params.insert(id, param);
                    self.automation_view = true;
                }
            }
            let points = track.automation.lane(current).len();
            if points > 0 && ui.small_button(format!("Clear {points}")).clicked() {
                self.edit(|p| {
                    if let Some(t) = p.track_mut(id) {
                        t.automation.lane_mut(current).clear();
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() {
                self.selection.track = Some(id);
                self.run(Command::DuplicateTrack);
            }
            if ui.button("Up").clicked() {
                self.edit(|p| {
                    edit::move_track(p, id, -1);
                });
            }
            if ui.button("Down").clicked() {
                self.edit(|p| {
                    edit::move_track(p, id, 1);
                });
            }
            if ui.button("Delete").clicked() {
                self.edit(|p| {
                    edit::delete_track(p, id);
                });
                self.selection.track = None;
                self.selection.regions.clear();
            }
        });
    }

    fn marker_section(&mut self, ui: &mut egui::Ui) {
        if ui.button("Add marker at playhead").clicked() {
            self.run(Command::AddMarker);
        }
        let ts = self.project.time_signature;
        let markers = self.project.markers.clone();
        for marker in markers {
            ui.horizontal(|ui| {
                if ui
                    .button(format!(
                        "{}  {}",
                        Position::from_tick(marker.tick, ts),
                        marker.name
                    ))
                    .on_hover_text("Go to marker")
                    .clicked()
                {
                    self.locate(marker.tick);
                }
                if ui.small_button("Rename").clicked() {
                    self.marker_rename = Some((marker.id, marker.name.clone()));
                }
                if ui
                    .small_button("x")
                    .on_hover_text("Delete marker")
                    .clicked()
                {
                    self.edit(|p| {
                        edit::delete_marker(p, marker.id);
                    });
                }
            });
        }
    }
}
