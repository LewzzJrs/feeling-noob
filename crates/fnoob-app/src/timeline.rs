use crate::app::{App, Drag, Pending};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2,
};
use fnoob_core::edit;
use fnoob_core::io::PROJECT_FILE;
use fnoob_core::model::{
    AudioInput, AutomationParam, Id, Region, RegionContent, Track, TrackKind, value_at,
};
use fnoob_core::time::{grid_ticks, snap_floor, ticks_per_bar};
use std::path::PathBuf;

pub const HEADER_WIDTH: f32 = 330.0;
const PINNED_ROW: f32 = 30.0;
const TAKE_H: f32 = 30.0;

type TakeRegion = (Id, u64, u64, String);
type TakeRows = (usize, Id, u8, Vec<(String, Vec<TakeRegion>)>);
const NAME_WIDTH: f32 = 116.0;
const IO_WIDTH: f32 = 96.0;
pub const LOOP_ROW: f32 = 12.0;
pub const RULER_HEIGHT: f32 = 64.0 + crate::arrange::OVERVIEW_ROW;
const TEMPO_ROW: f32 = 16.0;
const METER_ROW: f32 = 14.0;
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
const PUNCH: Color32 = Color32::from_rgb(220, 70, 70);
const TEMPO: Color32 = Color32::from_rgb(150, 200, 120);
const METER: Color32 = Color32::from_rgb(170, 150, 230);
const LOOP_OFF: Color32 = Color32::from_rgb(80, 80, 86);
const PLAYHEAD: Color32 = Color32::from_rgb(240, 240, 240);
const MARKER: Color32 = Color32::from_rgb(235, 170, 90);

pub fn track_color(index: u8) -> Color32 {
    let base = TRACK_PALETTE[usize::from(index) % TRACK_PALETTE.len()];
    let shade = |f: fn(u8) -> u8| Color32::from_rgb(f(base.r()), f(base.g()), f(base.b()));
    match usize::from(index) / TRACK_PALETTE.len() % 3 {
        1 => shade(|c| (u16::from(c) + (255 - u16::from(c)) * 2 / 5) as u8),
        2 => shade(|c| (u16::from(c) * 3 / 5) as u8),
        _ => base,
    }
}

pub enum UiAction {
    SelectTrack(Id),
    FocusTrackTitle {
        id: Id,
        shift: bool,
        ctrl: bool,
    },
    OpenDevices,
    TitleCommand {
        id: Id,
        command: fnoob_core::keymap::Command,
    },
    ShowDevices,
    ToggleTakes(Id),
    Comp {
        track: Id,
        lane: usize,
        start: u64,
        end: u64,
    },
    DeleteTake {
        track: Id,
        lane: usize,
    },
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
    SendToGrid(Id),
    DragLoopStart {
        tick: u64,
    },
    DragLoopUpdate {
        tick: u64,
    },
    DragEnd,
    ToggleLoop,
    Locate(u64),
    Rename {
        id: Id,
        name: String,
    },
    CancelRename,
    Scroll {
        ticks: f32,
        pixels: f32,
    },
    Zoom {
        factor: f32,
        anchor: f32,
    },
    DropFiles {
        paths: Vec<PathBuf>,
        track: Option<Id>,
        tick: u64,
    },
    TrimRegionStart {
        region: Id,
        pointer: Pos2,
    },
    AddAutomationPoint {
        track: Id,
        tick: u64,
        value: f32,
    },
    AutomationDragStart {
        track: Id,
        index: usize,
    },
    AutomationDragUpdate {
        tick: u64,
        value: f32,
    },
    DeleteAutomationPoint {
        track: Id,
        index: usize,
    },
    SetAutomationParam {
        track: Id,
        param: AutomationParam,
    },
    RegionGain {
        region: Id,
        db: Option<f32>,
    },
    RegionFades {
        region: Id,
        fade_in: Option<u64>,
        fade_out: Option<u64>,
    },
    MuteRegion(Id),
    BrowserDrop {
        item: crate::browser::BrowserItem,
        track: Option<Id>,
        tick: u64,
    },
    MoveTrack {
        id: Id,
        delta: i64,
    },
    SetColor {
        id: Id,
        color: u8,
    },
    DeleteRegion(Id),
    SplitRegion(Id),
    LocateMarker(u64),
    SetInsert(u64),
    TimeDragStart {
        tick: u64,
        index: usize,
    },
    TimeDragUpdate {
        tick: u64,
        index: usize,
    },
    TimeDragEnd,
    ScrollTo(f32),
    ToggleMarkerAt(u64),
    Run(fnoob_core::keymap::Command),
    SetGrid(fnoob_core::model::Grid),
    ToggleFold(Id),
    SelectTrackContent(Id),
    FadePreview(Option<(Id, u64, u64)>),
    SetFades {
        region: Id,
        fade_in: u64,
        fade_out: u64,
    },
    SetTempo {
        tick: u64,
        bpm: f64,
    },
    DeleteTempo(u64),
    AddTempo {
        tick: u64,
        bpm: f64,
    },
    MoveTempo {
        from: u64,
        to: u64,
        bpm: f64,
    },
    SetTempoRamp {
        tick: u64,
        ramp: bool,
    },
    SetMeter {
        bar: u64,
        signature: fnoob_core::model::TimeSignature,
    },
    DeleteMeter(u64),
    RenameMarker(Id),
    DeleteMarker(Id),
}

fn automation_y(lane: Rect, param: AutomationParam, value: f32) -> f32 {
    let (low, high) = param.range();
    let inner = lane.shrink2(Vec2::new(0.0, 6.0));
    inner.bottom() - inner.height() * ((value.max(low) - low) / (high - low)).clamp(0.0, 1.0)
}

fn automation_value(lane: Rect, param: AutomationParam, y: f32) -> f32 {
    let (low, high) = param.range();
    let inner = lane.shrink2(Vec2::new(0.0, 6.0));
    let t = ((inner.bottom() - y) / inner.height()).clamp(0.0, 1.0);
    let value = low + (high - low) * t;
    match param {
        AutomationParam::Volume => (value * 2.0).round() / 2.0,
        AutomationParam::Pan => (value * 100.0).round() / 100.0,
    }
}

fn base_value(track: &Track, param: AutomationParam) -> f32 {
    match param {
        AutomationParam::Volume => track.strip.volume_db.max(-60.0),
        AutomationParam::Pan => track.strip.pan,
    }
}

struct Geometry {
    ruler: Rect,
    lanes: Rect,
    headers: Rect,
    pixels_per_tick: f32,
    scroll_tick: f32,
    scroll_y: f32,
    offsets: Vec<f32>,
}

impl Geometry {
    fn track_at(&self, y: f32) -> Option<usize> {
        let y = y - self.lanes.top() + self.scroll_y;
        self.offsets.windows(2).position(|w| y >= w[0] && y < w[1])
    }

    fn x(&self, tick: u64) -> f32 {
        self.lanes.left() + (tick as f32 - self.scroll_tick) * self.pixels_per_tick
    }

    fn tick(&self, x: f32) -> u64 {
        ((x - self.lanes.left()) / self.pixels_per_tick + self.scroll_tick).max(0.0) as u64
    }

    fn lane_top(&self, index: usize) -> f32 {
        let offset = self
            .offsets
            .get(index)
            .or(self.offsets.last())
            .copied()
            .unwrap_or(0.0);
        self.lanes.top() + offset - self.scroll_y
    }
}

impl App {
    fn snapped(&self, tick: u64) -> u64 {
        if self.project.snap.enabled {
            self.project
                .meter()
                .snap_round(tick, self.project.snap.grid)
        } else {
            tick
        }
    }

