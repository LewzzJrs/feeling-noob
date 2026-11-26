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
                } => {
                    if region.warp.is_some() {
                        self.warped_clip_view(ui, &region, file_id);
                    } else {
                        self.audio_clip_view(
                            ui,
                            file_id,
                            offset_frames,
                            fnoob_core::edit::region_frames(
                                &self.project,
                                &region,
                                region.start_tick,
                                region.length_tick,
                            ),
                        )
                    }
                }
            });
        });
    }

    fn warped_clip_view(&mut self, ui: &mut Ui, region: &fnoob_core::model::Region, file: Id) {
        let (rect, background) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
        let (Some(data), Some(map)) = (
            self.pool.get(&file).cloned(),
            fnoob_core::edit::warp_map(&self.project, region),
        ) else {
            return;
        };
        let ruler = egui::Rect::from_min_size(rect.min, vec2(rect.width(), 16.0));
        let body = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 6.0, ruler.bottom() + 4.0),
            egui::pos2(rect.right() - 6.0, rect.bottom() - 6.0),
        );
        let origin = map.tick(region.audio_offset() as f64);
        let length = region.length_tick.max(1) as f64;
        let width = f64::from(body.width().max(1.0));
        let left = body.left();
        let x_of = |tick: f64| left + ((tick - origin) / length * width) as f32;
        let tick_of = |x: f32| origin + f64::from(x - left) / width * length;
        let beat = fnoob_core::time::ticks_per_beat(self.project.time_signature).max(1) as f64;
        let bar = beat * f64::from(self.project.time_signature.numerator.max(1));
        let mut tick = (origin / beat).floor() * beat;
        while tick <= origin + length {
            let x = x_of(tick);
            if x >= body.left() {
                let strong = ((tick - origin).rem_euclid(bar)).abs() < 0.5;
                painter.vline(
                    x,
                    body.y_range(),
                    Stroke::new(1.0, Color32::from_white_alpha(if strong { 40 } else { 14 })),
                );
                if strong {
                    painter.text(
                        egui::pos2(x + 3.0, ruler.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("{}", ((tick - origin) / bar).round() as i64 + 1),
                        egui::FontId::proportional(10.0),
                        ui.visuals().weak_text_color(),
                    );
                }
            }
            tick += beat;
        }
        crate::waveform::paint(
            &painter,
            body,
            rect,
            &data,
            1.0,
            crate::waveform::Style::on_dark(Color32::from_rgb(120, 190, 230)),
            |x| map.frame(tick_of(x)),
        );
        let id = region.id;
        let marker_color = Color32::from_rgb(240, 200, 60);
        for (index, marker) in region.warp_markers.iter().enumerate() {
            let x = x_of(marker.tick);
            if x < body.left() - 1.0 || x > body.right() + 1.0 {
                continue;
            }
            painter.vline(x, body.y_range(), Stroke::new(1.5, marker_color));
            let handle =
                egui::Rect::from_center_size(egui::pos2(x, ruler.center().y), vec2(10.0, 14.0));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    handle.left_top(),
                    handle.right_top(),
                    egui::pos2(x, handle.bottom()),
                ],
                marker_color,
                Stroke::NONE,
            ));
            let response = hint(
                ui.interact(
                    handle.expand(3.0),
                    ui.scope_id().with(("warp_marker", id, index)),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal),
                "Warp marker: drag to move this point of the audio in time. Right-click to delete it.",
            );
            if response.drag_started() {
                self.history.record(self.project.clone());
            }
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let tick = tick_of(pos.x);
                if fnoob_core::edit::move_warp_marker(&mut self.project, id, index, tick) {
                    self.sync_plan();
                }
            }
            if response.secondary_clicked() {
                self.edit(|p| {
                    fnoob_core::edit::delete_warp_marker(p, id, index);
                });
            }
        }
        let background = hint(
            background,
            "Warped clip: the grid shows beats. Double-click the audio to add a warp marker, then drag its yellow handle.",
        );
        if background.double_clicked()
            && let Some(pos) = background.interact_pointer_pos()
            && body.contains(pos)
        {
            let frame = map.frame(tick_of(pos.x)).max(0.0).round() as u64;
            self.edit(|p| {
                fnoob_core::edit::add_warp_marker(p, id, frame);
            });
        }
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
        let body = rect.shrink2(vec2(6.0, 8.0));
        let width = f64::from(body.width().max(1.0));
        let left = body.left();
        for fraction in [0.25, 0.5, 0.75] {
            let x = body.left() + body.width() * fraction;
            painter.vline(
                x,
                body.y_range(),
                Stroke::new(1.0, Color32::from_white_alpha(14)),
            );
        }
        crate::waveform::paint(
            &painter,
            body,
            rect,
            data,
            1.0,
            crate::waveform::Style::on_dark(Color32::from_rgb(120, 190, 230)),
            |x| offset as f64 + f64::from(x - left) / width * frames as f64,
        );
    }
}
