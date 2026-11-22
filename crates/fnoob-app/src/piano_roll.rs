use crate::app::{App, Drag};
use crate::timeline::track_color;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind,
    Vec2,
};
use fnoob_core::edit;
use fnoob_core::keymap::Command;
use fnoob_core::model::{Id, Note, Project};
use fnoob_core::time::{grid_ticks, snap_floor, snap_round, ticks_per_bar, ticks_per_beat};
use fnoob_engine::Command as EngineCommand;
use std::time::{Duration, Instant};

const KEYS_WIDTH: f32 = 56.0;
const RULER: f32 = 18.0;
const ROW: f32 = 12.0;
const VELOCITY_LANE: f32 = 56.0;
const EDGE: f32 = 6.0;
const PREVIEW: Duration = Duration::from_millis(250);
const MIN_PIXELS_PER_TICK: f32 = 0.01;
const MAX_PIXELS_PER_TICK: f32 = 2.0;
const DEFAULT_VELOCITY: u8 = 100;
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const PLAYHEAD: Color32 = Color32::from_rgb(240, 240, 240);

pub fn note_name(pitch: u8) -> String {
    let octave = i32::from(pitch / 12) - 1;
    format!("{}{octave}", NOTE_NAMES[usize::from(pitch % 12)])
}

fn is_black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

pub struct Editor {
    pub open: bool,
    pub region: Option<Id>,
    pub selected: Vec<usize>,
    pub pixels_per_tick: f32,
    pub scroll_tick: f32,
    pub scroll_y: f32,
    pub template: Option<(u64, u8)>,
    pub clipboard: Vec<Note>,
    pub preview: Option<(Id, u8, Instant)>,
    pub held_key: Option<(Id, u8)>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            open: false,
            region: None,
            selected: Vec::new(),
            pixels_per_tick: 0.12,
            scroll_y: f32::from(127u8 - 84) * ROW,
            scroll_tick: 0.0,
            template: None,
            clipboard: Vec::new(),
            preview: None,
            held_key: None,
        }
    }
}

impl Editor {
    pub fn clamp_selection(&mut self, project: &Project) {
        match self.region.and_then(|id| project.find_region(id)) {
            Some((_, region)) => {
                let len = region.notes().len();
                self.selected.retain(|&i| i < len);
            }
            None => self.selected.clear(),
        }
    }
}

enum EditorAction {
    Select {
        index: usize,
        toggle: bool,
    },
    ClearSelection,
    AddNote {
        tick: u64,
        pitch: u8,
    },
    DeleteNote(usize),
    DragStart {
        index: usize,
        pointer: Pos2,
        resize: bool,
    },
    DragUpdate(Pos2),
    VelocityStart(usize),
    VelocityUpdate(u8),
    DragEnd,
    KeyDown(u8),
    KeyUp,
    Locate(u64),
    Scroll {
        ticks: f32,
        pixels: f32,
    },
    Zoom(f32),
}

struct Layout {
    keys: Rect,
    ruler: Rect,
    notes: Rect,
    velocity: Rect,
    pixels_per_tick: f32,
    scroll_tick: f32,
    scroll_y: f32,
}

impl Layout {
    fn x(&self, tick: u64) -> f32 {
        self.notes.left() + (tick as f32 - self.scroll_tick) * self.pixels_per_tick
    }

    fn tick(&self, x: f32) -> u64 {
        ((x - self.notes.left()) / self.pixels_per_tick + self.scroll_tick).max(0.0) as u64
    }

    fn y(&self, pitch: u8) -> f32 {
        self.notes.top() + f32::from(127 - pitch) * ROW - self.scroll_y
    }

    fn pitch(&self, y: f32) -> u8 {
        let row = ((y - self.notes.top() + self.scroll_y) / ROW).floor();
        (127.0 - row).clamp(0.0, 127.0) as u8
    }

    fn velocity_y(&self, velocity: u8) -> f32 {
        let inner = self.velocity.shrink(4.0);
        inner.bottom() - inner.height() * f32::from(velocity) / 127.0
    }

    fn velocity_at(&self, y: f32) -> u8 {
        let inner = self.velocity.shrink(4.0);
        (((inner.bottom() - y) / inner.height()) * 127.0)
            .round()
            .clamp(1.0, 127.0) as u8
    }
}

