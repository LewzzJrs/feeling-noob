use crate::app::App;
use crate::browser::BrowserItem;
use crate::mixer::MixTarget;
use crate::timeline::track_color;
use crate::ui_kit::hint;
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2,
};
use fnoob_core::edit;
use fnoob_core::model::{Id, RegionContent, TrackKind};
use fnoob_core::session::{self, FollowAction, LaunchMode, SessionClip, clip as clip_at};
use fnoob_core::time::ticks_per_bar;
use fnoob_engine::Command as EngineCommand;
use fnoob_engine::session::SessionStatus;
use std::path::PathBuf;
use std::sync::Arc;

const CELL_W: f32 = 132.0;
const CELL_H: f32 = 26.0;
const LAUNCH_W: f32 = 22.0;
const HEADER_H: f32 = 26.0;
const SCENE_W: f32 = 150.0;
const IO_H: f32 = 62.0;
const STRIP_H: f32 = 196.0;
const FADER_H: f32 = 90.0;
const SPACER_MIN: f32 = 60.0;
const PLAYING: Color32 = Color32::from_rgb(90, 200, 110);

#[derive(Default)]
pub struct SessionUi {
    pub open: bool,
    pub selected: Option<Id>,
    held: Option<(Id, usize)>,
    renaming: Option<(usize, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClipDrag {
    track: Id,
    scene: usize,
}

enum GridAction {
    Launch(Id, usize),
    Release(Id, usize),
    StopTrack(Id),
    StopAll,
    LaunchScene(usize),
    Select(Id, Id),
    SelectTrack(Id),
    FocusTitle(Id, bool, bool),
    Edit(Id),
    NewClip(Id, usize),
    Move(ClipDrag, Id, usize),
    Browser(BrowserItem, Id, usize),
    Duplicate(Id, usize),
    Delete(Id, usize),
    ToTimeline(Id),
    InsertScene(usize),
    DuplicateScene(usize),
    DeleteScene(usize),
    NewTrackFrom(BrowserItem),
    ShowReturn(usize),
    RenameScene(usize, String),
}

fn dim(color: Color32, amount: f32) -> Color32 {
    color.gamma_multiply(amount)
}

fn triangle(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = 5.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x - s * 0.8, c.y - s),
            Pos2::new(c.x + s, c.y),
            Pos2::new(c.x - s * 0.8, c.y + s),
        ],
        color,
        Stroke::NONE,
    ));
}

fn square(painter: &egui::Painter, rect: Rect, color: Color32) {
    painter.rect_filled(
        Rect::from_center_size(rect.center(), vec2(8.0, 8.0)),
        1.0,
        color,
    );
}

impl App {
    fn clip_status(&self, index: usize) -> SessionStatus {
        self.audio
            .as_ref()
            .map_or_else(SessionStatus::default, |(_, e)| e.session_status(index))
    }

    pub(crate) fn any_clip_override(&self) -> bool {
        (0..self.project.tracks.len()).any(|i| self.clip_status(i).overrides)
    }