    pub fn tracks_area(&mut self, ui: &mut egui::Ui) {
        let (area, area_response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let split = area.right() - HEADER_WIDTH;
        let shown_returns = if self.returns_visible {
            self.project.returns.len()
        } else {
            0
        };
        let pinned_top = area.bottom() - (shown_returns + 1) as f32 * PINNED_ROW;
        let geometry = Geometry {
            ruler: Rect::from_min_max(area.min, Pos2::new(split, area.top() + RULER_HEIGHT)),
            lanes: Rect::from_min_max(
                Pos2::new(area.left(), area.top() + RULER_HEIGHT),
                Pos2::new(split, pinned_top - crate::arrange::TIME_ROW),
            ),
            headers: Rect::from_min_max(
                Pos2::new(split, area.top() + RULER_HEIGHT),
                Pos2::new(area.right(), pinned_top - crate::arrange::TIME_ROW),
            ),
            pixels_per_tick: self.view.pixels_per_tick,
            scroll_tick: self.view.scroll_tick,
            scroll_y: self.view.scroll_y,
            offsets: self.track_offsets(),
        };
        let mut actions = Vec::new();
        if area_response.contains_pointer() {
            let (scroll, zoom, pointer) = ui.input(|i| {
                (
                    i.smooth_scroll_delta,
                    i.zoom_delta(),
                    i.pointer.latest_pos(),
                )
            });
            if zoom != 1.0 {
                let anchor = pointer.map_or(0.0, |p| (p.x - geometry.lanes.left()).max(0.0));
                actions.push(UiAction::Zoom {
                    factor: zoom,
                    anchor,
                });
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
        self.draw_take_lanes(ui, &geometry, &mut actions);
        self.draw_ruler(ui, &geometry, &mut actions);
        self.draw_headers(ui, &geometry, &mut actions);
        self.draw_pinned_rows(ui, area, pinned_top, split);
        self.lanes_width = geometry.lanes.width();
        let time_row = Rect::from_min_max(
            Pos2::new(area.left(), geometry.lanes.bottom()),
            Pos2::new(split, pinned_top),
        );
        self.draw_time_row(ui, &geometry, time_row, &mut actions);
        let corner = Rect::from_min_max(
            Pos2::new(split, geometry.lanes.bottom()),
            Pos2::new(area.right(), pinned_top),
        );
        self.draw_zoom_corner(ui, corner, &mut actions);
        let locators = Rect::from_min_max(
            Pos2::new(split, area.top()),
            Pos2::new(area.right(), geometry.ruler.bottom()),
        );
        self.draw_locators(ui, locators, &mut actions);
        self.draw_time_selection(ui, &geometry);
        let playhead_tick = self.playhead_tick();
        let stopped = self.transport_state() == fnoob_engine::TransportState::Stopped;
        let playhead = geometry.x(playhead_tick);
        if (!stopped || playhead_tick != self.insert_tick)
            && geometry.lanes.x_range().contains(playhead)
        {
            painter.vline(
                playhead,
                geometry.ruler.top() + crate::arrange::OVERVIEW_ROW..=time_row.bottom(),
                Stroke::new(1.5, PLAYHEAD),
            );
        }
        self.draw_insert_marker(ui, &geometry, time_row);
        self.handle_file_drop(ui, &geometry, area, &mut actions);
        let pointer = ui.input(|i| i.pointer.latest_pos());
        if let Some(item) = area_response.dnd_release_payload::<crate::browser::BrowserItem>() {
            let (track, tick) = self.drop_point(&geometry, pointer);
            actions.push(UiAction::BrowserDrop {
                item: (*item).clone(),
                track,
                tick,
            });
        }
        if area_response
            .dnd_hover_payload::<crate::browser::BrowserItem>()
            .is_some()
        {
            self.paint_drop_target(ui, &geometry, area, pointer);
        }
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
            ui.painter_at(area).text(
                Pos2::new(area.center().x, g.lanes.top() + 8.0),
                Align2::CENTER_TOP,
                "Drop on a track to add to it, or below the tracks for a new track",
                FontId::proportional(15.0),
                ui.visuals().strong_text_color(),
            );
            self.paint_drop_target(ui, g, area, pointer);
        }
        let paths: Vec<PathBuf> = dropped.iter().map(|f| f.path().to_path_buf()).collect();
        if paths.is_empty() {
            return;
        }
        let (track, tick) = self.drop_point(g, pointer);
        actions.push(UiAction::DropFiles { paths, track, tick });
    }

    fn track_offsets(&self) -> Vec<f32> {
        let mut offsets = vec![0.0];
        let mut total = 0.0;
        for track in &self.project.tracks {
            if self.project.is_folded_away(track) {
                offsets.push(total);
                continue;
            }
            total += LANE_HEIGHT;
            if self.takes_open.contains(&track.id) {
                total += track.takes.len() as f32 * TAKE_H;
            }
            offsets.push(total);
        }
        offsets
    }

    fn draw_take_lanes(&mut self, ui: &mut egui::Ui, g: &Geometry, actions: &mut Vec<UiAction>) {
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let lanes: Vec<TakeRows> = self
            .project
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                self.takes_open.contains(&t.id)
                    && !t.takes.is_empty()
                    && !self.project.is_folded_away(t)
            })
            .map(|(i, t)| {
                let takes = t
                    .takes
                    .iter()
                    .map(|l| {
                        let regions = l
                            .regions
                            .iter()
                            .map(|r| (r.id, r.start_tick, r.end_tick(), r.name.clone()))
                            .collect();
                        (l.name.clone(), regions)
                    })
                    .collect();
                (i, t.id, t.color, takes)
            })
            .collect();
        let painter = ui.painter_at(g.lanes);
        let header_painter = ui.painter_at(g.headers);
        for (index, track, color, takes) in lanes {
            for (lane, (name, regions)) in takes.into_iter().enumerate() {
                let top = g.lane_top(index) + LANE_HEIGHT + lane as f32 * TAKE_H;
                let row = Rect::from_x_y_ranges(g.lanes.x_range(), top..=top + TAKE_H - 1.0);
                if row.bottom() < g.lanes.top() || row.top() > g.lanes.bottom() {
                    continue;
                }
                painter.rect_filled(row, 0.0, ui.visuals().faint_bg_color.gamma_multiply(0.6));
                for (_, start, end, region_name) in &regions {
                    let rect = Rect::from_x_y_ranges(
                        g.x(*start)..=g.x(*end).max(g.x(*start) + 3.0),
                        row.top() + 3.0..=row.bottom() - 3.0,
                    );
                    painter.rect_filled(
                        rect,
                        CornerRadius::same(2),
                        track_color(color).gamma_multiply(0.55),
                    );
                    painter.text(
                        rect.left_center() + Vec2::new(4.0, 0.0),
                        Align2::LEFT_CENTER,
                        region_name,
                        FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                }
                let response = crate::ui_kit::hint(
                    ui.interact(
                        row.intersect(g.lanes),
                        ui.scope_id().with(("take_lane", track, lane)),
                        Sense::click_and_drag(),
                    ),
                    "Take lane: drag over a part to use it in the track, or click a take to use all of it.",
                );
                if response.drag_started()
                    && let Some(p) = response.interact_pointer_pos()
                {
                    self.comp_drag = Some((track, lane, self.snapped(g.tick(p.x))));
                }
                if let (Some((t, l, start)), Some(p)) = (self.comp_drag, pointer)
                    && t == track
                    && l == lane
                {
                    let now = self.snapped(g.tick(p.x));
                    let range = Rect::from_x_y_ranges(
                        g.x(start.min(now))..=g.x(start.max(now)),
                        row.y_range(),
                    );
                    painter.rect_filled(range, 0.0, Color32::from_white_alpha(40));
                    if response.drag_stopped() {
                        self.comp_drag = None;
                        if now != start {
                            actions.push(UiAction::Comp {
                                track,
                                lane,
                                start: start.min(now),
                                end: start.max(now),
                            });
                        }
                    }
                } else if response.clicked()
                    && let Some(p) = response.interact_pointer_pos()
                {
                    let tick = g.tick(p.x);
                    if let Some((_, start, end, _)) =
                        regions.iter().find(|(_, s, e, _)| *s <= tick && tick < *e)
                    {
                        actions.push(UiAction::Comp {
                            track,
                            lane,
                            start: *start,
                            end: *end,
                        });
                    }
                }
                let header = Rect::from_x_y_ranges(g.headers.x_range(), row.y_range());
                header_painter.rect_filled(header, 0.0, ui.visuals().faint_bg_color);
                header_painter.text(
                    header.left_center() + Vec2::new(12.0, 0.0),
                    Align2::LEFT_CENTER,
                    &name,
                    FontId::proportional(12.0),
                    ui.visuals().text_color(),
                );
                let delete = Rect::from_center_size(
                    Pos2::new(header.right() - 20.0, header.center().y),
                    Vec2::new(22.0, 18.0),
                );
                if ui
                    .put(delete.intersect(g.headers), egui::Button::new("x").small())
                    .on_hover_text("Delete this take lane")
                    .clicked()
                {
                    actions.push(UiAction::DeleteTake { track, lane });
                }
            }
        }
    }

    fn drop_point(&self, g: &Geometry, pointer: Option<Pos2>) -> (Option<Id>, u64) {
        let Some(p) = pointer.filter(|p| {
            (g.lanes.top()..=g.lanes.bottom()).contains(&p.y)
                && (g.lanes.left()..=g.headers.right()).contains(&p.x)
        }) else {
            return (None, self.playhead_tick());
        };
        let track = g
            .track_at(p.y)
            .and_then(|index| self.project.tracks.get(index))
            .map(|t| t.id);
        let tick = if p.x >= g.lanes.left() {
            self.snapped(g.tick(p.x))
        } else {
            self.playhead_tick()
        };
        (track, tick)
    }

    fn paint_drop_target(&self, ui: &egui::Ui, g: &Geometry, area: Rect, pointer: Option<Pos2>) {
        let Some(p) = pointer.filter(|p| area.contains(*p)) else {
            return;
        };
        let (track, tick) = self.drop_point(g, Some(p));
        let index = track
            .and_then(|id| self.project.tracks.iter().position(|t| t.id == id))
            .unwrap_or(self.project.tracks.len());
        let top = g.lane_top(index);
        let rect =
            Rect::from_x_y_ranges(g.lanes.left()..=g.headers.right(), top..=top + LANE_HEIGHT)
                .intersect(Rect::from_x_y_ranges(
                    g.lanes.left()..=g.headers.right(),
                    g.lanes.y_range(),
                ));
        let painter = ui.painter_at(area);
        painter.rect_filled(rect, 0.0, MARKER.gamma_multiply(0.18));
        painter.rect_stroke(rect, 0.0, Stroke::new(2.0, MARKER), StrokeKind::Inside);
        if track.is_none() {
            painter.text(
                rect.left_center() + egui::vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                "New track",
                FontId::proportional(14.0),
                ui.visuals().strong_text_color(),
            );
        }
        let x = g.x(tick);
        if x >= g.lanes.left() {
            painter.vline(x, rect.y_range(), Stroke::new(2.0, MARKER));
        }
    }

    fn draw_ruler(&self, ui: &mut egui::Ui, g: &Geometry, actions: &mut Vec<UiAction>) {
        let painter = ui.painter_at(g.ruler);
        let visuals = ui.visuals();
        painter.rect_filled(g.ruler, 0.0, visuals.faint_bg_color);
        let overview = Rect::from_min_size(
            g.ruler.min,
            Vec2::new(g.ruler.width(), crate::arrange::OVERVIEW_ROW),
        );
        self.draw_overview(ui, g, overview, actions);
        let loop_row =
            Rect::from_min_size(overview.left_bottom(), Vec2::new(g.ruler.width(), LOOP_ROW));
        let bar_row = Rect::from_min_max(
            Pos2::new(g.ruler.left(), loop_row.bottom()),
            Pos2::new(g.ruler.right(), g.ruler.bottom() - TEMPO_ROW - METER_ROW),
        );
        let meter_row = Rect::from_min_max(
            Pos2::new(g.ruler.left(), bar_row.bottom()),
            Pos2::new(g.ruler.right(), g.ruler.bottom() - TEMPO_ROW),
        );
        let tempo_row =
            Rect::from_min_max(Pos2::new(g.ruler.left(), meter_row.bottom()), g.ruler.max);
        let range = self.project.loop_range;
        let loop_rect = Rect::from_x_y_ranges(
            g.x(range.start_tick)..=g.x(range.end_tick),
            loop_row.top() + 2.0..=loop_row.bottom() - 2.0,
        );
        let loop_color = if range.enabled { LOOP_ON } else { LOOP_OFF };
        painter.rect_filled(loop_rect, CornerRadius::same(2), loop_color);
        let punch = self.project.punch;
        for (on, tick) in [
            (punch.punch_in, range.start_tick),
            (punch.punch_out, range.end_tick),
        ] {
            if on {
                painter.vline(g.x(tick), g.ruler.y_range(), Stroke::new(3.0, PUNCH));
            }
        }
        let meter = self.project.meter();
        let bar = ticks_per_bar(self.project.time_signature);
        let bar_px = bar as f32 * g.pixels_per_tick;
        let label_every = ((48.0 / bar_px).ceil() as u64).max(1);
        let first_bar = meter.bar_at(g.tick(g.ruler.left())).0;
        let last_bar = meter.bar_at(g.tick(g.ruler.right())).0 + 1;
        let text_color = visuals.text_color();
        for index in first_bar..=last_bar {
            let x = g.x(meter.bar_start(index));
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
        for marker in &self.project.markers {
            let x = g.x(marker.tick);
            if x < bar_row.left() - 80.0 || x > bar_row.right() {
                continue;
            }
            let flag = Rect::from_min_size(
                Pos2::new(x, bar_row.center().y - 1.0),
                Vec2::new(
                    (marker.name.chars().count() as f32 * 6.5 + 8.0).min(140.0),
                    bar_row.bottom() - bar_row.center().y + 1.0,
                ),
            );
            painter.rect_filled(flag, CornerRadius::same(2), MARKER);
            painter.vline(x, bar_row.y_range(), Stroke::new(1.5, MARKER));
            ui.painter_at(flag.intersect(g.ruler)).text(
                flag.left_center() + Vec2::new(4.0, 0.0),
                Align2::LEFT_CENTER,
                &marker.name,
                FontId::proportional(11.0),
                Color32::BLACK,
            );
            let response = ui
                .interact(
                    flag.intersect(g.ruler),
                    ui.scope_id().with(("marker", marker.id)),
                    Sense::click(),
                )
                .on_hover_text("Click to jump. Right-click to rename or delete.");
            if response.clicked() {
                actions.push(UiAction::LocateMarker(marker.tick));
            }
            response.context_menu(|ui| {
                if ui.button("Rename...").clicked() {
                    actions.push(UiAction::RenameMarker(marker.id));
                    ui.close();
                }
                if ui.button("Delete marker").clicked() {
                    actions.push(UiAction::DeleteMarker(marker.id));
                    ui.close();
                }
            });
        }
        self.draw_meter_row(ui, g, meter_row, actions);
        self.draw_tempo_row(ui, g, tempo_row, actions);
    }

    fn draw_meter_row(
        &self,
        ui: &mut egui::Ui,
        g: &Geometry,
        row: Rect,
        actions: &mut Vec<UiAction>,
    ) {
        let painter = ui.painter_at(row);
        painter.rect_filled(row, 0.0, ui.visuals().faint_bg_color);
        let response = crate::ui_kit::hint(
            ui.interact(row, ui.scope_id().with("meter_row"), Sense::click()),
            "Time signature lane: right-click to insert a time signature change at a bar.",
        );
        response.context_menu(|ui| {
            if let Some(pos) = ui
                .ctx()
                .pointer_interact_pos()
                .or(response.interact_pointer_pos())
            {
                let meter = self.project.meter();
                let (bar, start, ts) = meter.bar_at(g.tick(pos.x));
                let bar = if g.tick(pos.x) - start > ticks_per_bar(ts) / 2 {
                    bar + 1
                } else {
                    bar
                };
                if bar > 0
                    && ui
                        .button(format!("Insert Time Signature Change at Bar {}", bar + 1))
                        .clicked()
                {
                    actions.push(UiAction::SetMeter {
                        bar: bar + 1,
                        signature: meter.signature_at(meter.bar_start(bar)),
                    });
                    ui.close();
                }
            }
        });
        let meter = self.project.meter();
        let mut signatures = vec![(1, self.project.time_signature)];
        signatures.extend(
            self.project
                .meter_changes
                .iter()
                .map(|c| (c.bar, c.signature)),
        );
        for (bar, ts) in signatures {
            let x = g.x(meter.bar_start(bar - 1));
            if x < row.left() - 60.0 || x > row.right() {
                continue;
            }
            let flag = Rect::from_min_size(
                Pos2::new(x - 30.0, row.top() + 1.0),
                Vec2::new(28.0, row.height() - 2.0),
            );
            painter.rect_filled(flag, CornerRadius::same(2), METER.gamma_multiply(0.85));
            painter.text(
                flag.center(),
                Align2::CENTER_CENTER,
                format!("{}/{}", ts.numerator, ts.denominator),
                FontId::proportional(10.0),
                Color32::BLACK,
            );
            if bar == 1 {
                continue;
            }
            let response = ui
                .interact(
                    flag.intersect(row),
                    ui.scope_id().with(("meter", bar)),
                    Sense::click(),
                )
                .on_hover_text("Time signature change. Right-click to edit or delete.");
            response.context_menu(|ui| {
                let mut next = ts;
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut next.numerator).range(1..=16));
                    ui.label("/");
                    for d in [2, 4, 8, 16] {
                        ui.selectable_value(&mut next.denominator, d, d.to_string());
                    }
                });
                if next != ts {
                    actions.push(UiAction::SetMeter {
                        bar,
                        signature: next,
                    });
                }
                if ui.button("Delete Time Signature Change").clicked() {
                    actions.push(UiAction::DeleteMeter(bar));
                    ui.close();
                }
            });
        }
    }

    fn draw_tempo_row(
        &self,
        ui: &mut egui::Ui,
        g: &Geometry,
        row: Rect,
        actions: &mut Vec<UiAction>,
    ) {
        let painter = ui.painter_at(row);
        painter.rect_filled(row, 0.0, ui.visuals().extreme_bg_color);
        let response = crate::ui_kit::hint(
            ui.interact(row, ui.scope_id().with("tempo_row"), Sense::click()),
            "Song tempo: right-click to add a point. Drag a point sideways to move it, up or down to change the tempo.",
        );
        let timeline = self.timeline();
        response.context_menu(|ui| {
            if let Some(pos) = ui
                .ctx()
                .pointer_interact_pos()
                .or(response.interact_pointer_pos())
            {
                let tick = self.snapped(g.tick(pos.x));
                if ui.button("Add Tempo Point Here").clicked() {
                    actions.push(UiAction::AddTempo {
                        tick,
                        bpm: timeline.bpm_at_tick(tick),
                    });
                    ui.close();
                }
            }
        });
        let mut points = vec![(0, self.project.tempo_bpm, false)];
        points.extend(
            self.project
                .tempo_changes
                .iter()
                .map(|c| (c.tick, c.bpm, c.ramp)),
        );
        let mut previous_x = None;
        for (tick, bpm, ramp) in points {
            let x = g.x(tick);
            if ramp && let Some(from) = previous_x {
                painter.line_segment(
                    [
                        Pos2::new(from, row.bottom() - 2.0),
                        Pos2::new(x, row.top() + 2.0),
                    ],
                    Stroke::new(1.5, TEMPO),
                );
            }
            previous_x = Some(x);
            let x = g.x(tick);
            if x < row.left() - 60.0 || x > row.right() {
                continue;
            }
            let flag = Rect::from_min_size(
                Pos2::new(x, row.top() + 1.0),
                Vec2::new(46.0, row.height() - 2.0),
            );
            painter.rect_filled(flag, CornerRadius::same(2), TEMPO.gamma_multiply(0.85));
            painter.text(
                flag.left_center() + Vec2::new(3.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{bpm:.1}"),
                FontId::proportional(10.0),
                Color32::BLACK,
            );
            if tick == 0 {
                continue;
            }
            let flag_response = ui
                .interact(
                    flag.intersect(row),
                    ui.scope_id().with(("tempo", tick)),
                    Sense::click_and_drag(),
                )
                .on_hover_text("Tempo point. Drag to move or change it. Right-click for more.");
            if flag_response.dragged()
                && let (Some(origin), Some(now)) = (
                    ui.input(|i| i.pointer.press_origin()),
                    flag_response.interact_pointer_pos(),
                )
            {
                let new_tick = self.snapped(g.tick(x + (now.x - origin.x))).max(1);
                let new_bpm = (bpm - f64::from(now.y - origin.y) * 0.5)
                    .clamp(fnoob_core::time::MIN_BPM, fnoob_core::time::MAX_BPM);
                let ghost = Rect::from_min_size(
                    Pos2::new(g.x(new_tick), row.top() + 1.0),
                    Vec2::new(46.0, row.height() - 2.0),
                );
                painter.rect_stroke(
                    ghost,
                    CornerRadius::same(2),
                    Stroke::new(1.5, Color32::WHITE),
                    StrokeKind::Inside,
                );
                painter.text(
                    ghost.left_center() + Vec2::new(3.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("{new_bpm:.1}"),
                    FontId::proportional(10.0),
                    Color32::WHITE,
                );
                if flag_response.drag_stopped() {
                    actions.push(UiAction::MoveTempo {
                        from: tick,
                        to: new_tick,
                        bpm: new_bpm,
                    });
                }
            }
            flag_response.context_menu(|ui| {
                let mut value = bpm;
                if ui
                    .add(
                        egui::DragValue::new(&mut value)
                            .range(fnoob_core::time::MIN_BPM..=fnoob_core::time::MAX_BPM)
                            .speed(0.2)
                            .suffix(" bpm"),
                    )
                    .changed()
                {
                    actions.push(UiAction::SetTempo { tick, bpm: value });
                }
                let mut glide = ramp;
                if ui
                    .checkbox(&mut glide, "Ramp from the previous tempo")
                    .changed()
                {
                    actions.push(UiAction::SetTempoRamp { tick, ramp: glide });
                }
                if ui.button("Delete Tempo Change").clicked() {
                    actions.push(UiAction::DeleteTempo(tick));
                    ui.close();
                }
            });
        }
    }

    fn draw_lanes(&self, ui: &mut egui::Ui, g: &Geometry, actions: &mut Vec<UiAction>) {
        let painter = ui.painter_at(g.lanes);
        let visuals = ui.visuals().clone();
        let background = ui.interact(
            g.lanes,
            ui.scope_id().with("lanes_background"),
            Sense::click_and_drag(),
        );
        let last = self.project.tracks.len().saturating_sub(1);
        self.time_drag_actions(&background, g, last, actions);
        background.context_menu(|ui| self.selection_menu(ui, actions));
        if background.clicked()
            && let Some(pos) = background.interact_pointer_pos()
        {
            actions.push(UiAction::ClearRegions);
            actions.push(UiAction::SetInsert(self.snapped(g.tick(pos.x))));
        }
        let meter = self.project.meter();
        let first_bar = meter.bar_at(g.tick(g.lanes.left())).0;
        let last_bar = meter.bar_at(g.tick(g.lanes.right())).0 + 1;
        for index in first_bar..=last_bar {
            painter.vline(
                g.x(meter.bar_start(index)),
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
            if self.project.is_folded_away(track) {
                continue;
            }
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
                Sense::click_and_drag(),
            );
            if !self.automation_view && !self.draw_mode {
                self.time_drag_actions(&lane_response, g, index, actions);
                lane_response.context_menu(|ui| self.selection_menu(ui, actions));
            }
            let automating = self.automation_view;
            if automating {
                if lane_response.clicked()
                    && let Some(pos) = lane_response.interact_pointer_pos()
                {
                    let param = self.automation_param(track.id);
                    actions.push(UiAction::SelectTrack(track.id));
                    actions.push(UiAction::AddAutomationPoint {
                        track: track.id,
                        tick: self.snapped(g.tick(pos.x)),
                        value: automation_value(lane, param, pos.y),
                    });
                }
            } else if (lane_response.double_clicked() || self.draw_mode && lane_response.clicked())
                && track.kind == TrackKind::Instrument
            {
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
                if let Some(pos) = lane_response.interact_pointer_pos() {
                    actions.push(UiAction::SetInsert(self.snapped(g.tick(pos.x))));
                }
            }
            if track.frozen.is_some() {
                painter.rect_filled(
                    lane,
                    0.0,
                    Color32::from_rgba_unmultiplied(120, 180, 255, 22),
                );
            }
            if track.kind == TrackKind::Group {
                let tint = track_color(track.color).gamma_multiply(0.35);
                for region in self
                    .project
                    .group_members(track.id)
                    .flat_map(|t| t.regions.iter())
                {
                    let rect = Rect::from_x_y_ranges(
                        g.x(region.start_tick)
                            ..=g.x(region.end_tick()).max(g.x(region.start_tick) + 2.0),
                        lane.top() + 8.0..=lane.bottom() - 8.0,
                    );
                    if rect.right() >= g.lanes.left() && rect.left() <= g.lanes.right() {
                        painter.rect_filled(rect, CornerRadius::same(2), tint);
                    }
                }
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
                let base = if automating {
                    base.gamma_multiply(0.45)
                } else {
                    base
                };
                painter.rect_filled(rect, CornerRadius::same(4), base.gamma_multiply(0.75));
                let border = if selected {
                    Stroke::new(2.0, Color32::WHITE)
                } else {
                    Stroke::new(1.0, base)
                };
                painter.rect_stroke(rect, CornerRadius::same(4), border, StrokeKind::Inside);
                self.draw_waveform(ui, g, region, rect);
                self.draw_fades(ui, g, region, rect, None);
                if let (Some(repeat), Some(_)) = (region.loop_ticks, region.audio_file()) {
                    let loop_painter = ui.painter_at(rect.intersect(g.lanes));
                    let mut at = region.start_tick + repeat.max(1);
                    while at < region.end_tick() {
                        loop_painter.vline(
                            g.x(at),
                            rect.y_range(),
                            Stroke::new(1.5, Color32::from_black_alpha(140)),
                        );
                        at += repeat.max(1);
                    }
                }
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
                    let repeat = region.loop_ticks.unwrap_or(region.length_tick).max(1);
                    for pass in 0..region.length_tick.div_ceil(repeat) {
                        let offset = region.start_tick + pass * repeat;
                        if pass > 0 {
                            note_painter.vline(
                                g.x(offset),
                                body.y_range(),
                                Stroke::new(1.0, Color32::from_black_alpha(90)),
                            );
                        }
                        let stop = if region.loop_ticks.is_some() {
                            (offset + repeat).min(region.end_tick())
                        } else {
                            region.end_tick()
                        };
                        for note in notes.iter().filter(|n| n.start_tick < repeat) {
                            let start = offset + note.start_tick;
                            if start >= stop {
                                continue;
                            }
                            let end = (start + note.length_tick).min(stop);
                            let y =
                                body.bottom() - body.height() * f32::from(note.pitch - low) / span;
                            note_painter.hline(
                                g.x(start)..=g.x(end).max(g.x(start) + 1.0),
                                y,
                                Stroke::new(2.0, Color32::from_black_alpha(170)),
                            );
                        }
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
                if automating {
                    continue;
                }
                let response = ui.interact(
                    rect.intersect(g.lanes),
                    ui.scope_id().with(("region", region.id)),
                    Sense::click_and_drag(),
                );
                self.draw_fades(ui, g, region, rect, Some(actions));
                let near_edge =
                    |pos: Pos2| pos.x >= rect.right() - EDGE && rect.width() > 3.0 * EDGE;
                let near_start =
                    |pos: Pos2| pos.x <= rect.left() + EDGE && rect.width() > 3.0 * EDGE;
                let response = if response
                    .hover_pos()
                    .is_some_and(|p| near_edge(p) || near_start(p))
                {
                    response.on_hover_cursor(CursorIcon::ResizeHorizontal)
                } else {
                    response
                };
                self.region_menu(&response, region, actions);
                if response.drag_started() {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if near_edge(pointer) {
                            actions.push(UiAction::ResizeRegionStart {
                                region: region.id,
                                pointer,
                            });
                        } else if near_start(pointer) {
                            actions.push(UiAction::TrimRegionStart {
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
                    if let Some(pos) = response.interact_pointer_pos() {
                        actions.push(UiAction::SetInsert(self.snapped(g.tick(pos.x))));
                    }
                }
                if response.drag_stopped() {
                    actions.push(UiAction::DragEnd);
                }
            }
            if automating {
                self.draw_automation(ui, g, track, lane, actions);
            }
        }
    }

    fn draw_automation(
        &self,
        ui: &mut egui::Ui,
        g: &Geometry,
        track: &Track,
        lane: Rect,
        actions: &mut Vec<UiAction>,
    ) {
        let param = self.automation_param(track.id);
        let points = track.automation.lane(param);
        let painter = ui.painter_at(lane.intersect(g.lanes));
        let color = Color32::from_rgb(250, 210, 90);
        let default = base_value(track, param);
        if points.is_empty() {
            let y = automation_y(lane, param, default);
            painter.hline(
                lane.x_range(),
                y,
                Stroke::new(1.0, color.gamma_multiply(0.5)),
            );
        } else {
            let first = g.tick(g.lanes.left());
            let last = g.tick(g.lanes.right());
            let mut line = vec![Pos2::new(
                g.lanes.left(),
                automation_y(lane, param, value_at(points, first, default)),
            )];
            line.extend(
                points
                    .iter()
                    .filter(|p| p.tick > first && p.tick < last)
                    .map(|p| Pos2::new(g.x(p.tick), automation_y(lane, param, p.value))),
            );
            line.push(Pos2::new(
                g.lanes.right(),
                automation_y(lane, param, value_at(points, last, default)),
            ));
            painter.add(egui::Shape::line(line, Stroke::new(2.0, color)));
        }
        painter.text(
            lane.left_top() + Vec2::new(6.0, 4.0),
            Align2::LEFT_TOP,
            param.label(),
            FontId::proportional(11.0),
            color,
        );
        for (index, point) in points.iter().enumerate() {
            let center = Pos2::new(g.x(point.tick), automation_y(lane, param, point.value));
            if !g.lanes.contains(center) {
                continue;
            }
            painter.circle_filled(center, 4.5, color);
            let hit = Rect::from_center_size(center, Vec2::splat(12.0));
            let response = ui
                .interact(
                    hit,
                    ui.scope_id()
                        .with(("point", track.id, param.label(), index)),
                    Sense::click_and_drag(),
                )
                .on_hover_text(match param {
                    AutomationParam::Volume => format!("{:+.1} dB", point.value),
                    AutomationParam::Pan => format!("Pan {:+.2}", point.value),
                });
            if response.drag_started() {
                actions.push(UiAction::AutomationDragStart {
                    track: track.id,
                    index,
                });
            } else if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
            {
                actions.push(UiAction::AutomationDragUpdate {
                    tick: self.snapped(g.tick(pos.x)),
                    value: automation_value(lane, param, pos.y),
                });
            } else if response.secondary_clicked() {
                actions.push(UiAction::DeleteAutomationPoint {
                    track: track.id,
                    index,
                });
            }
            if response.drag_stopped() {
                actions.push(UiAction::DragEnd);
            }
        }
    }

    fn draw_fades(
        &self,
        ui: &egui::Ui,
        g: &Geometry,
        region: &Region,
        rect: Rect,
        actions: Option<&mut Vec<UiAction>>,
    ) {
        if region.audio_file().is_none() {
            return;
        }
        let interactive = actions.is_some();
        let mut actions = actions;
        let (fade_in, fade_out) = match self.fade_preview {
            Some((id, i, o)) if id == region.id => (i, o),
            _ => (region.fade_in_tick, region.fade_out_tick),
        };
        let painter = ui.painter_at(rect.intersect(g.lanes));
        let shade = Color32::from_black_alpha(110);
        for (is_in, value) in [(true, fade_in), (false, fade_out)] {
            let x = if is_in {
                g.x(region.start_tick + value)
            } else {
                g.x(region.end_tick() - value)
            };
            let handle = Rect::from_center_size(Pos2::new(x, rect.top() + 6.0), Vec2::splat(9.0));
            if !handle.intersects(g.lanes) {
                continue;
            }
            if !interactive {
                painter.rect_filled(handle, 1.0, Color32::from_white_alpha(200));
                continue;
            }
            let response = crate::ui_kit::hint(
                ui.interact(
                    handle,
                    ui.scope_id().with(("fade", region.id, is_in)),
                    Sense::drag(),
                )
                .on_hover_cursor(CursorIcon::ResizeHorizontal),
                "Fade handle: drag to set the fade length.",
            );
            if (response.dragged() || response.drag_stopped())
                && let Some(pos) = response.interact_pointer_pos()
            {
                let tick = g.tick(pos.x).clamp(region.start_tick, region.end_tick());
                let (mut new_in, mut new_out) = (fade_in, fade_out);
                if is_in {
                    new_in = (tick - region.start_tick).min(region.length_tick - fade_out);
                } else {
                    new_out = (region.end_tick() - tick).min(region.length_tick - fade_in);
                }
                let Some(actions) = actions.as_mut() else {
                    continue;
                };
                actions.push(if response.drag_stopped() {
                    UiAction::SetFades {
                        region: region.id,
                        fade_in: new_in,
                        fade_out: new_out,
                    }
                } else {
                    UiAction::FadePreview(Some((region.id, new_in, new_out)))
                });
            }
        }
        if interactive {
            return;
        }
        if fade_in > 0 {
            let x = g.x(region.start_tick + fade_in);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    rect.left_top(),
                    Pos2::new(x, rect.top()),
                    rect.left_bottom(),
                ],
                shade,
                Stroke::NONE,
            ));
        }
        if fade_out > 0 {
            let x = g.x(region.end_tick() - fade_out);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x, rect.top()),
                    rect.right_top(),
                    rect.right_bottom(),
                ],
                shade,
                Stroke::NONE,
            ));
        }
    }

    fn region_menu(&self, response: &egui::Response, region: &Region, actions: &mut Vec<UiAction>) {
        let ts = self.project.time_signature;
        let lengths = [
            ("None", 0),
            ("1/16", grid_ticks(fnoob_core::model::Grid::Sixteenth, ts)),
            ("1 beat", grid_ticks(fnoob_core::model::Grid::Beat, ts)),
            ("1 bar", ticks_per_bar(ts)),
        ];
        response.context_menu(|ui| {
            ui.label(egui::RichText::new(&region.name).strong());
            if let RegionContent::Audio { gain_db, .. } = region.content {
                ui.horizontal(|ui| {
                    ui.label(format!("Gain {gain_db:+.1} dB"));
                    for (label, step) in [("-3", -3.0), ("+3", 3.0)] {
                        if ui.button(label).clicked() {
                            actions.push(UiAction::RegionGain {
                                region: region.id,
                                db: Some(gain_db + step),
                            });
                        }
                    }
                    if ui.button("0").clicked() {
                        actions.push(UiAction::RegionGain {
                            region: region.id,
                            db: None,
                        });
                    }
                });
                ui.menu_button("Fade in", |ui| {
                    for (label, ticks) in lengths {
                        if ui.button(label).clicked() {
                            actions.push(UiAction::RegionFades {
                                region: region.id,
                                fade_in: Some(ticks),
                                fade_out: None,
                            });
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Fade out", |ui| {
                    for (label, ticks) in lengths {
                        if ui.button(label).clicked() {
                            actions.push(UiAction::RegionFades {
                                region: region.id,
                                fade_in: None,
                                fade_out: Some(ticks),
                            });
                            ui.close();
                        }
                    }
                });
                ui.separator();
            } else if ui.button("Open in Piano Roll").clicked() {
                actions.push(UiAction::OpenEditor(region.id));
                ui.close();
            }
            if ui
                .button(if region.muted { "Unmute" } else { "Mute" })
                .clicked()
            {
                actions.push(UiAction::MuteRegion(region.id));
                ui.close();
            }
            if ui.button("Split at playhead").clicked() {
                actions.push(UiAction::SplitRegion(region.id));
                ui.close();
            }
            if ui.button("Copy to Clip Grid").clicked() {
                actions.push(UiAction::SendToGrid(region.id));
                ui.close();
            }
            if ui.button("Delete").clicked() {
                actions.push(UiAction::DeleteRegion(region.id));
                ui.close();
            }
        });
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
        let rows: Vec<(usize, Id, String, u8, bool)> = self
            .project
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| !self.project.is_folded_away(t))
            .map(|(i, t)| (i, t.id, t.name.clone(), t.color, t.mute))
            .collect();
        for (index, id, name, color, muted) in rows {
            let (is_group, folded, member_of) =
                self.project.track(id).map_or((false, false, None), |t| {
                    (t.kind == TrackKind::Group, t.folded, t.group)
                });
            let top = g.lane_top(index);
            let row = Rect::from_x_y_ranges(g.headers.x_range(), top..=top + LANE_HEIGHT);
            if row.bottom() < g.headers.top() || row.top() > g.headers.bottom() {
                continue;
            }
            let selected = self.is_title_selected(id);
            let fill = if selected {
                ui.visuals().selection.bg_fill.gamma_multiply(0.35)
            } else {
                ui.visuals().faint_bg_color
            };
            painter.rect_filled(row.shrink2(Vec2::new(0.0, 1.0)), 0.0, fill);
            let indent = if is_group {
                18.0
            } else if member_of.is_some() {
                10.0
            } else {
                0.0
            };
            let name_rect = Rect::from_min_size(
                row.min + Vec2::new(2.0 + indent, 2.0),
                Vec2::new(NAME_WIDTH - indent, LANE_HEIGHT - 4.0),
            );
            if let Some(group_color) = member_of
                .and_then(|g| self.project.track(g))
                .map(|g| track_color(g.color))
            {
                painter.rect_filled(
                    Rect::from_min_size(
                        row.min + Vec2::new(2.0, 2.0),
                        Vec2::new(6.0, LANE_HEIGHT - 4.0),
                    ),
                    CornerRadius::same(1),
                    group_color,
                );
            }
            if is_group {
                let fold_rect =
                    Rect::from_min_size(row.min + Vec2::new(2.0, 2.0), Vec2::new(16.0, 20.0));
                let fold = ui
                    .put(
                        fold_rect.intersect(g.headers),
                        egui::Button::new(if folded { "▸" } else { "▾" }).frame(false),
                    )
                    .on_hover_text(if folded {
                        "Unfold the group to show its tracks"
                    } else {
                        "Fold the group to hide its tracks"
                    });
                if fold.clicked() {
                    actions.push(UiAction::ToggleFold(id));
                }
            }
            let block = track_color(color);
            painter.rect_filled(
                name_rect,
                CornerRadius::same(2),
                if muted {
                    block.gamma_multiply(0.35)
                } else {
                    block
                },
            );
            if selected {
                painter.rect_stroke(
                    name_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.5, Color32::WHITE),
                    StrokeKind::Inside,
                );
            }
            let name_response = ui.interact(
                name_rect.intersect(g.headers),
                ui.scope_id().with(("header", id)),
                Sense::click(),
            );
            let name_response = crate::ui_kit::hint(
                name_response,
                "Track title: click to select, double-click to open its devices. Ctrl+R renames, right-click for more.",
            );
            match &mut renaming {
                Some((rid, text)) if *rid == id => {
                    let field = Rect::from_min_size(
                        name_rect.left_top() + Vec2::new(4.0, 2.0),
                        Vec2::new(name_rect.width() - 8.0, 20.0),
                    )
                    .intersect(g.headers);
                    match crate::ui_kit::rename_field(ui, field, text) {
                        Some(false) => actions.push(UiAction::CancelRename),
                        Some(true) => actions.push(UiAction::Rename {
                            id,
                            name: text.clone(),
                        }),
                        None => {}
                    }
                }
                _ => {
                    ui.painter_at(name_rect.intersect(g.headers)).text(
                        name_rect.left_top() + Vec2::new(6.0, 4.0),
                        Align2::LEFT_TOP,
                        &name,
                        FontId::proportional(13.0),
                        Color32::BLACK,
                    );
                }
            }
            if self.project.track(id).is_some_and(|t| t.frozen.is_some()) {
                ui.painter_at(name_rect.intersect(g.headers)).text(
                    name_rect.left_bottom() + Vec2::new(6.0, -4.0),
                    Align2::LEFT_BOTTOM,
                    "Frozen",
                    FontId::proportional(11.0),
                    Color32::from_rgb(20, 60, 120),
                );
            }
            let takes = self.project.track(id).map_or(0, |t| t.takes.len());
            if takes > 0 {
                let open = self.takes_open.contains(&id);
                let toggle = Rect::from_min_size(
                    Pos2::new(name_rect.left() + 4.0, name_rect.bottom() - 18.0),
                    Vec2::new(NAME_WIDTH - 8.0, 16.0),
                );
                let label = format!("{} {takes} takes", if open { "▾" } else { "▸" });
                if ui
                    .put(
                        toggle.intersect(g.headers),
                        egui::Button::new(egui::RichText::new(label).small()),
                    )
                    .on_hover_text("Show or hide the take lanes of this track")
                    .clicked()
                {
                    actions.push(UiAction::ToggleTakes(id));
                }
            }
            if name_response.double_clicked() {
                actions.push(UiAction::OpenDevices);
            } else if name_response.clicked() {
                let modifiers = ui.input(|i| i.modifiers);
                actions.push(UiAction::FocusTrackTitle {
                    id,
                    shift: modifiers.shift,
                    ctrl: modifiers.ctrl,
                });
                actions.push(UiAction::ShowDevices);
            }
            name_response.context_menu(|ui| {
                if let Some(command) = title_menu(ui) {
                    actions.push(UiAction::TitleCommand { id, command });
                }
                ui.separator();
                use fnoob_core::keymap::{Command, shortcut_label};
                for (text, command) in [
                    ("Insert Audio Track", Command::NewAudioTrack),
                    ("Insert MIDI Track", Command::NewMidiTrack),
                    ("Insert Return Track", Command::NewReturnTrack),
                ] {
                    let mut button = egui::Button::new(text);
                    if let Some(label) = shortcut_label(command) {
                        button = button.shortcut_text(label);
                    }
                    if ui.add(button).clicked() {
                        actions.push(UiAction::TitleCommand { id, command });
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Select Track Content").clicked() {
                    actions.push(UiAction::SelectTrackContent(id));
                    ui.close();
                }
                let mut takes = egui::Button::new("Show Take Lanes");
                if let Some(label) = shortcut_label(Command::ShowTakeLanes) {
                    takes = takes.shortcut_text(label);
                }
                if ui.add(takes).clicked() {
                    actions.push(UiAction::ToggleTakes(id));
                    ui.close();
                }
                ui.separator();
                let current = self.project.track(id).map_or(0, |t| t.color);
                for row in 0..fnoob_core::model::PALETTE_COLORS / 8 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for column in 0..8u8 {
                            let color = row * 8 + column;
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
                            ui.painter().rect_filled(rect, 2.0, track_color(color));
                            if color == current {
                                ui.painter().rect_stroke(
                                    rect,
                                    2.0,
                                    Stroke::new(1.5, Color32::WHITE),
                                    StrokeKind::Inside,
                                );
                            }
                            if response.clicked() {
                                actions.push(UiAction::SetColor { id, color });
                                ui.close();
                            }
                        }
                    });
                }
                ui.separator();
                if ui.button("Move up").clicked() {
                    actions.push(UiAction::MoveTrack { id, delta: -1 });
                    ui.close();
                }
                if ui.button("Move down").clicked() {
                    actions.push(UiAction::MoveTrack { id, delta: 1 });
                    ui.close();
                }
            });
            let io_rect = Rect::from_min_size(
                Pos2::new(name_rect.right() + 4.0, row.top() + 3.0),
                Vec2::new(IO_WIDTH, LANE_HEIGHT - 6.0),
            );
            let mix_rect = Rect::from_min_max(
                Pos2::new(io_rect.right() + 4.0, row.top() + 3.0),
                Pos2::new(row.right() - 12.0, row.bottom() - 3.0),
            );
            let mut io = ui.new_child(UiBuilder::new().max_rect(io_rect));
            io.set_clip_rect(io_rect.intersect(g.headers));
            io.vertical(|ui| {
                crate::ui_kit::compact(ui);
                if self.automation_view {
                    let current = self.automation_param(id);
                    egui::ComboBox::from_id_salt(("automation", id))
                        .width(80.0)
                        .selected_text(current.label())
                        .show_ui(ui, |ui| {
                            for param in AutomationParam::ALL {
                                if ui
                                    .selectable_label(param == current, param.label())
                                    .clicked()
                                {
                                    actions.push(UiAction::SetAutomationParam { track: id, param });
                                }
                            }
                        });
                }
                self.track_io(ui, id);
            });
            let mut mix = ui.new_child(UiBuilder::new().max_rect(mix_rect));
            mix.set_clip_rect(mix_rect.intersect(g.headers));
            mix.vertical(|ui| {
                crate::ui_kit::compact(ui);
                self.mix_buttons(ui, crate::mixer::MixTarget::Track(id));
                self.mix_levels(ui, crate::mixer::MixTarget::Track(id));
                self.mix_send(ui, crate::mixer::MixTarget::Track(id));
            });
            let meter = Rect::from_min_max(
                Pos2::new(row.right() - 9.0, row.top() + 4.0),
                Pos2::new(row.right() - 4.0, row.bottom() - 4.0),
            );
            self.paint_meter(&painter, meter, crate::mixer::MixTarget::Track(id));
        }
        if renaming.is_some()
            && !actions
                .iter()
                .any(|a| matches!(a, UiAction::Rename { .. } | UiAction::CancelRename))
        {
            self.renaming = renaming;
        }
    }

    fn draw_pinned_rows(&mut self, ui: &mut egui::Ui, area: Rect, top: f32, split: f32) {
        let painter = ui.painter_at(area);
        let mut rows: Vec<(crate::mixer::MixTarget, String, Color32)> = self
            .project
            .returns
            .iter()
            .take(if self.returns_visible { usize::MAX } else { 0 })
            .enumerate()
            .map(|(i, r)| {
                (
                    crate::mixer::MixTarget::Return(i),
                    r.name.clone(),
                    Color32::from_gray(150),
                )
            })
            .collect();
        rows.push((
            crate::mixer::MixTarget::Master,
            "Main".to_owned(),
            Color32::from_gray(205),
        ));
        let mut remove = None;
        let mut add = false;
        for (index, (target, name, color)) in rows.into_iter().enumerate() {
            let y = top + index as f32 * PINNED_ROW;
            let lane =
                Rect::from_min_max(Pos2::new(area.left(), y), Pos2::new(split, y + PINNED_ROW));
            let header =
                Rect::from_min_max(Pos2::new(split, y), Pos2::new(area.right(), y + PINNED_ROW));
            painter.rect_filled(lane, 0.0, ui.visuals().faint_bg_color);
            painter.rect_filled(header, 0.0, ui.visuals().panel_fill);
            painter.hline(
                area.x_range(),
                y,
                Stroke::new(1.0, ui.visuals().window_stroke.color),
            );
            let name_rect = Rect::from_min_size(
                header.min + Vec2::new(2.0, 3.0),
                Vec2::new(NAME_WIDTH, PINNED_ROW - 6.0),
            );
            painter.rect_filled(name_rect, CornerRadius::same(2), color);
            let response = crate::ui_kit::hint(
                ui.interact(
                    name_rect,
                    ui.scope_id().with(("pinned", index)),
                    Sense::click(),
                ),
                "Return track: click to show its devices, right-click to add or delete return tracks.",
            );
            if let crate::mixer::MixTarget::Return(i) = target {
                if response.clicked() {
                    self.chain_target = crate::chain::ChainTarget::Return(i);
                    self.show_devices();
                }
                response.context_menu(|ui| {
                    if ui.button("Insert Return Track").clicked() {
                        add = true;
                        ui.close();
                    }
                    if ui.button("Delete Return Track").clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
            } else {
                response.context_menu(|ui| {
                    if ui.button("Insert Return Track").clicked() {
                        add = true;
                        ui.close();
                    }
                });
            }
            painter.text(
                name_rect.left_center() + Vec2::new(6.0, 0.0),
                Align2::LEFT_CENTER,
                &name,
                FontId::proportional(12.0),
                Color32::BLACK,
            );
            let controls = Rect::from_min_max(
                Pos2::new(name_rect.right() + 4.0 + IO_WIDTH + 4.0, y + 4.0),
                Pos2::new(header.right() - 12.0, y + PINNED_ROW - 2.0),
            );
            let mut child = ui.new_child(UiBuilder::new().max_rect(controls));
            child.horizontal(|ui| {
                crate::ui_kit::compact(ui);
                self.mix_buttons(ui, target);
                self.mix_levels(ui, target);
            });
            let meter = Rect::from_min_max(
                Pos2::new(header.right() - 9.0, y + 4.0),
                Pos2::new(header.right() - 4.0, y + PINNED_ROW - 4.0),
            );
            self.paint_meter(&painter, meter, target);
        }
        if add {
            self.run(fnoob_core::keymap::Command::NewReturnTrack);
        }
        if let Some(index) = remove {
            self.capture_plugin_states();
            self.edit(|p| {
                edit::delete_return(p, index);
            });
            self.chain_target = crate::chain::ChainTarget::SelectedTrack;
        }
    }

    pub fn apply_ui_action(&mut self, action: UiAction) {
        match action {
            UiAction::SelectTrack(id) => {
                self.selection.track = Some(id);
                self.header_focus = false;
            }
            UiAction::FocusTrackTitle { id, shift, ctrl } => self.focus_title(id, shift, ctrl),
            UiAction::ToggleTakes(track) => {
                if !self.takes_open.remove(&track) {
                    self.takes_open.insert(track);
                }
            }
            UiAction::Comp {
                track,
                lane,
                start,
                end,
            } => {
                let mut created = Vec::new();
                self.edit(|p| created = fnoob_core::takes::comp(p, track, lane, start, end));
                if !created.is_empty() {
                    self.selection.track = Some(track);
                    self.selection.regions = created;
                }
            }
            UiAction::DeleteTake { track, lane } => {
                self.edit(|p| {
                    fnoob_core::takes::delete_take_lane(p, track, lane);
                });
            }
            UiAction::TitleCommand { id, command } => self.title_command(id, command),
            UiAction::OpenDevices => {
                self.chain_target = crate::chain::ChainTarget::SelectedTrack;
                self.show_devices();
            }
            UiAction::ShowDevices => {
                if self.detail_visible() {
                    self.chain_target = crate::chain::ChainTarget::SelectedTrack;
                    self.show_devices();
                }
            }
            UiAction::SelectRegion { id, toggle } => {
                self.header_focus = false;
                self.time_selection = None;
                if toggle {
                    if let Some(i) = self.selection.regions.iter().position(|&r| r == id) {
                        self.selection.regions.remove(i);
                    } else {
                        self.selection.regions.push(id);
                    }
                } else {
                    self.selection.regions = vec![id];
                }
                if self.detail_visible() {
                    self.show_clip();
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
            UiAction::TrimRegionStart { region, pointer } => {
                self.selection.regions = vec![region];
                self.drag = Some(Drag::RegionTrim {
                    origin: self.project.clone(),
                    region,
                    anchor: pointer,
                });
            }
            UiAction::AddAutomationPoint { track, tick, value } => {
                let param = self.automation_param(track);
                self.edit(|p| {
                    edit::add_automation_point(p, track, param, tick, value);
                });
            }
            UiAction::AutomationDragStart { track, index } => {
                let param = self.automation_param(track);
                self.drag = Some(Drag::Automation {
                    origin: self.project.clone(),
                    track,
                    param,
                    index,
                });
            }
            UiAction::AutomationDragUpdate { tick, value } => {
                if let Some(Drag::Automation {
                    origin,
                    track,
                    param,
                    index,
                }) = &self.drag
                {
                    let mut project = origin.clone();
                    edit::move_automation_point(&mut project, *track, *param, *index, tick, value);
                    if project != self.project {
                        self.project = project;
                        self.sync_plan();
                    }
                }
            }
            UiAction::DeleteAutomationPoint { track, index } => {
                let param = self.automation_param(track);
                self.edit(|p| {
                    edit::delete_automation_point(p, track, param, index);
                });
            }
            UiAction::SetAutomationParam { track, param } => {
                self.automation_params.insert(track, param);
            }
            UiAction::RegionGain { region, db } => {
                self.edit(|p| {
                    edit::set_region_gain(p, region, db.unwrap_or(0.0));
                });
            }
            UiAction::RegionFades {
                region,
                fade_in,
                fade_out,
            } => {
                if let Some((_, r)) = self.project.find_region(region) {
                    let fade_in = fade_in.unwrap_or(r.fade_in_tick);
                    let fade_out = fade_out.unwrap_or(r.fade_out_tick);
                    self.edit(|p| {
                        edit::set_region_fades(p, region, fade_in, fade_out);
                    });
                }
            }
            UiAction::BrowserDrop { item, track, tick } => {
                self.apply_browser_item(&item, track, tick);
            }
            UiAction::MoveTrack { id, delta } => {
                self.edit(|p| {
                    edit::move_track(p, id, delta);
                });
            }
            UiAction::SetColor { id, color } => self.edit_track(id, |t| t.color = color),
            UiAction::MuteRegion(region) => {
                self.edit(|p| edit::toggle_regions_muted(p, &[region]));
            }
            UiAction::DeleteRegion(region) => {
                self.edit(|p| {
                    edit::delete_regions(p, &[region]);
                });
                self.selection.regions.retain(|r| *r != region);
            }
            UiAction::SplitRegion(region) => {
                let tick = self.playhead_tick();
                self.edit(|p| {
                    edit::split_regions_at(p, &[region], tick);
                });
            }
            UiAction::LocateMarker(tick) => self.locate(tick),
            UiAction::SetInsert(tick) => {
                self.locate(tick);
                self.insert_tick = tick;
                self.time_selection = None;
            }
            UiAction::TimeDragStart { tick, index } => {
                self.time_drag = Some((tick, index));
                self.time_selection = None;
                self.selection.regions.clear();
            }
            UiAction::TimeDragUpdate { tick, index } => self.update_time_selection(tick, index),
            UiAction::TimeDragEnd => {
                self.time_drag = None;
                if let Some(start) = self.time_selection.as_ref().map(|s| s.start) {
                    if self.transport_state() == fnoob_engine::TransportState::Stopped {
                        self.locate(start);
                    }
                    self.insert_tick = start;
                }
            }
            UiAction::ScrollTo(tick) => self.view.scroll_tick = tick.max(0.0),
            UiAction::ToggleMarkerAt(tick) => {
                self.edit(
                    |p| match p.markers.iter().find(|m| m.tick == tick).map(|m| m.id) {
                        Some(id) => {
                            edit::delete_marker(p, id);
                        }
                        None => {
                            edit::add_marker(p, tick);
                        }
                    },
                )
            }
            UiAction::Run(command) => self.run(command),
            UiAction::SetGrid(grid) => self.edit(|p| p.snap.grid = grid),
            UiAction::ToggleFold(id) => self.edit_track(id, |t| t.folded = !t.folded),
            UiAction::FadePreview(preview) => self.fade_preview = preview,
            UiAction::SetFades {
                region,
                fade_in,
                fade_out,
            } => {
                self.fade_preview = None;
                self.edit(|p| {
                    edit::set_region_fades(p, region, fade_in, fade_out);
                });
            }
            UiAction::SelectTrackContent(id) => {
                self.selection.track = Some(id);
                self.header_focus = false;
                self.time_selection = None;
                self.selection.regions = self
                    .project
                    .track(id)
                    .map(|t| t.regions.iter().map(|r| r.id).collect())
                    .unwrap_or_default();
            }
            UiAction::SetTempo { tick, bpm } => {
                let mut added = false;
                self.edit(|p| added = edit::set_tempo_change(p, tick, bpm));
                if !added {
                    self.message = Some("A song can have at most 64 tempo changes.".into());
                }
            }
            UiAction::DeleteTempo(tick) => self.edit(|p| {
                edit::delete_tempo_change(p, tick);
            }),
            UiAction::AddTempo { tick, bpm } => {
                let mut added = false;
                self.edit(|p| {
                    added = edit::set_tempo_change(p, tick, bpm);
                    edit::set_tempo_ramp(p, tick, true);
                });
                if !added {
                    self.message = Some("A song can have at most 64 tempo changes.".into());
                }
            }
            UiAction::MoveTempo { from, to, bpm } => self.edit(|p| {
                let ramp = p
                    .tempo_changes
                    .iter()
                    .find(|c| c.tick == from)
                    .is_some_and(|c| c.ramp);
                edit::delete_tempo_change(p, from);
                edit::set_tempo_change(p, to, bpm);
                edit::set_tempo_ramp(p, to, ramp);
            }),
            UiAction::SetTempoRamp { tick, ramp } => self.edit(|p| {
                edit::set_tempo_ramp(p, tick, ramp);
            }),
            UiAction::SetMeter { bar, signature } => {
                let mut added = false;
                self.edit(|p| added = edit::set_meter_change(p, bar, signature));
                if !added {
                    self.message =
                        Some("A song can have at most 64 time signature changes.".into());
                }
            }
            UiAction::DeleteMeter(bar) => self.edit(|p| {
                edit::delete_meter_change(p, bar);
            }),
            UiAction::RenameMarker(id) => {
                if let Some(marker) = self.project.markers.iter().find(|m| m.id == id) {
                    self.marker_rename = Some((id, marker.name.clone()));
                }
            }
            UiAction::DeleteMarker(id) => {
                self.edit(|p| {
                    edit::delete_marker(p, id);
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
                Some(Drag::RegionTrim { .. }) => self.update_region_trim(pointer),
                _ => self.update_region_drag(pointer),
            },
            UiAction::SendToGrid(region) => self.send_region_to_grid(region),
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
            UiAction::Rename { id, name } => {
                let name = name.trim().to_owned();
                if !name.is_empty() {
                    self.edit_track(id, |t| t.name = name);
                }
            }
            UiAction::CancelRename => {}
            UiAction::Scroll { ticks, pixels } => {
                self.view.scroll_tick = (self.view.scroll_tick + ticks).max(0.0);
                let content = self.track_offsets().last().copied().unwrap_or(0.0);
                self.view.scroll_y =
                    (self.view.scroll_y + pixels).clamp(0.0, (content - LANE_HEIGHT).max(0.0));
            }
            UiAction::Zoom { factor, anchor } => self.zoom_at(factor, anchor),
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
                    .partition(|p| fnoob_engine::decode::is_audio_file(p));
                if wavs.is_empty() {
                    self.message = Some(format!(
                        "Only audio files (WAV, MP3, FLAC, OGG, AIFF, M4A) and project folders can be dropped ({} other files skipped).",
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

    pub(crate) fn zoom_to(&mut self, start: u64, end: u64) {
        self.zoom_history.push(crate::arrange::ZoomState {
            pixels_per_tick: self.view.pixels_per_tick,
            scroll_tick: self.view.scroll_tick,
        });
        let state = crate::arrange::fit(
            start,
            end,
            self.lanes_width,
            MIN_PIXELS_PER_TICK,
            MAX_PIXELS_PER_TICK,
        );
        self.view.pixels_per_tick = state.pixels_per_tick;
        self.view.scroll_tick = state.scroll_tick;
    }

    pub(crate) fn zoom_to_selection(&mut self) {
        let range = match &self.time_selection {
            Some(selection) => Some((selection.start, selection.end)),
            None => {
                let regions: Vec<(u64, u64)> = self
                    .project
                    .tracks
                    .iter()
                    .flat_map(|t| t.regions.iter())
                    .filter(|r| self.selection.regions.contains(&r.id))
                    .map(|r| (r.start_tick, r.end_tick()))
                    .collect();
                let start = regions.iter().map(|r| r.0).min();
                let end = regions.iter().map(|r| r.1).max();
                start.zip(end)
            }
        };
        match range {
            Some((start, end)) if end > start => self.zoom_to(start, end),
            _ => self.message = Some("Select a time range or a clip to zoom to it.".into()),
        }
    }

    fn time_drag_actions(
        &self,
        response: &egui::Response,
        g: &Geometry,
        index: usize,
        actions: &mut Vec<UiAction>,
    ) {
        if self.project.tracks.is_empty() {
            return;
        }
        let last = self.project.tracks.len() - 1;
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let index = g.track_at(pos.y).unwrap_or(index).min(last);
            actions.push(UiAction::TimeDragStart {
                tick: self.snapped(g.tick(pos.x)),
                index,
            });
        } else if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let index = g.track_at(pos.y).unwrap_or(last).min(last);
            actions.push(UiAction::TimeDragUpdate {
                tick: self.snapped(g.tick(pos.x)),
                index,
            });
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            actions.push(UiAction::TimeDragEnd);
        }
    }

    pub(crate) fn update_time_selection(&mut self, tick: u64, index: usize) {
        let Some((anchor, anchor_index)) = self.time_drag else {
            return;
        };
        if self.project.tracks.is_empty() {
            return;
        }
        let last = self.project.tracks.len() - 1;
        let (start, end) = (anchor.min(tick), anchor.max(tick));
        let low = anchor_index.min(index).min(last);
        let high = anchor_index.max(index).min(last);
        let tracks: Vec<Id> = self.project.tracks[low..=high]
            .iter()
            .map(|t| t.id)
            .collect();
        self.selection.track = self
            .project
            .tracks
            .get(anchor_index.min(last))
            .map(|t| t.id);
        self.header_focus = false;
        self.time_selection =
            (end > start).then_some(crate::arrange::TimeSelection { start, end, tracks });
    }

    fn draw_overview(&self, ui: &egui::Ui, g: &Geometry, row: Rect, actions: &mut Vec<UiAction>) {
        let painter = ui.painter_at(row);
        painter.rect_filled(row, 0.0, ui.visuals().extreme_bg_color);
        let visible = (g.tick(g.lanes.left()), g.tick(g.lanes.right()));
        let extent = (crate::arrange::song_end(&self.project)
            + ticks_per_bar(self.project.time_signature))
        .max(visible.1) as f32;
        let x = |tick: u64| row.left() + tick as f32 / extent * row.width();
        let count = self.project.tracks.len().max(1) as f32;
        let band = (row.height() - 2.0) / count;
        for (index, track) in self.project.tracks.iter().enumerate() {
            let top = row.top() + 1.0 + index as f32 * band;
            for region in &track.regions {
                painter.rect_filled(
                    Rect::from_x_y_ranges(
                        x(region.start_tick)..=x(region.end_tick()).max(x(region.start_tick) + 1.0),
                        top..=top + band.max(1.0),
                    ),
                    0.0,
                    track_color(track.color),
                );
            }
        }
        painter.rect_stroke(
            Rect::from_x_y_ranges(x(visible.0)..=x(visible.1), row.y_range()),
            CornerRadius::same(1),
            Stroke::new(1.0, ui.visuals().strong_text_color()),
            StrokeKind::Inside,
        );
        let response = crate::ui_kit::hint(
            ui.interact(row, ui.scope_id().with("overview"), Sense::click_and_drag()),
            "Overview of the whole song. Click or drag to move the view.",
        );
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
        {
            let center = (pos.x - row.left()) / row.width() * extent;
            actions.push(UiAction::ScrollTo(
                center - (visible.1 - visible.0) as f32 / 2.0,
            ));
        }
    }

    fn selection_menu(&self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
        use fnoob_core::keymap::Command;
        use fnoob_core::model::Grid;
        let selected = self.time_selection.is_some();
        let has_clips = !self.regions_in_selection().is_empty();
        let mut item = |ui: &mut egui::Ui, text: &str, command: Command, enabled: bool| {
            let mut button = egui::Button::new(text);
            if let Some(label) = fnoob_core::keymap::shortcut_label(command) {
                button = button.shortcut_text(label);
            }
            if ui.add_enabled(enabled, button).clicked() {
                actions.push(UiAction::Run(command));
                ui.close();
            }
        };
        item(ui, "Cut", Command::Cut, selected);
        item(ui, "Copy", Command::Copy, selected);
        item(ui, "Duplicate", Command::Duplicate, selected);
        item(ui, "Delete", Command::Delete, selected);
        ui.separator();
        item(
            ui,
            "Zoom to Time Selection",
            Command::ZoomToSelection,
            selected,
        );
        item(ui, "Zoom Back from Time Selection", Command::ZoomBack, true);
        ui.separator();
        item(ui, "Deactivate Clip(s)", Command::MuteRegions, has_clips);
        ui.separator();
        item(ui, "Loop Selection", Command::ToggleLoop, selected);
        ui.separator();
        let snap = self.project.snap;
        let mut toggle = egui::Button::selectable(snap.enabled, "Snap to Grid");
        if let Some(label) = fnoob_core::keymap::shortcut_label(Command::ToggleSnap) {
            toggle = toggle.shortcut_text(label);
        }
        if ui.add(toggle).clicked() {
            actions.push(UiAction::Run(Command::ToggleSnap));
            ui.close();
        }
        ui.label(egui::RichText::new("Fixed Grid:").small().weak());
        for row in Grid::ALL.chunks(5) {
            ui.horizontal(|ui| {
                for grid in row {
                    if ui
                        .selectable_label(snap.grid == *grid, grid.label())
                        .clicked()
                    {
                        actions.push(UiAction::SetGrid(*grid));
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        let index = Grid::ALL.iter().position(|g| *g == snap.grid).unwrap_or(0);
        let mut item = |ui: &mut egui::Ui, text: &str, command: Command, enabled: bool| {
            let mut button = egui::Button::new(text);
            if let Some(label) = fnoob_core::keymap::shortcut_label(command) {
                button = button.shortcut_text(label);
            }
            if ui.add_enabled(enabled, button).clicked() {
                actions.push(UiAction::Run(command));
                ui.close();
            }
        };
        item(
            ui,
            "Narrow Grid",
            Command::NarrowGrid,
            index + 1 < Grid::ALL.len(),
        );
        item(ui, "Widen Grid", Command::WidenGrid, index > 0);
    }

    fn clip_ranges(&self) -> Vec<(Id, u64, u64)> {
        if let Some(selection) = &self.time_selection {
            return selection
                .tracks
                .iter()
                .map(|t| (*t, selection.start, selection.end))
                .collect();
        }
        let mut ranges: Vec<(Id, u64, u64)> = Vec::new();
        for track in &self.project.tracks {
            for region in track
                .regions
                .iter()
                .filter(|r| self.selection.regions.contains(&r.id))
            {
                match ranges.iter_mut().find(|(t, _, _)| *t == track.id) {
                    Some(range) => {
                        range.1 = range.1.min(region.start_tick);
                        range.2 = range.2.max(region.end_tick());
                    }
                    None => ranges.push((track.id, region.start_tick, region.end_tick())),
                }
            }
        }
        ranges
    }

    pub(crate) fn consolidate(&mut self) {
        let ranges = self.clip_ranges();
        if ranges.is_empty() {
            self.message = Some("Select clips or a time range to consolidate.".into());
            return;
        }
        let kind = |t: Id| self.project.track(t).map(|t| t.kind);
        let midi: Vec<(Id, u64, u64)> = ranges
            .iter()
            .copied()
            .filter(|(t, _, _)| kind(*t) == Some(fnoob_core::model::TrackKind::Instrument))
            .collect();
        let audio: Vec<(Id, u64, u64)> = ranges
            .iter()
            .copied()
            .filter(|(t, _, _)| kind(*t) == Some(fnoob_core::model::TrackKind::Audio))
            .collect();
        let mut created = Vec::new();
        if !midi.is_empty() {
            self.edit(|p| {
                for (track, start, end) in &midi {
                    created.extend(fnoob_core::range::consolidate_midi(p, *track, *start, *end));
                }
            });
        }
        for (track, start, end) in audio {
            match self.consolidate_audio(track, start, end) {
                Ok(id) => created.push(id),
                Err(e) => {
                    self.message = Some(e);
                    break;
                }
            }
        }
        self.selection.regions = created;
        self.time_selection = None;
    }

    pub(crate) fn crop_selection(&mut self) {
        let Some(selection) = self.time_selection.clone() else {
            self.message = Some("Select a time range inside the clips to crop them.".into());
            return;
        };
        self.edit(|p| {
            fnoob_core::range::crop(p, &selection.tracks, selection.start, selection.end);
        });
    }

    pub(crate) fn fade_selection(&mut self) {
        let Some(selection) = self.time_selection.clone() else {
            self.message =
                Some("Select a time range over the start or end of an audio clip.".into());
            return;
        };
        let mut count = 0;
        self.edit(|p| {
            count =
                fnoob_core::range::fade_range(p, &selection.tracks, selection.start, selection.end);
        });
        if count == 0 {
            self.message = Some("Fades need an audio clip under the time selection.".into());
        }
    }

    pub(crate) fn reverse_clips(&mut self) {
        let ids = if self.selection.regions.is_empty() {
            self.regions_in_selection()
        } else {
            self.selection.regions.clone()
        };
        if ids.is_empty() {
            self.message = Some("Select clips to reverse.".into());
            return;
        }
        let audio: Vec<Id> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.project
                    .tracks
                    .iter()
                    .flat_map(|t| t.regions.iter())
                    .any(|r| {
                        r.id == *id
                            && matches!(r.content, fnoob_core::model::RegionContent::Audio { .. })
                    })
            })
            .collect();
        let mut prepared = Vec::new();
        for id in &audio {
            match self.reversed_audio(*id) {
                Ok(item) => prepared.push(item),
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            }
        }
        let mut files = Vec::new();
        self.edit(|p| {
            for track in &mut p.tracks {
                for region in &mut track.regions {
                    if ids.contains(&region.id) && !audio.contains(&region.id) {
                        fnoob_core::range::reverse_midi(region);
                    }
                }
            }
            for (region, rel, channels, frames, _) in &prepared {
                let file = edit::add_audio_file(p, rel.clone(), *channels, *frames);
                if let Some(target) = edit::region_mut(p, *region) {
                    if let fnoob_core::model::RegionContent::Audio {
                        file_id,
                        offset_frames,
                        ..
                    } = &mut target.content
                    {
                        *file_id = file;
                        *offset_frames = 0;
                    }
                    std::mem::swap(&mut target.fade_in_tick, &mut target.fade_out_tick);
                }
                files.push(file);
            }
        });
        for ((_, _, _, _, data), file) in prepared.into_iter().zip(files) {
            self.pool.insert(file, std::sync::Arc::new(data));
        }
        self.sync_plan();
    }

    pub(crate) fn regions_in_selection(&self) -> Vec<Id> {
        let Some(selection) = &self.time_selection else {
            return Vec::new();
        };
        self.project
            .tracks
            .iter()
            .filter(|t| selection.tracks.contains(&t.id))
            .flat_map(|t| t.regions.iter())
            .filter(|r| r.start_tick < selection.end && r.end_tick() > selection.start)
            .map(|r| r.id)
            .collect()
    }

    pub(crate) fn selection_command(&mut self, command: fnoob_core::keymap::Command) {
        use fnoob_core::keymap::Command;
        use fnoob_core::range;
        let Some(selection) = self.time_selection.clone() else {
            return;
        };
        let (start, end, tracks) = (selection.start, selection.end, selection.tracks.clone());
        if command != Command::Delete {
            let clip = range::copy_range(&self.project, &tracks, start, end);
            if command == Command::Duplicate {
                let Some(&first) = tracks.first() else {
                    return;
                };
                self.edit(|p| {
                    range::paste_range(p, first, end, &clip);
                });
                self.time_selection = Some(crate::arrange::TimeSelection {
                    start: end,
                    end: end + clip.length,
                    tracks,
                });
                self.insert_tick = end;
                return;
            }
            self.time_clipboard = Some(clip);
            self.clipboard.clear();
            self.track_clipboard.clear();
            self.system_clipboard = Some(crate::arrange::length_text(
                end - start,
                self.project.time_signature,
            ));
        }
        if command != Command::Copy {
            self.edit(|p| range::clear_range(p, &tracks, start, end));
        }
    }

    pub(crate) fn time_command(&mut self, command: fnoob_core::keymap::Command) {
        use fnoob_core::keymap::Command;
        use fnoob_core::range;
        if command == Command::PasteTime {
            self.paste_time_clip(true);
            return;
        }
        let Some(selection) = self.time_selection.clone() else {
            self.message = Some("Select a time range first.".into());
            return;
        };
        let (start, end) = (selection.start, selection.end);
        match command {
            Command::CutTime => {
                self.time_clipboard = Some(range::copy_range(
                    &self.project,
                    &selection.tracks,
                    start,
                    end,
                ));
                self.clipboard.clear();
                self.track_clipboard.clear();
                self.edit(|p| range::delete_time(p, start, end));
                self.time_selection = None;
                self.insert_tick = start;
            }
            Command::DeleteTime => {
                self.edit(|p| range::delete_time(p, start, end));
                self.time_selection = None;
                self.insert_tick = start;
            }
            Command::DuplicateTime => {
                let clip = range::copy_range(&self.project, &selection.tracks, start, end);
                let Some(&first) = selection.tracks.first() else {
                    return;
                };
                self.edit(|p| {
                    range::insert_time(p, end, clip.length);
                    range::paste_range(p, first, end, &clip);
                });
                self.time_selection = Some(crate::arrange::TimeSelection {
                    start: end,
                    end: end + clip.length,
                    tracks: selection.tracks,
                });
                self.insert_tick = end;
            }
            _ => {}
        }
    }

    pub(crate) fn insert_silence(&mut self) -> bool {
        let Some(selection) = self.time_selection.clone() else {
            return false;
        };
        self.edit(|p| fnoob_core::range::insert_time(p, selection.start, selection.length()));
        true
    }

    pub(crate) fn paste_time_clip(&mut self, insert: bool) {
        let Some(clip) = self.time_clipboard.clone() else {
            self.message = Some("Copy or cut a time selection first.".into());
            return;
        };
        let at = self
            .time_selection
            .as_ref()
            .map_or(self.insert_tick, |s| s.start);
        let first = self
            .time_selection
            .as_ref()
            .and_then(|s| s.tracks.first().copied())
            .or(self.selection.track)
            .or(self.project.tracks.first().map(|t| t.id));
        let Some(first) = first else {
            return;
        };
        self.edit(|p| {
            if insert {
                fnoob_core::range::insert_time(p, at, clip.length);
            }
            fnoob_core::range::paste_range(p, first, at, &clip);
        });
        let index = self
            .project
            .tracks
            .iter()
            .position(|t| t.id == first)
            .unwrap_or(0);
        let tracks = self
            .project
            .tracks
            .iter()
            .skip(index)
            .take(clip.lanes.len().max(1))
            .map(|t| t.id)
            .collect();
        self.time_selection = Some(crate::arrange::TimeSelection {
            start: at,
            end: at + clip.length,
            tracks,
        });
    }

    fn draw_time_selection(&self, ui: &egui::Ui, g: &Geometry) {
        let Some(selection) = &self.time_selection else {
            return;
        };
        let painter = ui.painter_at(g.lanes);
        let fill = ui.visuals().selection.bg_fill.gamma_multiply(0.35);
        let (left, right) = (g.x(selection.start), g.x(selection.end));
        for (index, track) in self.project.tracks.iter().enumerate() {
            if self.project.is_folded_away(track) {
                continue;
            }
            if selection.tracks.contains(&track.id) {
                let top = g.lane_top(index);
                painter.rect_filled(
                    Rect::from_x_y_ranges(left..=right, top..=top + LANE_HEIGHT),
                    0.0,
                    fill,
                );
            }
        }
    }

    fn draw_insert_marker(&self, ui: &egui::Ui, g: &Geometry, time_row: Rect) {
        let x = g.x(self.insert_tick);
        if !g.lanes.x_range().contains(x) {
            return;
        }
        let painter = ui.painter_at(g.lanes.union(time_row).union(g.ruler));
        let color = Color32::from_rgb(120, 220, 230);
        for y in [g.lanes.top(), time_row.top()] {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x - 5.0, y),
                    Pos2::new(x + 5.0, y),
                    Pos2::new(x, y + 6.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
        if self.time_selection.is_some() {
            return;
        }
        let time = ui.input(|i| i.time);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
        if (time * 2.0) as i64 % 2 == 1 {
            return;
        }
        let marked = self.title_selection();
        for (index, track) in self.project.tracks.iter().enumerate() {
            if self.project.is_folded_away(track) {
                continue;
            }
            if marked.is_empty() || marked.contains(&track.id) {
                let top = g.lane_top(index);
                painter.vline(x, top..=top + LANE_HEIGHT, Stroke::new(1.5, color));
            }
        }
    }

    fn draw_time_row(&self, ui: &egui::Ui, g: &Geometry, row: Rect, actions: &mut Vec<UiAction>) {
        let painter = ui.painter_at(row);
        let visuals = ui.visuals();
        painter.rect_filled(row, 0.0, visuals.faint_bg_color);
        let rate = self.sample_rate();
        let timeline = self.timeline();
        let left_tick = g.tick(row.left());
        let ticks_per_second =
            timeline.bpm_at_tick(left_tick) / 60.0 * fnoob_core::time::PPQ as f64;
        let step = crate::arrange::clock_step(ticks_per_second as f32 * g.pixels_per_tick);
        let first = crate::arrange::seconds_at(timeline, left_tick, rate) as u64 / step * step;
        let last = crate::arrange::seconds_at(timeline, g.tick(row.right()), rate) as u64 + step;
        let mut second = first;
        while second <= last {
            let tick = crate::arrange::tick_at_seconds(timeline, second as f64, rate);
            let x = g.x(tick);
            painter.vline(
                x,
                row.top()..=row.top() + 5.0,
                Stroke::new(1.0, visuals.weak_text_color()),
            );
            painter.text(
                Pos2::new(x + 3.0, row.center().y),
                Align2::LEFT_CENTER,
                crate::arrange::clock_text(second as f64),
                FontId::proportional(10.0),
                visuals.weak_text_color(),
            );
            second += step;
        }
        let response = crate::ui_kit::hint(
            ui.interact(row, ui.scope_id().with("time_row"), Sense::click_and_drag()),
            "Time ruler in minutes and seconds. Click to move the insert marker.",
        );
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
        {
            actions.push(UiAction::SetInsert(self.snapped(g.tick(pos.x))));
        }
    }

    fn draw_zoom_corner(&self, ui: &mut egui::Ui, rect: Rect, actions: &mut Vec<UiAction>) {
        use fnoob_core::keymap::{Command, shortcut_label};
        let with_key = |text: &str, command: Command| match shortcut_label(command) {
            Some(label) => format!("{text} ({label})"),
            None => text.to_owned(),
        };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(1.0)));
        child.horizontal_centered(|ui| {
            crate::ui_kit::compact(ui);
            for (label, text, command) in [
                ("W", "Fit the whole song in view", Command::FitSong),
                ("Z", "Zoom to the time selection", Command::ZoomToSelection),
                ("X", "Zoom back", Command::ZoomBack),
            ] {
                if ui
                    .small_button(label)
                    .on_hover_text(with_key(text, command))
                    .clicked()
                {
                    actions.push(UiAction::Run(command));
                }
            }
        });
    }

    fn draw_locators(&self, ui: &mut egui::Ui, rect: Rect, actions: &mut Vec<UiAction>) {
        use fnoob_core::keymap::Command;
        let area = Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.bottom() - 26.0),
            Vec2::new(rect.width() - 12.0, 22.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        child.horizontal(|ui| {
            crate::ui_kit::compact(ui);
            let tick = self.insert_tick;
            let exists = self.project.markers.iter().any(|m| m.tick == tick);
            let label = if exists { "Delete" } else { "Set" };
            if ui
                .button(label)
                .on_hover_text("Add a locator at the insert marker, or delete the one there")
                .clicked()
            {
                actions.push(UiAction::ToggleMarkerAt(tick));
            }
            if ui
                .button("<")
                .on_hover_text("Jump to the previous locator")
                .clicked()
            {
                actions.push(UiAction::Run(Command::PreviousMarker));
            }
            if ui
                .button(">")
                .on_hover_text("Jump to the next locator")
                .clicked()
            {
                actions.push(UiAction::Run(Command::NextMarker));
            }
        });
    }

    fn zoom_at(&mut self, factor: f32, anchor: f32) {
        let tick = self.view.scroll_tick + anchor / self.view.pixels_per_tick;
        self.zoom(factor);
        self.view.scroll_tick = (tick - anchor / self.view.pixels_per_tick).max(0.0);
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

    fn update_region_trim(&mut self, pointer: Pos2) {
        let Some(Drag::RegionTrim {
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
        let delta = ((pointer.x - anchor.x) / self.view.pixels_per_tick) as i64;
        let wanted = (original.start_tick as i64 + delta).max(0) as u64;
        let new_start = self
            .snapped(wanted)
            .min(original.end_tick().saturating_sub(1));
        edit::trim_region_start(&mut project, region, new_start);
        if project != self.project {
            self.project = project;
            self.sync_plan();
        }
    }

    pub(crate) fn automation_param(&self, track: Id) -> AutomationParam {
        self.automation_params
            .get(&track)
            .copied()
            .unwrap_or_default()
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

pub(crate) fn input_label(input: AudioInput) -> String {
    let first = input.first_channel + 1;
    if input.channels > 1 {
        format!("In {first}-{}", first + 1)
    } else {
        format!("In {first}")
    }
}

pub(crate) fn input_options(channels: u16) -> Vec<AudioInput> {
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

pub(crate) fn title_menu(ui: &mut egui::Ui) -> Option<fnoob_core::keymap::Command> {
    use fnoob_core::keymap::{Command, shortcut_label};
    let mut chosen = None;
    for (text, command) in [
        ("Cut", Command::Cut),
        ("Copy", Command::Copy),
        ("Paste", Command::Paste),
        ("Duplicate", Command::Duplicate),
        ("Delete", Command::Delete),
        ("Rename", Command::Rename),
        ("Group Tracks", Command::GroupTracks),
        ("Ungroup Tracks", Command::Ungroup),
        ("Freeze / Unfreeze Track", Command::FreezeTrack),
        ("Freeze and Flatten Track", Command::FlattenTrack),
    ] {
        let mut button = egui::Button::new(text);
        if let Some(label) = shortcut_label(command) {
            button = button.shortcut_text(label);
        }
        if ui.add(button).clicked() {
            chosen = Some(command);
            ui.close();
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_light_and_dark_rows() {
        let base = track_color(0);
        let light = track_color(8);
        let dark = track_color(16);
        assert!(light.r() >= base.r() && light.g() >= base.g());
        assert!(dark.r() <= base.r() && dark.b() <= base.b());
        for index in 0..fnoob_core::model::PALETTE_COLORS {
            let _ = track_color(index);
        }
    }

    #[test]
    fn input_choices_cover_mono_and_stereo_pairs() {
        let labels: Vec<String> = input_options(4).into_iter().map(input_label).collect();
        assert_eq!(labels, ["In 1", "In 2", "In 3", "In 4", "In 1-2", "In 3-4"]);
        assert_eq!(input_options(1).len(), 1);
    }
}