impl App {
    fn grid_ticks(&self) -> u64 {
        grid_ticks(self.project.snap.grid, self.project.time_signature)
    }

    fn snap_tick(&self, tick: u64) -> u64 {
        if self.project.snap.enabled {
            snap_round(tick, self.grid_ticks())
        } else {
            tick
        }
    }

    fn note_template(&self) -> (u64, u8) {
        self.editor
            .template
            .unwrap_or((self.grid_ticks(), DEFAULT_VELOCITY))
    }

    fn editor_region(&self) -> Option<(Id, Id, u64, u64)> {
        let id = self.editor.region?;
        let (track, region) = self.project.find_region(id)?;
        Some((id, track.id, region.start_tick, region.end_tick()))
    }

    pub(crate) fn open_editor(&mut self, region_id: Id) {
        let Some((_, region)) = self.project.find_region(region_id) else {
            return;
        };
        let pitches = region.notes().iter().map(|n| n.pitch);
        let top = pitches.max().map_or(84, |p| p.saturating_add(4).min(127));
        self.editor.scroll_y = f32::from(127 - top) * ROW;
        self.editor.scroll_tick = region.start_tick as f32;
        self.editor.region = Some(region_id);
        self.editor.selected.clear();
        self.editor.open = true;
        self.devices_open = false;
        self.detail_devices = false;
    }

    fn preview(&mut self, track: Id, pitch: u8, velocity: u8) {
        self.stop_preview();
        self.send(EngineCommand::Preview {
            track,
            pitch,
            velocity: velocity.max(1),
        });
        self.editor.preview = Some((track, pitch, Instant::now()));
    }

    fn stop_preview(&mut self) {
        if let Some((track, pitch, _)) = self.editor.preview.take() {
            self.send(EngineCommand::Preview {
                track,
                pitch,
                velocity: 0,
            });
        }
    }

    pub(crate) fn update_preview(&mut self) {
        if self
            .editor
            .preview
            .is_some_and(|(_, _, started)| started.elapsed() >= PREVIEW)
        {
            self.stop_preview();
        }
    }

    fn remember_note(&mut self, region: Id, index: usize) {
        if let Some((_, r)) = self.project.find_region(region)
            && let Some(note) = r.notes().get(index)
        {
            self.editor.template = Some((note.length_tick, note.velocity));
        }
    }

