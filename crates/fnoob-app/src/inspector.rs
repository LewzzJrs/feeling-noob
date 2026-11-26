use crate::app::App;
use eframe::egui::{self};
use fnoob_core::edit;
use fnoob_core::model::{Id, MAX_GAIN_DB, MIN_GAIN_DB, Region, RegionContent};
use fnoob_core::time::PPQ;

#[derive(Default)]
pub struct Inspector {
    region_name: Option<(Id, String)>,
    pub focus_name: bool,
}

fn beats(ticks: u64) -> f64 {
    ticks as f64 / PPQ as f64
}

fn ticks(beats: f64) -> u64 {
    (beats.max(0.0) * PPQ as f64).round() as u64
}

impl App {
    pub(crate) fn region_section(&mut self, ui: &mut egui::Ui, region: &Region) {
        let id = region.id;
        let mut text = match self.inspector.region_name.take() {
            Some((rid, text)) if rid == id => text,
            _ => region.name.clone(),
        };
        let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
        if std::mem::take(&mut self.inspector.focus_name) {
            response.request_focus();
        }
        if response.lost_focus() {
            let name = text.clone();
            self.edit(|p| {
                edit::rename_region(p, id, &name);
            });
        }
        self.inspector.region_name = Some((id, text));
        let meter = self.project.meter();
        let track = self.project.find_region(id).map(|(t, _)| t.id);
        let insert = self.insert_tick;
        ui.horizontal(|ui| {
            ui.label(format!(
                "Position {}",
                crate::arrange::position_text(&meter, region.start_tick)
            ));
            if ui
                .small_button("Set")
                .on_hover_text("Move the clip to the insert marker")
                .clicked()
                && let Some(track) = track
            {
                self.edit(|p| {
                    edit::move_region(p, id, insert, track);
                });
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!(
                "Length {}",
                crate::arrange::length_text(region.length_tick, self.project.time_signature)
            ));
            if ui
                .add_enabled(insert > region.start_tick, egui::Button::new("Set").small())
                .on_hover_text("End the clip at the insert marker")
                .clicked()
            {
                self.edit(|p| {
                    edit::resize_region(p, id, insert - region.start_tick);
                });
            }
        });
        let signature = meter.signature_at(region.start_tick);
        ui.label(format!(
            "Signature {}/{}",
            signature.numerator, signature.denominator
        ));
        let is_midi = matches!(region.content, RegionContent::Midi { .. });
        {
            ui.horizontal(|ui| {
                let mut looped = region.loop_ticks.is_some();
                if ui
                    .checkbox(&mut looped, "Loop")
                    .on_hover_text("Repeat the start of the clip over its whole length")
                    .changed()
                {
                    let length = region.length_tick;
                    self.edit(|p| {
                        edit::set_region_loop(p, id, looped.then_some(length));
                    });
                }
                if let Some(repeat) = region.loop_ticks {
                    let mut loop_beats = beats(repeat);
                    let response = ui.add(
                        egui::DragValue::new(&mut loop_beats)
                            .speed(0.25)
                            .range(0.25..=100_000.0)
                            .suffix(" beats"),
                    );
                    self.live_edit(&response, |p| {
                        edit::set_region_loop(p, id, Some(ticks(loop_beats).max(1)));
                    });
                }
                if is_midi
                    && ui
                    .button("Duplicate Loop")
                    .on_hover_text(
                        "Copy the loop (or the whole clip) after itself and make it twice as long",
                    )
                    .clicked()
                {
                    self.edit(|p| {
                        edit::duplicate_loop(p, id);
                    });
                }
            });
        }
        if !is_midi {
            ui.horizontal(|ui| {
                let mut warped = region.warp.is_some();
                if ui
                    .checkbox(&mut warped, "Warp")
                    .on_hover_text(
                        "Warp: the clip follows the song tempo. Set the tempo the audio was played at.",
                    )
                    .changed()
                {
                    let warp = if warped {
                        edit::warp_at_tempo(&self.project, id)
                    } else {
                        None
                    };
                    self.edit(|p| {
                        edit::set_warp(p, id, warp);
                    });
                }
                if let Some(warp) = region.warp {
                    let mut bpm = warp.bpm;
                    let response = ui
                        .add(
                            egui::DragValue::new(&mut bpm)
                                .range(
                                    fnoob_core::model::MIN_WARP_BPM
                                        ..=fnoob_core::model::MAX_WARP_BPM,
                                )
                                .speed(0.1)
                                .fixed_decimals(2)
                                .suffix(" BPM"),
                        )
                        .on_hover_text("Tempo the audio was played at");
                    self.live_edit(&response, |p| {
                        edit::set_warp(p, id, Some(fnoob_core::model::Warp { bpm, ..warp }));
                    });
                    for (label, factor, info) in [
                        (":2", 0.5, "Half the original tempo: the clip plays twice as long"),
                        ("*2", 2.0, "Double the original tempo: the clip plays half as long"),
                    ] {
                        if ui.small_button(label).on_hover_text(info).clicked() {
                            let next = fnoob_core::model::Warp {
                                bpm: warp.bpm * factor,
                                ..warp
                            };
                            self.edit(|p| {
                                edit::set_warp(p, id, Some(next));
                            });
                        }
                    }
                }
            });
            if let Some(warp) = region.warp {
                ui.horizontal(|ui| {
                    ui.label("Mode");
                    for mode in fnoob_core::model::WarpMode::ALL {
                        let info = match mode {
                            fnoob_core::model::WarpMode::Stretch => {
                                "Stretch: change the speed and keep the pitch"
                            }
                            fnoob_core::model::WarpMode::Repitch => {
                                "Repitch: speed and pitch change together, like a turntable"
                            }
                        };
                        if ui
                            .selectable_label(warp.mode == mode, mode.label())
                            .on_hover_text(info)
                            .clicked()
                            && warp.mode != mode
                        {
                            self.edit(|p| {
                                edit::set_warp(
                                    p,
                                    id,
                                    Some(fnoob_core::model::Warp { mode, ..warp }),
                                );
                            });
                        }
                    }
                });
            }
        }
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
}
