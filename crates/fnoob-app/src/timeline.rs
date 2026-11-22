use crate::app::{App, Drag, Pending};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2,
};
use fnoob_core::edit;
use fnoob_core::io::PROJECT_FILE;
use fnoob_core::model::{AudioInput, Id, Region, RegionContent, TrackKind};
use fnoob_core::time::{grid_ticks, snap_floor, snap_round, ticks_per_bar};
use std::path::PathBuf;

pub const HEADER_WIDTH: f32 = 210.0;
pub const LOOP_ROW: f32 = 12.0;
pub const RULER_HEIGHT: f32 = 34.0;
pub const LANE_HEIGHT: f32 = 60.0;
pub const MIN_PIXELS_PER_TICK: f32 = 0.002;
pub const MAX_PIXELS_PER_TICK: f32 = 0.5;
const EDGE: f32 = 6.0;

const TRACK_PALETTE: [Color32; 8] = [
    Color32::from_rgb(86, 156, 214),
    Color32::from_rgb(214, 118, 86),
    Color32::from_rgb(120, 190, 110),
    Color32::from_rgb(196, 120, 200),
    Color32::from_rgb(220, 190, 90),
    Color32::from_rgb(90, 200, 190),
    Color32::from_rgb(230, 110, 150),
    Color32::from_rgb(150, 150, 230),
];
const LOOP_ON: Color32 = Color32::from_rgb(64, 170, 150);
const LOOP_OFF: Color32 = Color32::from_rgb(80, 80, 86);
const PLAYHEAD: Color32 = Color32::from_rgb(240, 240, 240);

pub fn track_color(index: u8) -> Color32 {
    TRACK_PALETTE[usize::from(index) % TRACK_PALETTE.len()]
}

pub enum UiAction {
    SelectTrack(Id),
    SelectRegion {
        id: Id,
        toggle: bool,
    },
    ClearRegions,
    CreateRegion {
        track: Id,
        tick: u64,
    },
    DragRegionsStart {
        primary: Id,
        pointer: Pos2,
    },
    ResizeRegionStart {
        region: Id,
        pointer: Pos2,
    },
    DragRegionsUpdate {
        pointer: Pos2,
    },
    OpenEditor(Id),
    DragLoopStart {
        tick: u64,
    },
    DragLoopUpdate {
        tick: u64,
    },
    DragEnd,
    ToggleLoop,
    Locate(u64),
    ToggleMute(Id),
    ToggleSolo(Id),
    ToggleRecord(Id),
    ToggleMonitor(Id),
    SetInput {
        id: Id,
        input: AudioInput,
    },
    StartRename(Id),
    Rename {
        id: Id,
        name: String,
    },
    CancelRename,
    Scroll {
        ticks: f32,
        pixels: f32,
    },
    Zoom(f32),
    DropFiles {
        paths: Vec<PathBuf>,
        track: Option<Id>,
        tick: u64,
    },
}

struct Geometry {
    ruler: Rect,
    lanes: Rect,
    headers: Rect,
    pixels_per_tick: f32,
    scroll_tick: f32,
    scroll_y: f32,
}

impl Geometry {
    fn x(&self, tick: u64) -> f32 {
        self.lanes.left() + (tick as f32 - self.scroll_tick) * self.pixels_per_tick
    }

    fn tick(&self, x: f32) -> u64 {
        ((x - self.lanes.left()) / self.pixels_per_tick + self.scroll_tick).max(0.0) as u64
    }

    fn lane_top(&self, index: usize) -> f32 {
        self.lanes.top() + index as f32 * LANE_HEIGHT - self.scroll_y
    }
}

impl App {
    fn snapped(&self, tick: u64) -> u64 {
        if self.project.snap.enabled {
            snap_round(
                tick,
                grid_ticks(self.project.snap.grid, self.project.time_signature),
            )
        } else {
            tick
        }
    }