    pub(crate) fn editor_command(&mut self, command: Command) -> bool {
        let Some((region, _, start, end)) = self.editor_region() else {
            return false;
        };
        let selected = self.editor.selected.clone();
        let grid = self.grid_ticks();
        match command {
            Command::Delete => {
                self.edit(|p| {
                    edit::delete_notes(p, region, &selected);
                });
                self.editor.selected.clear();
            }
            Command::SelectAll => {
                let count = self
                    .project
                    .find_region(region)
                    .map_or(0, |(_, r)| r.notes().len());
                self.editor.selected = (0..count).collect();
            }
            Command::Copy => {
                self.editor.clipboard = edit::copy_notes(&self.project, region, &selected)
            }
            Command::Cut => {
                self.editor.clipboard = edit::copy_notes(&self.project, region, &selected);
                self.edit(|p| {
                    edit::delete_notes(p, region, &selected);
                });
                self.editor.selected.clear();
            }
            Command::Paste => {
                let tick = self.playhead_tick();
                let at = if (start..end).contains(&tick) {
                    tick - start
                } else {
                    0
                };
                let clip = self.editor.clipboard.clone();
                let mut created = Vec::new();
                self.edit(|p| created = edit::paste_notes(p, region, at, &clip));
                if !created.is_empty() {
                    self.editor.selected = created;
                }
            }
            Command::NudgeLeft | Command::NudgeRight => {
                let delta = if command == Command::NudgeLeft {
                    -(grid as i64)
                } else {
                    grid as i64
                };
                self.edit(|p| {
                    edit::move_notes(p, region, &selected, delta, 0);
                });
            }
            Command::SelectPreviousTrack | Command::SelectNextTrack => {
                let delta = if command == Command::SelectPreviousTrack {
                    1
                } else {
                    -1
                };
                self.edit(|p| {
                    edit::move_notes(p, region, &selected, 0, delta);
                });
            }
            Command::Quantize => {
                let picked: Vec<usize> = if selected.is_empty() {
                    let count = self
                        .project
                        .find_region(region)
                        .map_or(0, |(_, r)| r.notes().len());
                    (0..count).collect()
                } else {
                    selected
                };
                self.edit(|p| {
                    edit::quantize_notes(p, region, &picked, grid);
                });
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn piano_roll(&mut self, ui: &mut egui::Ui) {
        let Some((region, track, start, end)) = self.editor_region() else {
            ui.add_space(8.0);
            ui.label("Select a MIDI region and press P, or double-click a region.");
            return;
        };
        self.editor_toolbar(ui, region, track);
        let (area, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let layout = Layout {
            keys: Rect::from_min_max(
                Pos2::new(area.left(), area.top() + RULER),
                Pos2::new(area.left() + KEYS_WIDTH, area.bottom() - VELOCITY_LANE),
            ),
            ruler: Rect::from_min_max(
                Pos2::new(area.left() + KEYS_WIDTH, area.top()),
                Pos2::new(area.right(), area.top() + RULER),
            ),
            notes: Rect::from_min_max(
                Pos2::new(area.left() + KEYS_WIDTH, area.top() + RULER),
                Pos2::new(area.right(), area.bottom() - VELOCITY_LANE),
            ),
            velocity: Rect::from_min_max(
                Pos2::new(area.left() + KEYS_WIDTH, area.bottom() - VELOCITY_LANE),
                area.max,
            ),
            pixels_per_tick: self.editor.pixels_per_tick,
            scroll_tick: self.editor.scroll_tick,
            scroll_y: self.editor.scroll_y,
        };
        let mut actions = Vec::new();
        if response.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if zoom != 1.0 {
                actions.push(EditorAction::Zoom(zoom));
            } else if scroll != Vec2::ZERO {
                actions.push(EditorAction::Scroll {
                    ticks: -scroll.x / layout.pixels_per_tick,
                    pixels: -scroll.y,
                });
            }
        }
        ui.painter_at(area)
            .rect_filled(area, 0.0, ui.visuals().extreme_bg_color);
        self.draw_note_grid(ui, &layout, region, track, start, end, &mut actions);
        self.draw_editor_ruler(ui, &layout, start, end, &mut actions);
        self.draw_keys(ui, &layout, &mut actions);
        self.draw_velocity(ui, &layout, region, start, &mut actions);
        let playhead = layout.x(self.playhead_tick());
        if playhead >= layout.notes.left() && playhead <= layout.notes.right() {
            ui.painter_at(area).vline(
                playhead,
                layout.ruler.top()..=area.bottom(),
                Stroke::new(1.5, PLAYHEAD),
            );
        }
        for action in actions {
            self.apply_editor_action(action, region, track, start, end);
        }
    }

    fn editor_toolbar(&mut self, ui: &mut egui::Ui, region: Id, track: Id) {
        ui.horizontal(|ui| {
            let (track_name, region_name) = match self.project.find_region(region) {
                Some((t, r)) => (t.name.clone(), r.name.clone()),
                None => (String::new(), String::new()),
            };
            ui.label(egui::RichText::new("Piano Roll").strong());
            ui.label(format!("{track_name}  /  {region_name}"));
            ui.separator();
            let selected = self.editor.selected.clone();
            let first = selected.first().and_then(|&i| {
                self.project
                    .find_region(region)
                    .and_then(|(_, r)| r.notes().get(i).copied())
            });
            match first {
                Some(note) => {
                    ui.label(format!(
                        "{} selected  {}",
                        selected.len(),
                        note_name(note.pitch)
                    ));
                    let mut velocity = note.velocity;
                    let slider = ui.add(egui::Slider::new(&mut velocity, 1..=127).text("Velocity"));
                    if slider.changed() {
                        self.editor.template = Some((note.length_tick, velocity));
                    }
                    self.live_edit(&slider, |p| {
                        edit::set_notes_velocity(p, region, &selected, velocity);
                    });
                }
                None => {
                    ui.label("Click to add a note. Right-click a note to delete it.");
                }
            }
            ui.separator();
            if ui
                .button("Quantize")
                .on_hover_text("Quantize (Q)")
                .clicked()
            {
                self.editor_command(Command::Quantize);
            }
            if ui.button("Instrument...").clicked() {
                self.open_instrument(track);
            }
            if ui.button("Close").clicked() {
                self.editor.open = false;
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_note_grid(
        &self,
        ui: &mut egui::Ui,
        g: &Layout,
        region: Id,
        track: Id,
        start: u64,
        end: u64,
        actions: &mut Vec<EditorAction>,
    ) {
        let painter = ui.painter_at(g.notes);
        let visuals = ui.visuals().clone();
        let first_pitch = g.pitch(g.notes.bottom());
        let last_pitch = g.pitch(g.notes.top());
        for pitch in first_pitch..=last_pitch {
            let y = g.y(pitch);
            if is_black(pitch) {
                let row = Rect::from_x_y_ranges(g.notes.x_range(), y..=y + ROW);
                painter.rect_filled(row, 0.0, Color32::from_black_alpha(40));
            }
            if pitch % 12 == 0 {
                painter.hline(
                    g.notes.x_range(),
                    y + ROW,
                    Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.5)),
                );
            }
        }
        let outside = Color32::from_black_alpha(90);
        let left = Rect::from_x_y_ranges(g.notes.left()..=g.x(start), g.notes.y_range());
        let right = Rect::from_x_y_ranges(g.x(end)..=g.notes.right(), g.notes.y_range());
        painter.rect_filled(left, 0.0, outside);
        painter.rect_filled(right, 0.0, outside);
        let ts = self.project.time_signature;
        let bar = ticks_per_bar(ts);
        let beat = ticks_per_beat(ts);
        let grid = self.grid_ticks();
        let step = if grid as f32 * g.pixels_per_tick >= 6.0 {
            grid
        } else if beat as f32 * g.pixels_per_tick >= 6.0 {
            beat
        } else {
            bar
        };
        let first = snap_floor(g.tick(g.notes.left()), step);
        let last = g.tick(g.notes.right());
        let mut tick = first;
        while tick <= last {
            let color = if tick.is_multiple_of(bar) {
                visuals.weak_text_color()
            } else if tick.is_multiple_of(beat) {
                visuals.weak_text_color().gamma_multiply(0.5)
            } else {
                visuals.weak_text_color().gamma_multiply(0.2)
            };
            painter.vline(g.x(tick), g.notes.y_range(), Stroke::new(1.0, color));
            tick += step;
        }
        let background = ui.interact(g.notes, ui.scope_id().with("note_grid"), Sense::click());
        let background = crate::ui_kit::hint(
            background,
            "Double-click to add a note, or turn on Draw mode (B) and click.",
        );
        if let Some(pos) = background.interact_pointer_pos() {
            let add = background.double_clicked() || self.draw_mode && background.clicked();
            if add {
                actions.push(EditorAction::AddNote {
                    tick: g.tick(pos.x),
                    pitch: g.pitch(pos.y),
                });
            } else if background.clicked() {
                actions.push(EditorAction::ClearSelection);
            }
        }
        let Some((track_ref, region_ref)) = self.project.find_region(region) else {
            return;
        };
        debug_assert_eq!(track_ref.id, track);
        let color = track_color(track_ref.color);
        for (index, note) in region_ref.notes().iter().enumerate() {
            let x0 = g.x(start + note.start_tick);
            let x1 = g.x(start + note.end_tick()).max(x0 + 3.0);
            let y = g.y(note.pitch);
            let rect = Rect::from_x_y_ranges(x0..=x1, y + 1.0..=y + ROW - 1.0);
            if !rect.intersects(g.notes) {
                continue;
            }
            let selected = self.editor.selected.contains(&index);
            let shade = 0.45 + 0.55 * f32::from(note.velocity) / 127.0;
            painter.rect_filled(rect, CornerRadius::same(2), color.gamma_multiply(shade));
            let border = if selected {
                Stroke::new(2.0, Color32::WHITE)
            } else {
                Stroke::new(1.0, Color32::from_black_alpha(160))
            };
            painter.rect_stroke(rect, CornerRadius::same(2), border, StrokeKind::Inside);
            let response = ui.interact(
                rect.intersect(g.notes),
                ui.scope_id().with(("note", region, index)),
                Sense::click_and_drag(),
            );
            let near_edge = |pos: Pos2| pos.x >= rect.right() - EDGE && rect.width() > 2.0 * EDGE;
            let response = if response.hover_pos().is_some_and(near_edge) {
                response.on_hover_cursor(CursorIcon::ResizeHorizontal)
            } else {
                response
            };
            if response.drag_started() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    actions.push(EditorAction::DragStart {
                        index,
                        pointer,
                        resize: near_edge(pointer),
                    });
                }
            } else if response.dragged() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    actions.push(EditorAction::DragUpdate(pointer));
                }
            } else if response.secondary_clicked() {
                actions.push(EditorAction::DeleteNote(index));
            } else if response.clicked() {
                actions.push(EditorAction::Select {
                    index,
                    toggle: ui.input(|i| i.modifiers.ctrl),
                });
            }
            if response.drag_stopped() {
                actions.push(EditorAction::DragEnd);
            }
        }
    }

    fn draw_editor_ruler(
        &self,
        ui: &mut egui::Ui,
        g: &Layout,
        start: u64,
        end: u64,
        actions: &mut Vec<EditorAction>,
    ) {
        let painter = ui.painter_at(g.ruler);
        let visuals = ui.visuals();
        painter.rect_filled(g.ruler, 0.0, visuals.faint_bg_color);
        let span = Rect::from_x_y_ranges(
            g.x(start)..=g.x(end),
            g.ruler.bottom() - 4.0..=g.ruler.bottom(),
        );
        painter.rect_filled(span, 0.0, visuals.selection.bg_fill);
        let bar = ticks_per_bar(self.project.time_signature);
        let bar_px = bar as f32 * g.pixels_per_tick;
        let label_every = ((40.0 / bar_px).ceil() as u64).max(1);
        let first = snap_floor(g.tick(g.ruler.left()), bar) / bar;
        let last = g.tick(g.ruler.right()) / bar + 1;
        for index in first..=last {
            let x = g.x(index * bar);
            painter.vline(
                x,
                g.ruler.y_range(),
                Stroke::new(1.0, visuals.weak_text_color()),
            );
            if index % label_every == 0 {
                painter.text(
                    Pos2::new(x + 3.0, g.ruler.top() + 1.0),
                    Align2::LEFT_TOP,
                    (index + 1).to_string(),
                    FontId::proportional(11.0),
                    visuals.text_color(),
                );
            }
        }
        let response = ui.interact(
            g.ruler,
            ui.scope_id().with("editor_ruler"),
            Sense::click_and_drag(),
        );
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
        {
            actions.push(EditorAction::Locate(self.snap_tick(g.tick(pos.x))));
        }
    }

    fn draw_keys(&self, ui: &mut egui::Ui, g: &Layout, actions: &mut Vec<EditorAction>) {
        let painter = ui.painter_at(g.keys);
        painter.rect_filled(g.keys, 0.0, Color32::from_gray(230));
        let first = g.pitch(g.keys.bottom());
        let last = g.pitch(g.keys.top());
        let held = self.editor.held_key.map(|(_, p)| p);
        for pitch in first..=last {
            let y = g.y(pitch);
            let row = Rect::from_x_y_ranges(g.keys.x_range(), y..=y + ROW);
            let fill = if held == Some(pitch) {
                ui.visuals().selection.bg_fill
            } else if is_black(pitch) {
                Color32::from_gray(40)
            } else {
                Color32::from_gray(232)
            };
            let key = if is_black(pitch) {
                Rect::from_x_y_ranges(
                    g.keys.left()..=g.keys.left() + KEYS_WIDTH * 0.62,
                    row.y_range(),
                )
            } else {
                row
            };
            painter.rect_filled(key, 0.0, fill);
            painter.hline(
                g.keys.x_range(),
                y + ROW,
                Stroke::new(1.0, Color32::from_gray(170)),
            );
            if pitch % 12 == 0 {
                painter.text(
                    Pos2::new(g.keys.right() - 3.0, y + ROW / 2.0),
                    Align2::RIGHT_CENTER,
                    note_name(pitch),
                    FontId::proportional(9.0),
                    Color32::from_gray(60),
                );
            }
        }
        let response = ui.interact(
            g.keys,
            ui.scope_id().with("editor_keys"),
            Sense::click_and_drag(),
        );
        if response.is_pointer_button_down_on() {
            if let Some(pos) = response.interact_pointer_pos() {
                let pitch = g.pitch(pos.y);
                if held != Some(pitch) {
                    actions.push(EditorAction::KeyDown(pitch));
                }
            }
        } else if held.is_some() {
            actions.push(EditorAction::KeyUp);
        }
    }

    fn draw_velocity(
        &self,
        ui: &mut egui::Ui,
        g: &Layout,
        region: Id,
        start: u64,
        actions: &mut Vec<EditorAction>,
    ) {
        let painter = ui.painter_at(g.velocity);
        painter.rect_filled(g.velocity, 0.0, ui.visuals().faint_bg_color);
        painter.hline(
            g.velocity.x_range(),
            g.velocity.top(),
            Stroke::new(1.0, ui.visuals().weak_text_color()),
        );
        let keys_corner = Rect::from_min_max(
            Pos2::new(g.keys.left(), g.velocity.top()),
            Pos2::new(g.keys.right(), g.velocity.bottom()),
        );
        ui.painter_at(keys_corner).text(
            keys_corner.center(),
            Align2::CENTER_CENTER,
            "Velocity",
            FontId::proportional(10.0),
            ui.visuals().weak_text_color(),
        );
        let Some((track, r)) = self.project.find_region(region) else {
            return;
        };
        let color = track_color(track.color);
        let mut stems = Vec::new();
        for (index, note) in r.notes().iter().enumerate() {
            let x = g.x(start + note.start_tick);
            if x < g.velocity.left() - 4.0 || x > g.velocity.right() + 4.0 {
                continue;
            }
            let selected = self.editor.selected.contains(&index);
            let top = g.velocity_y(note.velocity);
            let stroke = Stroke::new(
                if selected { 3.0 } else { 2.0 },
                if selected { Color32::WHITE } else { color },
            );
            painter.vline(x, top..=g.velocity.bottom() - 4.0, stroke);
            painter.circle_filled(Pos2::new(x, top), 3.0, stroke.color);
            stems.push((x, index));
        }
        let response = ui.interact(
            g.velocity,
            ui.scope_id().with("velocity_lane"),
            Sense::click_and_drag(),
        );
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        if response.drag_started() || response.clicked() {
            let nearest = stems
                .iter()
                .filter(|(x, _)| (x - pos.x).abs() <= 6.0)
                .min_by(|a, b| (a.0 - pos.x).abs().total_cmp(&(b.0 - pos.x).abs()));
            if let Some(&(_, index)) = nearest {
                actions.push(EditorAction::VelocityStart(index));
                actions.push(EditorAction::VelocityUpdate(g.velocity_at(pos.y)));
                if response.clicked() {
                    actions.push(EditorAction::DragEnd);
                }
            }
        } else if response.dragged() {
            actions.push(EditorAction::VelocityUpdate(g.velocity_at(pos.y)));
        }
        if response.drag_stopped() {
            actions.push(EditorAction::DragEnd);
        }
    }

    fn apply_editor_action(
        &mut self,
        action: EditorAction,
        region: Id,
        track: Id,
        start: u64,
        end: u64,
    ) {
        match action {
            EditorAction::Select { index, toggle } => {
                if toggle {
                    if let Some(i) = self.editor.selected.iter().position(|&s| s == index) {
                        self.editor.selected.remove(i);
                    } else {
                        self.editor.selected.push(index);
                    }
                } else {
                    self.editor.selected = vec![index];
                }
                self.remember_note(region, index);
                if let Some((_, r)) = self.project.find_region(region)
                    && let Some(note) = r.notes().get(index).copied()
                {
                    self.preview(track, note.pitch, note.velocity);
                }
            }
            EditorAction::ClearSelection => self.editor.selected.clear(),
            EditorAction::AddNote { tick, pitch } => {
                if !(start..end).contains(&tick) {
                    return;
                }
                let tick = if self.project.snap.enabled {
                    snap_floor(tick, self.grid_ticks()).max(start)
                } else {
                    tick
                };
                let (length, velocity) = self.note_template();
                let note = Note {
                    pitch,
                    velocity,
                    start_tick: tick - start,
                    length_tick: length.max(1),
                };
                let mut created = None;
                self.edit(|p| created = edit::add_note(p, region, note));
                if let Some(index) = created {
                    self.editor.selected = vec![index];
                    self.preview(track, pitch, velocity);
                }
            }
            EditorAction::DeleteNote(index) => {
                self.edit(|p| {
                    edit::delete_notes(p, region, &[index]);
                });
                self.editor.selected.clear();
            }
            EditorAction::DragStart {
                index,
                pointer,
                resize,
            } => {
                if !self.editor.selected.contains(&index) {
                    self.editor.selected = vec![index];
                }
                self.drag = Some(Drag::Notes {
                    origin: self.project.clone(),
                    region,
                    primary: index,
                    anchor: pointer,
                    resize,
                });
                if let Some((_, r)) = self.project.find_region(region)
                    && let Some(note) = r.notes().get(index).copied()
                {
                    self.preview(track, note.pitch, note.velocity);
                }
            }
            EditorAction::DragUpdate(pointer) => self.update_note_drag(pointer, track),
            EditorAction::VelocityStart(index) => {
                self.drag = Some(Drag::Velocity {
                    origin: self.project.clone(),
                    index,
                });
            }
            EditorAction::VelocityUpdate(velocity) => {
                let Some(Drag::Velocity { origin, index }) = &self.drag else {
                    return;
                };
                let index = *index;
                let mut project = origin.clone();
                let targets = if self.editor.selected.contains(&index) {
                    self.editor.selected.clone()
                } else {
                    vec![index]
                };
                edit::set_notes_velocity(&mut project, region, &targets, velocity);
                if project != self.project {
                    self.project = project;
                    self.sync_plan();
                }
                self.remember_note(region, index);
            }
            EditorAction::DragEnd => {
                if let Some(Drag::Notes {
                    region, primary, ..
                }) = &self.drag
                {
                    let (region, primary) = (*region, *primary);
                    self.remember_note(region, primary);
                }
                self.finish_drag();
            }
            EditorAction::KeyDown(pitch) => {
                self.release_key();
                let velocity = self.note_template().1;
                self.send(EngineCommand::Preview {
                    track,
                    pitch,
                    velocity,
                });
                self.editor.held_key = Some((track, pitch));
            }
            EditorAction::KeyUp => self.release_key(),
            EditorAction::Locate(tick) => self.locate(tick),
            EditorAction::Scroll { ticks, pixels } => {
                self.editor.scroll_tick = (self.editor.scroll_tick + ticks).max(0.0);
                self.editor.scroll_y = (self.editor.scroll_y + pixels).clamp(0.0, 127.0 * ROW);
            }
            EditorAction::Zoom(factor) => {
                self.editor.pixels_per_tick = (self.editor.pixels_per_tick * factor)
                    .clamp(MIN_PIXELS_PER_TICK, MAX_PIXELS_PER_TICK);
            }
        }
    }

    fn release_key(&mut self) {
        if let Some((track, pitch)) = self.editor.held_key.take() {
            self.send(EngineCommand::Preview {
                track,
                pitch,
                velocity: 0,
            });
        }
    }

    fn update_note_drag(&mut self, pointer: Pos2, track: Id) {
        let Some(Drag::Notes {
            origin,
            region,
            primary,
            anchor,
            resize,
        }) = &self.drag
        else {
            return;
        };
        let (region, primary, anchor, resize) = (*region, *primary, *anchor, *resize);
        let Some((_, original)) = origin.find_region(region) else {
            return;
        };
        let Some(note) = original.notes().get(primary).copied() else {
            return;
        };
        let region_start = original.start_tick;
        let mut project = origin.clone();
        let selected = self.editor.selected.clone();
        let delta_ticks = ((pointer.x - anchor.x) / self.editor.pixels_per_tick) as i64;
        if resize {
            let end = region_start + note.end_tick();
            let wanted = (end as i64 + delta_ticks).max(0) as u64;
            let snapped = self
                .snap_tick(wanted)
                .max(region_start + note.start_tick + 1);
            edit::resize_notes(&mut project, region, &selected, snapped as i64 - end as i64);
        } else {
            let note_start = region_start + note.start_tick;
            let wanted = (note_start as i64 + delta_ticks).max(region_start as i64) as u64;
            let delta = self.snap_tick(wanted).max(region_start) as i64 - note_start as i64;
            let delta_pitch = ((anchor.y - pointer.y) / ROW).round() as i32;
            edit::move_notes(&mut project, region, &selected, delta, delta_pitch);
        }
        if project == self.project {
            return;
        }
        self.project = project;
        self.sync_plan();
        let moved_pitch = self
            .project
            .find_region(region)
            .and_then(|(_, r)| r.notes().get(primary).copied());
        if let Some(moved) = moved_pitch
            && self.editor.preview.map(|(_, p, _)| p) != Some(moved.pitch)
            && !resize
        {
            self.preview(track, moved.pitch, moved.velocity);
        }
    }
}
