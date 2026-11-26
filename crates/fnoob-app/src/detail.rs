use crate::app::App;
use crate::ui_kit::hint;
use eframe::egui::{self, Color32, Layout, RichText, Sense, Stroke, Ui, vec2};
use fnoob_core::keymap::Command;
use fnoob_core::model::{Id, RegionContent};
use fnoob_core::session;

const PROPERTIES_WIDTH: f32 = 230.0;

impl App {
    pub(crate) fn detail_visible(&self) -> bool {
        self.editor.open || self.devices_open
    }

    pub(crate) fn show_devices(&mut self) {
        self.devices_open = true;
        self.editor.open = false;
        self.detail_devices = true;
    }

    pub(crate) fn show_clip(&mut self) {
        self.devices_open = false;
        self.editor.open = true;
        self.detail_devices = false;
    }

    pub(crate) fn toggle_detail(&mut self) {
        if self.detail_visible() {
            self.editor.open = false;
            self.devices_open = false;
        } else if self.detail_devices {
            self.show_devices();
        } else {
            self.show_clip();
        }
    }

    pub(crate) fn switch_detail(&mut self) {
        if self.devices_open {
            self.show_clip();
        } else {
            self.show_devices();
        }
    }

    pub(crate) fn rename_selection(&mut self) {
        let clip = self.session.open && self.session.selected.is_some();
        if clip || !self.selection.regions.is_empty() {
            self.show_clip();
            self.inspector.focus_name = true;
        } else if let Some(track) = self.selection.track.and_then(|id| self.project.track(id)) {
            self.renaming = Some((track.id, track.name.clone()));
        }
    }

    fn detail_target(&self) -> Option<Id> {
        if self.session.open
            && let Some(id) = self
                .session
                .selected
                .filter(|id| session::clip_position(&self.project, *id).is_some())
        {
            return Some(id);
        }
        self.selection
            .regions
            .first()
            .copied()
            .or(self.editor.region)
            .filter(|id| self.project.find_region(*id).is_some())
    }

    pub(crate) fn detail_area(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let clip = self.editor.open;
            if hint(
                ui.selectable_label(clip, "Clip"),
                "Clip view: notes and settings of the selected clip (Shift+Tab).",
            )
            .clicked()
            {
                self.show_clip();
            }
            if hint(
                ui.selectable_label(!clip, "Devices"),
                "Device view: instrument and effects of the selected track (Shift+Tab).",
            )
            .clicked()
            {
                self.show_devices();
            }
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if hint(ui.small_button("Hide"), "Hide this area (Ctrl+Alt+L).").clicked() {
                    self.run(Command::ToggleDetail);
                }
            });
        });
        ui.separator();
        if self.devices_open {
            self.device_chain(ui);
        } else {
            self.clip_view(ui);
        }
    }

    fn clip_view(&mut self, ui: &mut Ui) {
        let Some(id) = self.detail_target() else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(
                        "Select a clip to edit it here. Double-click an empty spot on an instrument track to make one.",
                    )
                    .weak(),
                );
            });
            return;
        };
        let grid_clip = session::clip_position(&self.project, id)
            .and_then(|(t, s)| session::clip(&self.project, t, s))
            .cloned();
        let region = self.project.find_region(id).map(|(_, r)| r.clone());
        let Some(region) = region else {
            return;
        };
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(PROPERTIES_WIDTH, ui.available_height()),
                Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(PROPERTIES_WIDTH);
                    egui::ScrollArea::vertical()
                        .id_salt("clip_properties")
                        .show(ui, |ui| match &grid_clip {
                            Some(clip) => self.clip_section(ui, clip),
                            None => self.region_section(ui, &region),
                        });
                },
            );
            ui.separator();
            ui.vertical(|ui| match region.content {
                RegionContent::Midi { .. } => {
                    if self.editor.region != Some(id) {
                        self.open_editor(id);
                    }
                    self.piano_roll(ui);
                }
                RegionContent::Audio {
                    file_id,
                    offset_frames,
                    ..
                } => self.audio_clip_view(
                    ui,
                    file_id,
                    offset_frames,
                    fnoob_core::edit::region_frames(
                        &self.project,
                        &region,
                        region.start_tick,
                        region.length_tick,
                    ),
                ),
            });
        });
    }

    fn audio_clip_view(&self, ui: &mut Ui, file: Id, offset: u64, frames: u64) {
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
        let Some(data) = self.pool.get(&file) else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The audio file is missing.",
                egui::FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        let width = rect.width().max(1.0) as u64;
        let color = Color32::from_rgb(120, 190, 230);
        let mid = rect.center().y;
        let half = rect.height() * 0.45;
        for x in 0..width {
            let from = offset + frames * x / width;
            let to = offset + frames * (x + 1) / width;
            let peak = data.peak(from, to.max(from + 1)).min(1.0);
            let px = rect.left() + x as f32;
            painter.line_segment(
                [
                    egui::pos2(px, mid - peak * half),
                    egui::pos2(px, mid + peak * half),
                ],
                Stroke::new(1.0, color),
            );
        }
    }
}