    pub fn tracks_area(&mut self, ui: &mut egui::Ui) {
        let (area, area_response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let geometry = Geometry {
            ruler: Rect::from_min_max(
                Pos2::new(area.left() + HEADER_WIDTH, area.top()),
                Pos2::new(area.right(), area.top() + RULER_HEIGHT),
            ),
            lanes: Rect::from_min_max(
                Pos2::new(area.left() + HEADER_WIDTH, area.top() + RULER_HEIGHT),
                area.max,
            ),
            headers: Rect::from_min_max(
                Pos2::new(area.left(), area.top() + RULER_HEIGHT),
                Pos2::new(area.left() + HEADER_WIDTH, area.bottom()),
            ),
            pixels_per_tick: self.view.pixels_per_tick,
            scroll_tick: self.view.scroll_tick,
            scroll_y: self.view.scroll_y,
        };
        let mut actions = Vec::new();
        if area_response.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if zoom != 1.0 {
                actions.push(UiAction::Zoom(zoom));
            } else if scroll != Vec2::ZERO {
                actions.push(UiAction::Scroll {
                    ticks: -scroll.x / geometry.pixels_per_tick,
                    pixels: -scroll.y,
                });
            }
        }
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 0.0, ui.visuals().extreme_bg_color);
        self.draw_lanes(ui, &geometry, &mut actions);
        self.draw_ruler(ui, &geometry, &mut actions);
        self.draw_headers(ui, &geometry, &mut actions);
        let playhead = geometry.x(self.playhead_tick());
        if playhead >= geometry.lanes.left() {
            painter.vline(
                playhead,
                geometry.ruler.top()..=area.bottom(),
                Stroke::new(1.5, PLAYHEAD),
            );
        }
        self.handle_file_drop(ui, &geometry, area, &mut actions);
        for action in actions {
            self.apply_ui_action(action);
        }
    }

    fn handle_file_drop(
        &self,
        ui: &egui::Ui,
        g: &Geometry,
        area: Rect,
        actions: &mut Vec<UiAction>,
    ) {
        let (hovering, dropped, pointer) = ui.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw.dropped_files.clone(),
                i.pointer.latest_pos(),
            )
        });
        if hovering {
            let painter = ui.painter_at(area);
            painter.rect_filled(area, 0.0, Color32::from_black_alpha(120));
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                "Drop WAV files to import them, or a .fnoob folder to open it",
                FontId::proportional(18.0),
                Color32::WHITE,
            );
        }
        let paths: Vec<PathBuf> = dropped.iter().map(|f| f.path().to_path_buf()).collect();
        if paths.is_empty() {
            return;
        }
        let inside = pointer.filter(|p| g.lanes.contains(*p));
        let track = inside.and_then(|p| {
            let index = ((p.y - g.lanes.top() + g.scroll_y) / LANE_HEIGHT).floor();
            (index >= 0.0)
                .then(|| self.project.tracks.get(index as usize))
                .flatten()
                .map(|t| t.id)
        });
        let tick = inside.map_or_else(|| self.playhead_tick(), |p| self.snapped(g.tick(p.x)));
        actions.push(UiAction::DropFiles {
            paths,
            track: track.or(self.selection.track),
            tick,
        });
    }

    fn draw_ruler(&self, ui: &mut egui::Ui, g: &Geometry, actions: &mut Vec<UiAction>) {
        let painter = ui.painter_at(g.ruler);
        let visuals = ui.visuals();
        painter.rect_filled(g.ruler, 0.0, visuals.faint_bg_color);
        let loop_row = Rect::from_min_size(g.ruler.min, Vec2::new(g.ruler.width(), LOOP_ROW));
        let bar_row = Rect::from_min_max(Pos2::new(g.ruler.left(), loop_row.bottom()), g.ruler.max);
        let range = self.project.loop_range;
        let loop_rect = Rect::from_x_y_ranges(
            g.x(range.start_tick)..=g.x(range.end_tick),
            loop_row.top() + 2.0..=loop_row.bottom() - 2.0,
        );
        let loop_color = if range.enabled { LOOP_ON } else { LOOP_OFF };
        painter.rect_filled(loop_rect, CornerRadius::same(2), loop_color);
        let bar = ticks_per_bar(self.project.time_signature);
        let bar_px = bar as f32 * g.pixels_per_tick;
        let label_every = ((48.0 / bar_px).ceil() as u64).max(1);
        let first_bar = snap_floor(g.tick(g.ruler.left()), bar) / bar;
        let last_bar = g.tick(g.ruler.right()) / bar + 1;
        let text_color = visuals.text_color();
        for index in first_bar..=last_bar {
            let x = g.x(index * bar);
            let labeled = index % label_every == 0;
            let top = if labeled {
                bar_row.top()
            } else {
                bar_row.center().y
            };
            painter.vline(
                x,
                top..=bar_row.bottom(),
                Stroke::new(1.0, visuals.weak_text_color()),
            );
            if labeled {
                painter.text(
                    Pos2::new(x + 3.0, bar_row.top() + 2.0),
                    Align2::LEFT_TOP,
                    (index + 1).to_string(),
                    FontId::proportional(12.0),
                    text_color,
                );
            }
        }
        let loop_response = ui.interact(
            loop_row,
            ui.scope_id().with("loop_row"),
            Sense::click_and_drag(),
        );
        if let Some(pos) = loop_response.interact_pointer_pos() {
            let tick = self.snapped(g.tick(pos.x));
            if loop_response.drag_started() {
                actions.push(UiAction::DragLoopStart { tick });
            } else if loop_response.dragged() {
                actions.push(UiAction::DragLoopUpdate { tick });
            }
        }
        if loop_response.drag_stopped() {
            actions.push(UiAction::DragEnd);
        } else if loop_response.clicked() {
            actions.push(UiAction::ToggleLoop);
        }
        let bar_response = ui.interact(
            bar_row,
            ui.scope_id().with("bar_row"),
            Sense::click_and_drag(),
        );
        if (bar_response.clicked() || bar_response.dragged())
            && let Some(pos) = bar_response.interact_pointer_pos()
        {
            actions.push(UiAction::Locate(self.snapped(g.tick(pos.x))));
        }
    }

    fn draw_lanes(&self, ui: &mut egui::Ui, g: &Geometry, actions: &mut Vec<UiAction>) {
        let painter = ui.painter_at(g.lanes);
        let visuals = ui.visuals().clone();
        let bar = ticks_per_bar(self.project.time_signature);
        let first_bar = snap_floor(g.tick(g.lanes.left()), bar) / bar;
        let last_bar = g.tick(g.lanes.right()) / bar + 1;
        for index in first_bar..=last_bar {
            painter.vline(
                g.x(index * bar),
                g.lanes.y_range(),
                Stroke::new(1.0, visuals.faint_bg_color),
            );
        }
        let range = self.project.loop_range;
        if range.enabled {
            let shade = Rect::from_x_y_ranges(
                g.x(range.start_tick)..=g.x(range.end_tick),
                g.lanes.y_range(),
            );
            painter.rect_filled(shade, 0.0, LOOP_ON.gamma_multiply(0.08));
        }
        for (index, track) in self.project.tracks.iter().enumerate() {
            let top = g.lane_top(index);
            let lane = Rect::from_x_y_ranges(g.lanes.x_range(), top..=top + LANE_HEIGHT);
            if lane.bottom() < g.lanes.top() || lane.top() > g.lanes.bottom() {
                continue;
            }
            if self.selection.track == Some(track.id) {
                painter.rect_filled(lane, 0.0, visuals.selection.bg_fill.gamma_multiply(0.15));
            }
            painter.hline(
                g.lanes.x_range(),
                lane.bottom(),
                Stroke::new(1.0, visuals.faint_bg_color),
            );
            let lane_response = ui.interact(
                lane.intersect(g.lanes),
                ui.scope_id().with(("lane", track.id)),
                Sense::click(),
            );
            if lane_response.double_clicked() && track.kind == TrackKind::Instrument {
                if let Some(pos) = lane_response.interact_pointer_pos() {
                    let tick = snap_floor(
                        g.tick(pos.x),
                        grid_ticks(self.project.snap.grid, self.project.time_signature),
                    );
                    actions.push(UiAction::CreateRegion {
                        track: track.id,
                        tick,
                    });
                }
            } else if lane_response.clicked() {
                actions.push(UiAction::SelectTrack(track.id));
                actions.push(UiAction::ClearRegions);
            }
            let audible = self.project.is_track_audible(track);
            for region in &track.regions {
                let rect = Rect::from_x_y_ranges(
                    g.x(region.start_tick)
                        ..=g.x(region.end_tick()).max(g.x(region.start_tick) + 4.0),
                    lane.top() + 3.0..=lane.bottom() - 3.0,
                );
                if rect.right() < g.lanes.left() || rect.left() > g.lanes.right() {
                    continue;
                }
                let selected = self.selection.regions.contains(&region.id);
                let missing = region
                    .audio_file()
                    .is_some_and(|id| !self.pool.contains_key(&id));
                let base = if missing {
                    Color32::from_rgb(150, 60, 60)
                } else if region.muted || !audible {
                    Color32::from_gray(90)
                } else {
                    track_color(track.color)
                };
                painter.rect_filled(rect, CornerRadius::same(4), base.gamma_multiply(0.75));
                let border = if selected {
                    Stroke::new(2.0, Color32::WHITE)
                } else {
                    Stroke::new(1.0, base)
                };
                painter.rect_stroke(rect, CornerRadius::same(4), border, StrokeKind::Inside);
                self.draw_waveform(ui, g, region, rect);
                let notes = region.notes();
                if let (Some(low), Some(high)) = (
                    notes.iter().map(|n| n.pitch).min(),
                    notes.iter().map(|n| n.pitch).max(),
                ) {
                    let body = Rect::from_x_y_ranges(
                        rect.x_range(),
                        rect.top() + 18.0..=rect.bottom() - 4.0,
                    );
                    let span = f32::from(high - low).max(12.0);
                    let note_painter = ui.painter_at(body.intersect(g.lanes));
                    for note in notes {
                        let start = region.start_tick + note.start_tick;
                        if start >= region.end_tick() {
                            continue;
                        }
                        let end = (start + note.length_tick).min(region.end_tick());
                        let y = body.bottom() - body.height() * f32::from(note.pitch - low) / span;
                        note_painter.hline(
                            g.x(start)..=g.x(end).max(g.x(start) + 1.0),
                            y,
                            Stroke::new(2.0, Color32::from_black_alpha(170)),
                        );
                    }
                }
                let label = if missing {
                    format!("{} (missing file)", region.name)
                } else if region.muted {
                    format!("{} (muted)", region.name)
                } else {
                    region.name.clone()
                };
                ui.painter_at(rect.intersect(g.lanes)).text(
                    rect.left_top() + Vec2::new(6.0, 4.0),
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(12.0),
                    Color32::BLACK,
                );
                let response = ui.interact(
                    rect.intersect(g.lanes),
                    ui.scope_id().with(("region", region.id)),
                    Sense::click_and_drag(),
                );
                let near_edge =
                    |pos: Pos2| pos.x >= rect.right() - EDGE && rect.width() > 3.0 * EDGE;
                let response = if response.hover_pos().is_some_and(near_edge) {
                    response.on_hover_cursor(CursorIcon::ResizeHorizontal)
                } else {
                    response
                };
                if response.drag_started() {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if near_edge(pointer) {
                            actions.push(UiAction::ResizeRegionStart {
                                region: region.id,
                                pointer,
                            });
                        } else {
                            actions.push(UiAction::DragRegionsStart {
                                primary: region.id,
                                pointer,
                            });
                        }
                    }
                } else if response.double_clicked() {
                    actions.push(UiAction::OpenEditor(region.id));
                } else if response.dragged() {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        actions.push(UiAction::DragRegionsUpdate { pointer });
                    }
                } else if response.clicked() {
                    let toggle = ui.input(|i| i.modifiers.ctrl);
                    actions.push(UiAction::SelectTrack(track.id));
                    actions.push(UiAction::SelectRegion {
                        id: region.id,
                        toggle,
                    });
                }
                if response.drag_stopped() {
                    actions.push(UiAction::DragEnd);
                }
            }
        }
    }

    fn draw_headers(&mut self, ui: &mut egui::Ui, g: &Geometry, actions: &mut Vec<UiAction>) {
        let painter = ui.painter_at(g.headers);
        painter.rect_filled(g.headers, 0.0, ui.visuals().panel_fill);
        let corner = Rect::from_min_max(
            Pos2::new(g.headers.left(), g.ruler.top()),
            Pos2::new(g.headers.right(), g.ruler.bottom()),
        );
        ui.painter_at(corner)
            .rect_filled(corner, 0.0, ui.visuals().panel_fill);
        let mut renaming = self.renaming.take();
        for (index, track) in self.project.tracks.iter().enumerate() {
            let top = g.lane_top(index);
            let row = Rect::from_x_y_ranges(g.headers.x_range(), top..=top + LANE_HEIGHT);
            if row.bottom() < g.headers.top() || row.top() > g.headers.bottom() {
                continue;
            }
            let selected = self.selection.track == Some(track.id);
            let row_response = ui.interact(
                row.intersect(g.headers),
                ui.scope_id().with(("header", track.id)),
                Sense::click(),
            );
            if row_response.clicked() {
                actions.push(UiAction::SelectTrack(track.id));
            }
            let fill = if selected {
                ui.visuals().selection.bg_fill.gamma_multiply(0.35)
            } else {
                ui.visuals().faint_bg_color
            };
            painter.rect_filled(row.shrink2(Vec2::new(0.0, 1.0)), 0.0, fill);
            painter.rect_filled(
                Rect::from_min_size(row.min, Vec2::new(5.0, LANE_HEIGHT)),
                0.0,
                track_color(track.color),
            );
            let mut child =
                ui.new_child(UiBuilder::new().max_rect(row.shrink2(Vec2::new(12.0, 6.0))));
            child.set_clip_rect(g.headers);
            child.vertical(|ui| {
                match &mut renaming {
                    Some((id, text)) if *id == track.id => {
                        let edit = ui.add(egui::TextEdit::singleline(text).desired_width(170.0));
                        edit.request_focus();
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            actions.push(UiAction::CancelRename);
                        } else if edit.lost_focus() {
                            actions.push(UiAction::Rename {
                                id: track.id,
                                name: text.clone(),
                            });
                        }
                    }
                    _ => {
                        let kind = match track.kind {
                            TrackKind::Audio => "Audio",
                            TrackKind::Instrument => "Instrument",
                        };
                        let name = ui
                            .add(
                                egui::Label::new(egui::RichText::new(&track.name).strong())
                                    .sense(Sense::click()),
                            )
                            .on_hover_text(format!("{kind} track. Double-click to rename."));
                        if name.double_clicked() {
                            actions.push(UiAction::StartRename(track.id));
                        } else if name.clicked() {
                            actions.push(UiAction::SelectTrack(track.id));
                        }
                    }
                }
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(track.mute, "M")
                        .on_hover_text("Mute")
                        .clicked()
                    {
                        actions.push(UiAction::ToggleMute(track.id));
                    }
                    if ui
                        .selectable_label(track.solo, "S")
                        .on_hover_text("Solo")
                        .clicked()
                    {
                        actions.push(UiAction::ToggleSolo(track.id));
                    }
                    let record = egui::Button::selectable(
                        track.record_enabled,
                        egui::RichText::new("R").color(if track.record_enabled {
                            Color32::from_rgb(230, 70, 70)
                        } else {
                            ui.visuals().text_color()
                        }),
                    );
                    if ui.add(record).on_hover_text("Record enable").clicked() {
                        actions.push(UiAction::ToggleRecord(track.id));
                    }
                    if track.kind == TrackKind::Audio {
                        if ui
                            .selectable_label(track.input_monitoring, "I")
                            .on_hover_text("Input monitoring")
                            .clicked()
                        {
                            actions.push(UiAction::ToggleMonitor(track.id));
                        }
                        let current = track.audio_input.unwrap_or_default();
                        egui::ComboBox::from_id_salt(("input", track.id))
                            .width(62.0)
                            .selected_text(input_label(current))
                            .show_ui(ui, |ui| {
                                for option in input_options(self.input_channels()) {
                                    if ui
                                        .selectable_label(option == current, input_label(option))
                                        .clicked()
                                    {
                                        actions.push(UiAction::SetInput {
                                            id: track.id,
                                            input: option,
                                        });
                                    }
                                }
                            });
                    }
                });
            });
        }
        if renaming.is_some()
            && !actions
                .iter()
                .any(|a| matches!(a, UiAction::Rename { .. } | UiAction::CancelRename))
        {
            self.renaming = renaming;
        }
    }

    pub fn apply_ui_action(&mut self, action: UiAction) {
        match action {
            UiAction::SelectTrack(id) => self.selection.track = Some(id),
            UiAction::SelectRegion { id, toggle } => {
                if toggle {
                    if let Some(i) = self.selection.regions.iter().position(|&r| r == id) {
                        self.selection.regions.remove(i);
                    } else {
                        self.selection.regions.push(id);
                    }
                } else {
                    self.selection.regions = vec![id];
                }
            }
            UiAction::ClearRegions => self.selection.regions.clear(),
            UiAction::CreateRegion { track, tick } => {
                let length = ticks_per_bar(self.project.time_signature);
                let mut created = None;
                self.edit(|p| created = edit::add_midi_region(p, track, tick, length));
                if let Some(id) = created {
                    self.selection.track = Some(track);
                    self.selection.regions = vec![id];
                }
            }
            UiAction::DragRegionsStart { primary, pointer } => {
                if !self.selection.regions.contains(&primary) {
                    self.selection.regions = vec![primary];
                }
                self.drag = Some(Drag::Regions {
                    origin: self.project.clone(),
                    primary,
                    anchor: pointer,
                });
            }
            UiAction::ResizeRegionStart { region, pointer } => {
                self.selection.regions = vec![region];
                self.drag = Some(Drag::RegionResize {
                    origin: self.project.clone(),
                    region,
                    anchor: pointer,
                });
            }
            UiAction::DragRegionsUpdate { pointer } => match self.drag {
                Some(Drag::RegionResize { .. }) => self.update_region_resize(pointer),
                _ => self.update_region_drag(pointer),
            },
            UiAction::OpenEditor(region) => {
                if self
                    .project
                    .find_region(region)
                    .is_some_and(|(_, r)| r.audio_file().is_some())
                {
                    self.message = Some("Audio regions have no piano roll.".into());
                } else {
                    self.open_editor(region);
                }
            }
            UiAction::ToggleMonitor(id) => {
                self.edit_track(id, |t| t.input_monitoring = !t.input_monitoring)
            }
            UiAction::SetInput { id, input } => {
                self.edit_track(id, |t| t.audio_input = Some(input));
            }
            UiAction::DragLoopStart { tick } => {
                self.drag = Some(Drag::Loop {
                    origin: self.project.clone(),
                    anchor: tick,
                });
            }
            UiAction::DragLoopUpdate { tick } => self.update_loop_drag(tick),
            UiAction::DragEnd => self.finish_drag(),
            UiAction::ToggleLoop => self.edit(|p| p.loop_range.enabled = !p.loop_range.enabled),
            UiAction::Locate(tick) => self.locate(tick),
            UiAction::ToggleMute(id) => self.edit_track(id, |t| t.mute = !t.mute),
            UiAction::ToggleSolo(id) => self.edit_track(id, |t| t.solo = !t.solo),
            UiAction::ToggleRecord(id) => {
                self.edit_track(id, |t| t.record_enabled = !t.record_enabled)
            }
            UiAction::StartRename(id) => {
                if let Some(track) = self.project.track(id) {
                    self.renaming = Some((id, track.name.clone()));
                }
            }
            UiAction::Rename { id, name } => {
                let name = name.trim().to_owned();
                if !name.is_empty() {
                    self.edit_track(id, |t| t.name = name);
                }
            }
            UiAction::CancelRename => {}
            UiAction::Scroll { ticks, pixels } => {
                self.view.scroll_tick = (self.view.scroll_tick + ticks).max(0.0);
                let content = self.project.tracks.len() as f32 * LANE_HEIGHT;
                self.view.scroll_y =
                    (self.view.scroll_y + pixels).clamp(0.0, (content - LANE_HEIGHT).max(0.0));
            }
            UiAction::Zoom(factor) => self.zoom(factor),
            UiAction::DropFiles { paths, track, tick } => {
                let project = paths.iter().find_map(|p| {
                    if p.is_dir() && p.join(PROJECT_FILE).is_file() {
                        Some(p.clone())
                    } else if p.file_name().is_some_and(|n| n == PROJECT_FILE) {
                        p.parent().map(PathBuf::from)
                    } else {
                        None
                    }
                });
                if let Some(dir) = project {
                    self.request(Pending::OpenPath(dir));
                    return;
                }
                let (wavs, others): (Vec<PathBuf>, Vec<PathBuf>) = paths
                    .into_iter()
                    .partition(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")));
                if wavs.is_empty() {
                    self.message = Some(format!(
                        "Only WAV files and project folders can be dropped ({} other files skipped).",
                        others.len()
                    ));
                    return;
                }
                self.import_files(&wavs, track, tick);
            }
        }
    }

    fn draw_waveform(&self, ui: &egui::Ui, g: &Geometry, region: &Region, rect: Rect) {
        let RegionContent::Audio {
            file_id,
            offset_frames,
            gain_db,
        } = region.content
        else {
            return;
        };
        let Some(data) = self.pool.get(&file_id) else {
            return;
        };
        let body = Rect::from_x_y_ranges(rect.x_range(), rect.top() + 18.0..=rect.bottom() - 3.0);
        let visible = body.intersect(g.lanes);
        if visible.width() <= 0.0 {
            return;
        }
        let painter = ui.painter_at(visible);
        let timeline = self.timeline();
        let region_start = timeline.tick_to_sample(region.start_tick);
        let gain = 10f32.powf(gain_db / 20.0);
        let frame_at = |x: f32| {
            let sample = timeline.tick_to_sample(g.tick(x));
            offset_frames + sample.saturating_sub(region_start)
        };
        let stroke = Stroke::new(1.0, Color32::from_black_alpha(170));
        let mut x = visible.left();
        while x < visible.right() {
            let peak = (data.peak(frame_at(x), frame_at(x + 2.0)) * gain).min(1.0);
            let half = body.height() * 0.5 * peak;
            if half >= 0.5 {
                painter.vline(x, body.center().y - half..=body.center().y + half, stroke);
            }
            x += 2.0;
        }
    }

    pub fn zoom(&mut self, factor: f32) {
        self.view.pixels_per_tick =
            (self.view.pixels_per_tick * factor).clamp(MIN_PIXELS_PER_TICK, MAX_PIXELS_PER_TICK);
    }

    fn update_region_drag(&mut self, pointer: Pos2) {
        let Some(Drag::Regions {
            origin,
            primary,
            anchor,
        }) = &self.drag
        else {
            return;
        };
        let (origin, primary, anchor) = (origin.clone(), *primary, *anchor);
        let Some((primary_track, primary_region)) = origin.find_region(primary) else {
            return;
        };
        let delta_ticks = ((pointer.x - anchor.x) / self.view.pixels_per_tick) as i64;
        let wanted = (primary_region.start_tick as i64 + delta_ticks).max(0) as u64;
        let delta = self.snapped(wanted) as i64 - primary_region.start_tick as i64;
        let lane_delta = ((pointer.y - anchor.y) / LANE_HEIGHT).round() as i64;
        let ids = self.selection.regions.clone();
        let source_index = origin
            .tracks
            .iter()
            .position(|t| t.id == primary_track.id)
            .unwrap_or(0);
        let target =
            (source_index as i64 + lane_delta).clamp(0, origin.tracks.len() as i64 - 1) as usize;
        let mut project = origin.clone();
        let min_start = ids
            .iter()
            .filter_map(|id| origin.find_region(*id).map(|(_, r)| r.start_tick))
            .min()
            .unwrap_or(0);
        let delta = delta.max(-(min_start as i64));
        for id in &ids {
            let Some((track, region)) = origin.find_region(*id) else {
                continue;
            };
            let start = (region.start_tick as i64 + delta) as u64;
            let track_id = if ids.len() == 1 && origin.tracks[target].kind == track.kind {
                origin.tracks[target].id
            } else {
                track.id
            };
            edit::move_region(&mut project, *id, start, track_id);
        }
        if project != self.project {
            self.project = project;
            if ids.len() == 1
                && let Some((track, _)) = self.project.find_region(primary)
            {
                self.selection.track = Some(track.id);
            }
            self.sync_plan();
        }
    }

    fn update_region_resize(&mut self, pointer: Pos2) {
        let Some(Drag::RegionResize {
            origin,
            region,
            anchor,
        }) = &self.drag
        else {
            return;
        };
        let (mut project, region, anchor) = (origin.clone(), *region, *anchor);
        let Some((_, original)) = origin.find_region(region) else {
            return;
        };
        let (start, end) = (original.start_tick, original.end_tick());
        let delta = ((pointer.x - anchor.x) / self.view.pixels_per_tick) as i64;
        let wanted = (end as i64 + delta).max(start as i64 + 1) as u64;
        let minimum = if self.project.snap.enabled {
            grid_ticks(self.project.snap.grid, self.project.time_signature)
        } else {
            1
        };
        let new_end = self.snapped(wanted).max(start + minimum);
        edit::resize_region(&mut project, region, new_end - start);
        if project != self.project {
            self.project = project;
            self.sync_plan();
        }
    }

    fn update_loop_drag(&mut self, tick: u64) {
        let Some(Drag::Loop { origin, anchor }) = &self.drag else {
            return;
        };
        let (mut project, anchor) = (origin.clone(), *anchor);
        let grid = grid_ticks(project.snap.grid, project.time_signature);
        let (start, end) = if tick >= anchor {
            (anchor, tick)
        } else {
            (tick, anchor)
        };
        let end = end.max(start + grid);
        edit::set_loop(&mut project, start, end);
        if project != self.project {
            self.project = project;
            self.sync_plan();
        }
    }

    pub(crate) fn finish_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            let origin = drag.into_origin();
            if origin != self.project {
                self.history.record(origin);
            }
        }
    }
}

fn input_label(input: AudioInput) -> String {
    let first = input.first_channel + 1;
    if input.channels > 1 {
        format!("In {first}-{}", first + 1)
    } else {
        format!("In {first}")
    }
}

fn input_options(channels: u16) -> Vec<AudioInput> {
    let mono = (0..channels).map(|first_channel| AudioInput {
        first_channel,
        channels: 1,
    });
    let stereo = (0..channels.saturating_sub(1))
        .step_by(2)
        .map(|first_channel| AudioInput {
            first_channel,
            channels: 2,
        });
    mono.chain(stereo).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_choices_cover_mono_and_stereo_pairs() {
        let labels: Vec<String> = input_options(4).into_iter().map(input_label).collect();
        assert_eq!(labels, ["In 1", "In 2", "In 3", "In 4", "In 1-2", "In 3-4"]);
        assert_eq!(input_options(1).len(), 1);
    }
}