    pub(crate) fn session_view(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Clip Grid").strong());
            ui.separator();
            if ui.button("Stop All Clips").clicked() {
                self.send(EngineCommand::StopClips(None));
            }
            let overriding = self.any_clip_override();
            if ui
                .add_enabled(
                    overriding,
                    egui::Button::new("Back to Timeline").selected(overriding),
                )
                .on_hover_text("Stop the clips and let every track play its timeline again")
                .clicked()
            {
                self.send(EngineCommand::BackToArrangement);
            }
            if ui.button("Add Scene").clicked() {
                let at = self.project.scenes.len();
                self.edit(|p| {
                    session::insert_scene(p, at);
                });
            }
        });
        ui.label(
            RichText::new(
                "Press the triangle to launch a clip. Double-click an empty slot on an instrument track for a new clip. Drag clips to move them, and drop samples on audio tracks.",
            )
            .small()
            .weak(),
        );
        let mut actions = Vec::new();
        let grid = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| self.session_grid(ui, &mut actions));
        self.session_file_drop(ui, grid.inner_rect, grid.inner);
        if let Some((track, slot)) = self.session.held
            && !ui.input(|i| i.pointer.primary_down())
        {
            actions.push(GridAction::Release(track, slot));
        }
        for action in actions {
            self.apply_grid_action(action);
        }
    }

    fn session_grid(&mut self, ui: &mut Ui, actions: &mut Vec<GridAction>) -> Option<(Id, usize)> {
        let tracks: Vec<(Id, TrackKind, String, u8)> = self
            .project
            .tracks
            .iter()
            .map(|t| (t.id, t.kind, t.name.clone(), t.color))
            .collect();
        let scenes = self.project.scenes.len();
        let returns: Vec<String> = if self.returns_visible {
            self.project
                .returns
                .iter()
                .map(|r| r.name.clone())
                .collect()
        } else {
            Vec::new()
        };
        let io_h = if self.io_open { IO_H } else { 0.0 };
        let mixer = if self.mixer_open { io_h + STRIP_H } else { 0.0 };
        let right_w = returns.len() as f32 * CELL_W + SCENE_W;
        let width = (tracks.len() as f32 * CELL_W + SPACER_MIN + right_w).max(ui.available_width());
        let size = vec2(
            width,
            HEADER_H + (scenes as f32 + 1.0) * CELL_H + mixer + 8.0,
        );
        let (area, _) = ui.allocate_exact_size(size, Sense::hover());
        let right_x = area.right() - right_w;
        let main_x = right_x + returns.len() as f32 * CELL_W;
        let stop_y = area.top() + HEADER_H + scenes as f32 * CELL_H;
        let mixer_y = stop_y + CELL_H + 4.0;
        let painter = ui.painter_at(ui.clip_rect());
        let blink = (ui.input(|i| i.time) * 4.0) as i64 % 2 == 0;
        let base = ui.make_persistent_id("clip_grid");
        let visuals = ui.visuals().clone();
        let mut hovered_cell = None;
        let held_before = self.session.held;
        for (column, (track, kind, name, color)) in tracks.iter().enumerate() {
            let color = track_color(*color);
            let x = area.left() + column as f32 * CELL_W;
            let header =
                Rect::from_min_size(Pos2::new(x, area.top()), vec2(CELL_W - 2.0, HEADER_H - 2.0));
            let selected = self.is_title_selected(*track);
            painter.rect_filled(header, 2.0, dim(color, if selected { 0.9 } else { 0.55 }));
            painter.text(
                header.left_center() + vec2(6.0, 0.0),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            if ui
                .interact(header, base.with(("header", track)), Sense::click())
                .clicked()
            {
                let modifiers = ui.input(|i| i.modifiers);
                actions.push(GridAction::FocusTitle(
                    *track,
                    modifiers.shift,
                    modifiers.ctrl,
                ));
            }
            let status = self.clip_status(column);
            for scene in 0..scenes {
                let cell = Rect::from_min_size(
                    Pos2::new(x, area.top() + HEADER_H + scene as f32 * CELL_H),
                    vec2(CELL_W - 2.0, CELL_H - 2.0),
                );
                let launch = Rect::from_min_size(cell.min, vec2(LAUNCH_W, cell.height()));
                let body = Rect::from_min_max(Pos2::new(launch.right(), cell.top()), cell.max);
                let clip = clip_at(&self.project, *track, scene).cloned();
                let body_response = ui.interact(
                    body,
                    base.with(("body", track, scene)),
                    Sense::click_and_drag(),
                );
                let launch_response = ui.interact(
                    launch,
                    base.with(("launch", track, scene)),
                    Sense::click_and_drag(),
                );
                if body_response.contains_pointer() || launch_response.contains_pointer() {
                    hovered_cell = Some((*track, scene));
                }
                let playing = status.playing == Some(scene);
                let queued = status.queued == Some(Some(scene));
                match &clip {
                    Some(clip) => {
                        let is_selected = self.session.selected == Some(clip.region.id);
                        painter.rect_filled(
                            cell,
                            2.0,
                            dim(color, if playing { 0.85 } else { 0.45 }),
                        );
                        if is_selected {
                            painter.rect_stroke(
                                cell,
                                2.0,
                                Stroke::new(1.5, Color32::WHITE),
                                StrokeKind::Inside,
                            );
                        }
                        let glyph = if playing || (queued && blink) {
                            PLAYING
                        } else {
                            visuals.strong_text_color()
                        };
                        triangle(&painter, launch, glyph);
                        painter.text(
                            body.left_center() + vec2(2.0, 0.0),
                            Align2::LEFT_CENTER,
                            &clip.region.name,
                            FontId::proportional(12.0),
                            Color32::WHITE,
                        );
                        if playing {
                            let width = body.width() * status.progress;
                            painter.line_segment(
                                [
                                    Pos2::new(body.left(), body.bottom() - 1.0),
                                    Pos2::new(body.left() + width, body.bottom() - 1.0),
                                ],
                                Stroke::new(2.0, PLAYING),
                            );
                        }
                        let down = launch_response.is_pointer_button_down_on();
                        if down && held_before != Some((*track, scene)) {
                            actions.push(GridAction::Launch(*track, scene));
                        }
                        if body_response.clicked() {
                            actions.push(GridAction::Select(*track, clip.region.id));
                        }
                        if body_response.double_clicked() {
                            actions.push(GridAction::Edit(clip.region.id));
                        }
                        body_response.dnd_set_drag_payload(ClipDrag {
                            track: *track,
                            scene,
                        });
                        let region = clip.region.id;
                        body_response.context_menu(|ui| {
                            ui.label(RichText::new(&clip.region.name).strong());
                            if ui.button("Launch").clicked() {
                                actions.push(GridAction::Launch(*track, scene));
                                ui.close();
                            }
                            if matches!(clip.region.content, RegionContent::Midi { .. })
                                && ui.button("Edit Notes").clicked()
                            {
                                actions.push(GridAction::Edit(region));
                                ui.close();
                            }
                            if ui.button("Copy to Timeline at Playhead").clicked() {
                                actions.push(GridAction::ToTimeline(region));
                                ui.close();
                            }
                            if ui.button("Duplicate").clicked() {
                                actions.push(GridAction::Duplicate(*track, scene));
                                ui.close();
                            }
                            if ui.button("Delete").clicked() {
                                actions.push(GridAction::Delete(*track, scene));
                                ui.close();
                            }
                        });
                    }
                    None => {
                        painter.rect_filled(cell, 2.0, visuals.faint_bg_color);
                        let stop_color = if status.playing.is_some() {
                            visuals.strong_text_color()
                        } else {
                            visuals.weak_text_color()
                        };
                        square(&painter, launch, dim(stop_color, 0.7));
                        if launch_response.clicked() {
                            actions.push(GridAction::StopTrack(*track));
                        }
                        if body_response.double_clicked() && *kind == TrackKind::Instrument {
                            actions.push(GridAction::NewClip(*track, scene));
                        } else if body_response.clicked() {
                            actions.push(GridAction::SelectTrack(*track));
                        }
                        if *kind == TrackKind::Instrument {
                            body_response.context_menu(|ui| {
                                if ui.button("New MIDI Clip").clicked() {
                                    actions.push(GridAction::NewClip(*track, scene));
                                    ui.close();
                                }
                            });
                        }
                    }
                }
                for response in [&body_response, &launch_response] {
                    if response.dnd_hover_payload::<ClipDrag>().is_some()
                        || response.dnd_hover_payload::<BrowserItem>().is_some()
                    {
                        painter.rect_stroke(
                            cell,
                            2.0,
                            Stroke::new(2.0, PLAYING),
                            StrokeKind::Inside,
                        );
                    }
                    if let Some(drag) = response.dnd_release_payload::<ClipDrag>() {
                        actions.push(GridAction::Move(*drag, *track, scene));
                    }
                    if let Some(item) = response.dnd_release_payload::<BrowserItem>() {
                        actions.push(GridAction::Browser((*item).clone(), *track, scene));
                    }
                }
            }
            let stop = Rect::from_min_size(
                Pos2::new(x, area.top() + HEADER_H + scenes as f32 * CELL_H),
                vec2(CELL_W - 2.0, CELL_H - 2.0),
            );
            let stopping = status.queued == Some(None);
            painter.rect_filled(stop, 2.0, visuals.extreme_bg_color);
            let color = if status.playing.is_some() && !(stopping && blink) {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            };
            square(&painter, stop, color);
            if hint(
                ui.interact(stop, base.with(("stop", track)), Sense::click()),
                "Clip stop button: stop this track's clip.",
            )
            .clicked()
            {
                actions.push(GridAction::StopTrack(*track));
            }
            if mixer > 0.0 {
                self.grid_mixer(ui, &painter, x, mixer_y, MixTarget::Track(*track));
            }
        }
        let spacer = Rect::from_min_max(
            Pos2::new(area.left() + tracks.len() as f32 * CELL_W, area.top()),
            Pos2::new(right_x - 2.0, stop_y + CELL_H),
        );
        let spacer_response = hint(
            ui.interact(spacer, base.with("spacer"), Sense::hover()),
            "Drop samples, instruments or effects here to make a new track.",
        );
        let hovering = spacer_response.dnd_hover_payload::<BrowserItem>().is_some();
        painter.text(
            spacer.center(),
            Align2::CENTER_CENTER,
            "Drop Files and Devices Here",
            FontId::proportional(13.0),
            if hovering {
                PLAYING
            } else {
                visuals.weak_text_color()
            },
        );
        if let Some(item) = spacer_response.dnd_release_payload::<BrowserItem>() {
            actions.push(GridAction::NewTrackFrom((*item).clone()));
        }
        for (index, name) in returns.iter().enumerate() {
            let x = right_x + index as f32 * CELL_W;
            let header =
                Rect::from_min_size(Pos2::new(x, area.top()), vec2(CELL_W - 2.0, HEADER_H - 2.0));
            painter.rect_filled(header, 2.0, Color32::from_gray(150));
            painter.text(
                header.left_center() + vec2(6.0, 0.0),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(13.0),
                Color32::BLACK,
            );
            let response = hint(
                ui.interact(header, base.with(("return", index)), Sense::click()),
                "Return track: click to show its devices.",
            );
            if response.clicked() {
                actions.push(GridAction::ShowReturn(index));
            }
            if mixer > 0.0 {
                self.grid_mixer(ui, &painter, x, mixer_y, MixTarget::Return(index));
            }
        }
        let x = main_x;
        let header = Rect::from_min_size(
            Pos2::new(x, area.top()),
            vec2(SCENE_W - 2.0, HEADER_H - 2.0),
        );
        painter.rect_filled(header, 2.0, Color32::from_gray(205));
        painter.text(
            header.left_center() + vec2(6.0, 0.0),
            Align2::LEFT_CENTER,
            "Main",
            FontId::proportional(13.0),
            Color32::BLACK,
        );
        for scene in 0..scenes {
            let cell = Rect::from_min_size(
                Pos2::new(x, area.top() + HEADER_H + scene as f32 * CELL_H),
                vec2(SCENE_W - 2.0, CELL_H - 2.0),
            );
            if let Some((index, text)) = &mut self.session.renaming
                && *index == scene
            {
                let response = ui.put(cell, egui::TextEdit::singleline(text));
                response.request_focus();
                if response.lost_focus() {
                    actions.push(GridAction::RenameScene(scene, text.clone()));
                }
                continue;
            }
            let launch = Rect::from_min_size(cell.min, vec2(LAUNCH_W, cell.height()));
            painter.rect_filled(cell, 2.0, visuals.widgets.inactive.bg_fill);
            triangle(&painter, launch, visuals.strong_text_color());
            painter.text(
                Pos2::new(launch.right() + 2.0, cell.center().y),
                Align2::LEFT_CENTER,
                &self.project.scenes[scene].name,
                FontId::proportional(12.0),
                visuals.strong_text_color(),
            );
            let response = ui
                .interact(cell, base.with(("scene", scene)), Sense::click())
                .on_hover_text("Launch every clip in this row");
            if response.clicked() {
                actions.push(GridAction::LaunchScene(scene));
            }
            response.context_menu(|ui| {
                if ui.button("Launch").clicked() {
                    actions.push(GridAction::LaunchScene(scene));
                    ui.close();
                }
                if ui.button("Rename").clicked() {
                    self.session.renaming = Some((scene, self.project.scenes[scene].name.clone()));
                    ui.close();
                }
                if ui.button("Insert Scene Above").clicked() {
                    actions.push(GridAction::InsertScene(scene));
                    ui.close();
                }
                if ui.button("Insert Scene Below").clicked() {
                    actions.push(GridAction::InsertScene(scene + 1));
                    ui.close();
                }
                if ui.button("Duplicate Scene").clicked() {
                    actions.push(GridAction::DuplicateScene(scene));
                    ui.close();
                }
                if ui
                    .add_enabled(scenes > 1, egui::Button::new("Delete Scene"))
                    .clicked()
                {
                    actions.push(GridAction::DeleteScene(scene));
                    ui.close();
                }
            });
        }
        let stop_all = Rect::from_min_size(
            Pos2::new(x, area.top() + HEADER_H + scenes as f32 * CELL_H),
            vec2(SCENE_W - 2.0, CELL_H - 2.0),
        );
        painter.rect_filled(stop_all, 2.0, visuals.extreme_bg_color);
        painter.text(
            stop_all.center(),
            Align2::CENTER_CENTER,
            "Stop All Clips",
            FontId::proportional(12.0),
            visuals.strong_text_color(),
        );
        if ui
            .interact(stop_all, base.with("stop_all"), Sense::click())
            .clicked()
        {
            actions.push(GridAction::StopAll);
        }
        if mixer > 0.0 {
            self.grid_mixer(ui, &painter, x, mixer_y, MixTarget::Master);
        }
        hovered_cell
    }

    fn grid_mixer(
        &mut self,
        ui: &mut Ui,
        painter: &egui::Painter,
        x: f32,
        top: f32,
        target: MixTarget,
    ) {
        let width = if target == MixTarget::Master {
            SCENE_W
        } else {
            CELL_W
        };
        let io_h = if self.io_open { IO_H } else { 0.0 };
        if self.io_open {
            let io = Rect::from_min_size(Pos2::new(x, top), vec2(width - 2.0, IO_H - 4.0));
            painter.rect_filled(io, 2.0, ui.visuals().faint_bg_color);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(io.shrink(3.0)));
            child.set_clip_rect(io.intersect(ui.clip_rect()));
            child.vertical(|ui| {
                crate::ui_kit::compact(ui);
                match target {
                    MixTarget::Track(id) => self.track_io(ui, id),
                    MixTarget::Return(_) => {
                        ui.label(RichText::new("Audio To").small().weak());
                        ui.label("Main");
                    }
                    MixTarget::Master => self.main_io(ui),
                }
            });
        }
        let strip = Rect::from_min_size(Pos2::new(x, top + io_h), vec2(width - 2.0, STRIP_H - 4.0));
        painter.rect_filled(strip, 2.0, ui.visuals().faint_bg_color);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(strip.shrink(3.0)));
        child.set_clip_rect(strip.intersect(ui.clip_rect()));
        child.vertical(|ui| {
            crate::ui_kit::compact(ui);
            self.mix_strip(ui, target, FADER_H);
        });
    }

    fn session_file_drop(&mut self, ui: &Ui, area: Rect, cell: Option<(Id, usize)>) {
        let (hovering, dropped) =
            ui.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
        if hovering {
            ui.painter_at(area).text(
                Pos2::new(area.center().x, area.bottom() - 16.0),
                Align2::CENTER_BOTTOM,
                "Drop WAV files on an audio track's slot, or anywhere else for a new audio track",
                FontId::proportional(15.0),
                ui.visuals().strong_text_color(),
            );
        }
        let paths: Vec<PathBuf> = dropped
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| fnoob_engine::decode::is_audio_file(p))
            .collect();
        if paths.is_empty() {
            if !dropped.is_empty() {
                self.message = Some("Only audio files can be dropped on the clip grid.".into());
            }
            return;
        }
        let (track, scene) = cell.map_or((None, 0), |(t, s)| (Some(t), s));
        self.import_to_slots(&paths, track, scene);
    }

    pub(crate) fn import_to_slots(&mut self, sources: &[PathBuf], track: Option<Id>, scene: usize) {
        let mut ready = Vec::new();
        for source in sources {
            if let Some(item) = self.copy_into_project(source) {
                ready.push(item);
            } else {
                return;
            }
        }
        if ready.is_empty() {
            return;
        }
        let specs: Vec<(String, u16, u64)> = ready
            .iter()
            .map(|(rel, data)| (rel.clone(), data.channels, data.frames))
            .collect();
        let mut files = Vec::new();
        let mut placed = None;
        self.edit(|p| {
            let track = track
                .filter(|id| p.track(*id).is_some_and(|t| t.kind == TrackKind::Audio))
                .unwrap_or_else(|| edit::add_track(p, TrackKind::Audio));
            let mut scene = scene;
            for (rel, channels, frames) in &specs {
                let file = edit::add_audio_file(p, rel.clone(), *channels, *frames);
                files.push(file);
                while scene < p.scenes.len() && clip_at(p, track, scene).is_some() {
                    scene += 1;
                }
                if scene >= p.scenes.len() {
                    session::insert_scene(p, p.scenes.len());
                }
                if let Some(id) = session::add_audio_clip(p, track, scene, file) {
                    placed = Some((track, id));
                }
                scene += 1;
            }
        });
        for ((_, data), file) in ready.into_iter().zip(files) {
            self.pool.insert(file, Arc::new(data));
        }
        self.sync_plan();
        if let Some((track, clip)) = placed {
            self.selection.track = Some(track);
            self.session.selected = Some(clip);
        }
    }

    fn apply_grid_action(&mut self, action: GridAction) {
        match action {
            GridAction::Launch(track, slot) => {
                if self.rate_matches() {
                    self.session.held = Some((track, slot));
                    self.send(EngineCommand::LaunchClip { track, slot });
                }
            }
            GridAction::Release(track, slot) => {
                self.session.held = None;
                self.send(EngineCommand::ReleaseClip { track, slot });
            }
            GridAction::StopAll => self.send(EngineCommand::StopClips(None)),
            GridAction::StopTrack(track) => self.send(EngineCommand::StopClips(Some(track))),
            GridAction::LaunchScene(scene) => {
                if self.rate_matches() {
                    self.send(EngineCommand::LaunchScene(scene));
                }
            }
            GridAction::Select(track, clip) => {
                self.selection.track = Some(track);
                self.selection.regions.clear();
                self.session.selected = Some(clip);
                if self.detail_visible() {
                    self.show_clip();
                }
            }
            GridAction::SelectTrack(track) => {
                self.selection.track = Some(track);
                self.session.selected = None;
                self.header_focus = false;
            }
            GridAction::FocusTitle(track, shift, ctrl) => {
                self.session.selected = None;
                self.focus_title(track, shift, ctrl);
            }
            GridAction::Edit(clip) => {
                let midi = self
                    .project
                    .find_region(clip)
                    .is_some_and(|(_, r)| matches!(r.content, RegionContent::Midi { .. }));
                self.session.selected = Some(clip);
                if midi {
                    self.open_editor(clip);
                }
            }
            GridAction::NewClip(track, scene) => {
                let length = ticks_per_bar(self.project.time_signature);
                let mut created = None;
                self.edit(|p| created = session::add_midi_clip(p, track, scene, length));
                if let Some(clip) = created {
                    self.selection.track = Some(track);
                    self.session.selected = Some(clip);
                    self.open_editor(clip);
                }
            }
            GridAction::Move(drag, track, scene) => {
                let mut moved = false;
                self.edit(|p| {
                    moved = session::move_clip(p, (drag.track, drag.scene), (track, scene))
                });
                if !moved && (drag.track, drag.scene) != (track, scene) {
                    self.message = Some(
                        "Clips move only to an empty slot on a track of the same kind.".into(),
                    );
                }
            }
            GridAction::Browser(item, track, scene) => match item {
                BrowserItem::Sample(path) => self.import_to_slots(&[path], Some(track), scene),
                other => {
                    let tick = self.playhead_tick();
                    self.apply_browser_item(&other, Some(track), tick);
                }
            },
            GridAction::Duplicate(track, scene) => {
                let mut created = None;
                self.edit(|p| created = session::duplicate_clip(p, track, scene));
                if created.is_some() {
                    self.session.selected = created;
                }
            }
            GridAction::Delete(track, scene) => {
                self.edit(|p| {
                    session::delete_clip(p, track, scene);
                });
                self.session.selected = None;
            }
            GridAction::ToTimeline(clip) => {
                let tick = self.playhead_tick();
                let mut placed = None;
                self.edit(|p| placed = session::copy_to_arrangement(p, clip, tick));
                if let Some(region) = placed {
                    self.message = Some("Copied the clip to the timeline at the playhead.".into());
                    self.selection.regions = vec![region];
                }
            }
            GridAction::InsertScene(index) => self.edit(|p| {
                session::insert_scene(p, index);
            }),
            GridAction::DuplicateScene(index) => self.edit(|p| {
                session::duplicate_scene(p, index);
            }),
            GridAction::DeleteScene(index) => self.edit(|p| {
                session::delete_scene(p, index);
            }),
            GridAction::NewTrackFrom(item) => match item {
                BrowserItem::Sample(path) => self.import_to_slots(&[path], None, 0),
                other => {
                    let tick = self.playhead_tick();
                    self.apply_browser_item(&other, None, tick);
                }
            },
            GridAction::ShowReturn(index) => {
                self.chain_target = crate::chain::ChainTarget::Return(index);
                self.show_devices();
            }
            GridAction::RenameScene(index, name) => {
                self.session.renaming = None;
                let name = name.trim().to_owned();
                if !name.is_empty() {
                    self.edit(|p| {
                        if let Some(scene) = p.scenes.get_mut(index) {
                            scene.name = name;
                        }
                    });
                }
            }
        }
    }

    pub(crate) fn launch_selected_clip(&mut self) -> bool {
        let Some((track, scene)) = self
            .session
            .selected
            .and_then(|id| session::clip_position(&self.project, id))
        else {
            return false;
        };
        if self.rate_matches() {
            self.send(EngineCommand::LaunchClip { track, slot: scene });
        }
        true
    }

    pub(crate) fn insert_midi_clip(&mut self) {
        let Some(track) = self.selection.track.filter(|id| {
            self.project
                .track(*id)
                .is_some_and(|t| t.kind == TrackKind::Instrument)
        }) else {
            self.message = Some("Select a MIDI track first.".into());
            return;
        };
        let length = ticks_per_bar(self.project.time_signature);
        if self.session.open {
            let scene = (0..self.project.scenes.len())
                .find(|&s| clip_at(&self.project, track, s).is_none());
            let Some(scene) = scene else {
                self.message = Some("This track has no empty slot. Insert a scene first.".into());
                return;
            };
            self.apply_grid_action(GridAction::NewClip(track, scene));
        } else {
            let meter = self.project.meter();
            let tick = meter.bar_floor(self.playhead_tick());
            let mut created = None;
            self.edit(|p| created = edit::add_midi_region(p, track, tick, length));
            if let Some(id) = created {
                self.selection.regions = vec![id];
                self.open_editor(id);
            }
        }
    }

    pub(crate) fn delete_selected_clip(&mut self) -> bool {
        let Some((track, scene)) = self
            .session
            .selected
            .and_then(|id| session::clip_position(&self.project, id))
        else {
            return false;
        };
        self.apply_grid_action(GridAction::Delete(track, scene));
        true
    }

    pub(crate) fn duplicate_selected_clip(&mut self) -> bool {
        let Some((track, scene)) = self
            .session
            .selected
            .and_then(|id| session::clip_position(&self.project, id))
        else {
            return false;
        };
        self.apply_grid_action(GridAction::Duplicate(track, scene));
        true
    }

    pub(crate) fn send_region_to_grid(&mut self, region: Id) {
        let Some((track, _)) = self.project.find_region(region) else {
            return;
        };
        let track = track.id;
        let mut placed = None;
        self.edit(|p| {
            let scene = (0..p.scenes.len())
                .find(|&s| clip_at(p, track, s).is_none())
                .unwrap_or_else(|| {
                    session::insert_scene(p, p.scenes.len());
                    p.scenes.len() - 1
                });
            placed = session::copy_from_arrangement(p, region, scene);
        });
        if placed.is_some() {
            self.session.selected = placed;
            self.message = Some("Copied the region to the clip grid.".into());
        }
    }

    pub(crate) fn clip_section(&mut self, ui: &mut Ui, clip: &SessionClip) {
        let id = clip.region.id;
        let Some((track, scene)) = session::clip_position(&self.project, id) else {
            return;
        };
        let update =
            |app: &mut Self, response: &egui::Response, change: &dyn Fn(&mut SessionClip)| {
                app.live_edit(response, |p| {
                    if let Some(c) = session::clip_mut(p, track, scene) {
                        change(c);
                    }
                });
            };
        let mut name = clip.region.name.clone();
        let response = ui.add(egui::TextEdit::singleline(&mut name).hint_text("Clip name"));
        if std::mem::take(&mut self.inspector.focus_name) {
            response.request_focus();
        }
        update(self, &response, &|c| c.region.name = name.clone());
        let mut looping = clip.looping;
        let response = ui.checkbox(&mut looping, "Loop");
        update(self, &response, &|c| c.looping = looping);
        let beat = fnoob_core::time::ticks_per_beat(self.project.time_signature) as f64;
        let mut beats = clip.region.length_tick as f64 / beat;
        let response = ui.add(
            egui::DragValue::new(&mut beats)
                .range(0.25..=1024.0)
                .speed(0.25)
                .prefix("Length ")
                .suffix(" beats"),
        );
        update(self, &response, &|c| {
            c.region.length_tick = ((beats * beat).round() as u64).max(1);
        });
        if let RegionContent::Audio { gain_db, .. } = clip.region.content {
            let mut gain = gain_db;
            let response = ui.add(
                egui::Slider::new(
                    &mut gain,
                    fnoob_core::model::MIN_GAIN_DB..=fnoob_core::model::MAX_GAIN_DB,
                )
                .text("Gain")
                .suffix(" dB"),
            );
            update(self, &response, &|c| {
                if let RegionContent::Audio { gain_db, .. } = &mut c.region.content {
                    *gain_db = gain;
                }
            });
        }
        let mut launch = clip.launch;
        egui::ComboBox::from_label("Launch mode")
            .selected_text(launch.label())
            .show_ui(ui, |ui| {
                for mode in LaunchMode::ALL {
                    ui.selectable_value(&mut launch, mode, mode.label());
                }
            });
        if launch != clip.launch {
            self.edit(|p| {
                if let Some(c) = session::clip_mut(p, track, scene) {
                    c.launch = launch;
                }
            });
        }
        ui.separator();
        ui.label(RichText::new("Follow action").strong());
        let bar = ticks_per_bar(self.project.time_signature) as f64;
        let mut bars = clip.follow.after_tick as f64 / bar;
        let response = ui.add(
            egui::DragValue::new(&mut bars)
                .range(0.0..=256.0)
                .speed(0.25)
                .prefix("After ")
                .suffix(" bars (0 = off)"),
        );
        update(self, &response, &|c| {
            c.follow.after_tick = (bars * bar).round() as u64
        });
        let mut follow = clip.follow;
        for (label, value) in [("Action A", &mut follow.a), ("Action B", &mut follow.b)] {
            egui::ComboBox::from_label(label)
                .selected_text(value.label())
                .show_ui(ui, |ui| {
                    for action in FollowAction::ALL {
                        ui.selectable_value(value, action, action.label());
                    }
                });
        }
        if follow.a != clip.follow.a || follow.b != clip.follow.b {
            self.edit(|p| {
                if let Some(c) = session::clip_mut(p, track, scene) {
                    c.follow.a = follow.a;
                    c.follow.b = follow.b;
                }
            });
        }
        let mut chance = clip.follow.chance_a * 100.0;
        let response = ui.add(
            egui::Slider::new(&mut chance, 0.0..=100.0)
                .text("Chance of A")
                .suffix(" %"),
        );
        update(self, &response, &|c| c.follow.chance_a = chance / 100.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Launch").clicked() {
                self.apply_grid_action(GridAction::Launch(track, scene));
                self.session.held = None;
            }
            if matches!(clip.region.content, RegionContent::Midi { .. })
                && ui.button("Edit Notes").clicked()
            {
                self.apply_grid_action(GridAction::Edit(id));
            }
            if ui.button("Copy to Timeline").clicked() {
                self.apply_grid_action(GridAction::ToTimeline(id));
            }
            if ui.button("Duplicate").clicked() {
                self.apply_grid_action(GridAction::Duplicate(track, scene));
            }
            if ui.button("Delete").clicked() {
                self.apply_grid_action(GridAction::Delete(track, scene));
            }
        });
    }
}
