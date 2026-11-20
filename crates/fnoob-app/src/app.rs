use crate::keys::take_commands;
use crate::timeline::{LANE_HEIGHT, MAX_PIXELS_PER_TICK, MIN_PIXELS_PER_TICK};
use eframe::egui::{self, Color32, Pos2, RichText};
use fnoob_core::edit;
use fnoob_core::history::History;
use fnoob_core::io::{self, PROJECT_EXTENSION, PROJECT_FILE};
use fnoob_core::keymap::{Command, shortcut_label};
use fnoob_core::model::{Grid, Id, Project, Region, Track, TrackKind};
use fnoob_core::settings::{Settings, default_settings_path};
use fnoob_core::time::{
    MAX_BPM, MIN_BPM, Position, Timeline, grid_ticks, snap_floor, ticks_per_bar,
};
use fnoob_engine::device::{AudioOutput, OutputDeviceInfo, list_output_devices, open_output};
use fnoob_engine::{Command as EngineCommand, EngineHandle, RenderPlan, TransportState};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DENOMINATORS: [u32; 4] = [2, 4, 8, 16];

pub enum Drag {
    Regions {
        origin: Project,
        primary: Id,
        anchor: Pos2,
    },
    Loop {
        origin: Project,
        anchor: u64,
    },
}

#[derive(Default)]
pub struct Selection {
    pub track: Option<Id>,
    pub regions: Vec<Id>,
}

pub struct View {
    pub pixels_per_tick: f32,
    pub scroll_tick: f32,
    pub scroll_y: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    New,
    Open,
    Quit,
}

struct NewTracksDialog {
    kind: TrackKind,
    count: u32,
}

pub struct App {
    settings: Settings,
    settings_path: Option<PathBuf>,
    devices: Vec<OutputDeviceInfo>,
    audio: Option<(AudioOutput, EngineHandle)>,
    pub(crate) project: Project,
    saved: Project,
    project_dir: Option<PathBuf>,
    pub(crate) history: History,
    pub(crate) selection: Selection,
    clipboard: Vec<Region>,
    pub(crate) view: View,
    pub(crate) drag: Option<Drag>,
    pub(crate) renaming: Option<(Id, String)>,
    live_origin: Option<Project>,
    stopped_tick: u64,
    show_settings: bool,
    new_tracks: Option<NewTracksDialog>,
    pending: Option<Pending>,
    allow_close: bool,
    title: String,
    message: Option<String>,
}

impl App {
    pub fn new(open: Option<PathBuf>) -> Self {
        let settings_path = default_settings_path();
        let mut message = None;
        let settings = match settings_path.as_deref().map(Settings::load) {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                message = Some(format!("{e}. Using default settings."));
                Settings::default()
            }
            None => {
                message = Some("APPDATA is not set. Settings will not be saved.".into());
                Settings::default()
            }
        };
        let project = Project::new("Untitled", DEFAULT_SAMPLE_RATE);
        let mut app = Self {
            settings,
            settings_path,
            devices: Vec::new(),
            audio: None,
            saved: project.clone(),
            project,
            project_dir: None,
            history: History::default(),
            selection: Selection::default(),
            clipboard: Vec::new(),
            view: View {
                pixels_per_tick: 0.04,
                scroll_tick: 0.0,
                scroll_y: 0.0,
            },
            drag: None,
            renaming: None,
            live_origin: None,
            stopped_tick: 0,
            show_settings: false,
            new_tracks: None,
            pending: None,
            allow_close: false,
            title: String::new(),
            message,
        };
        app.refresh_devices();
        app.open_device();
        if let Some((output, _)) = &app.audio {
            app.project.sample_rate = output.sample_rate;
            app.saved = app.project.clone();
        }
        if let Some(dir) = open {
            app.open_dir(dir);
        }
        app
    }

    fn sample_rate(&self) -> u32 {
        self.audio
            .as_ref()
            .map_or(self.project.sample_rate, |(o, _)| o.sample_rate)
    }

    fn timeline(&self) -> Timeline {
        Timeline {
            sample_rate: self.sample_rate(),
            bpm: self.project.tempo_bpm,
        }
    }

    fn is_dirty(&self) -> bool {
        self.project != self.saved
    }

    pub(crate) fn playhead_tick(&self) -> u64 {
        match &self.audio {
            Some((_, engine)) => self.timeline().sample_to_tick(engine.position()),
            None => self.stopped_tick,
        }
    }

    fn transport_state(&self) -> TransportState {
        self.audio
            .as_ref()
            .map_or(TransportState::Stopped, |(_, e)| e.state())
    }

    fn refresh_devices(&mut self) {
        match list_output_devices() {
            Ok(devices) => self.devices = devices,
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    fn open_device(&mut self) {
        let tick = self.playhead_tick();
        self.audio = None;
        let project = &self.project;
        match open_output(self.settings.output_device.as_deref(), |rate| {
            RenderPlan::from_project(project, rate)
        }) {
            Ok(audio) => self.audio = Some(audio),
            Err(e) => self.message = Some(e.to_string()),
        }
        self.locate(tick);
        self.check_sample_rate();
    }

    fn check_sample_rate(&mut self) {
        if let Some((output, _)) = &self.audio
            && output.sample_rate != self.project.sample_rate
        {
            self.message = Some(format!(
                "Project sample rate is {} Hz but the device runs at {} Hz.",
                self.project.sample_rate, output.sample_rate
            ));
        }
    }

    fn select_device(&mut self, id: Option<String>) {
        if self.settings.output_device == id {
            return;
        }
        self.settings.output_device = id;
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            self.message = Some(e.to_string());
        }
        self.open_device();
    }

    fn send(&mut self, command: EngineCommand) {
        if let Some((_, engine)) = &mut self.audio
            && engine.send(command).is_err()
        {
            self.message = Some("Audio engine is not responding.".into());
        }
    }

    pub(crate) fn sync_plan(&mut self) {
        let rate = self.sample_rate();
        let plan = Box::new(RenderPlan::from_project(&self.project, rate));
        self.send(EngineCommand::SetPlan(plan));
    }

    pub(crate) fn locate(&mut self, tick: u64) {
        self.stopped_tick = tick;
        let sample = self.timeline().tick_to_sample(tick);
        self.send(EngineCommand::Locate(sample));
    }

    pub(crate) fn edit(&mut self, change: impl FnOnce(&mut Project)) {
        let before = self.project.clone();
        change(&mut self.project);
        if self.project != before {
            self.history.record(before);
            self.sync_plan();
        }
    }

    pub(crate) fn edit_track(&mut self, id: Id, change: impl FnOnce(&mut Track)) {
        self.edit(|p| {
            if let Some(track) = p.track_mut(id) {
                change(track);
            }
        });
    }

    fn live_edit(&mut self, response: &egui::Response, change: impl FnOnce(&mut Project)) {
        if response.changed() {
            if self.live_origin.is_none() {
                self.live_origin = Some(self.project.clone());
            }
            change(&mut self.project);
            self.sync_plan();
        }
        if (response.drag_stopped()
            || response.lost_focus()
            || !response.has_focus() && !response.dragged())
            && let Some(origin) = self.live_origin.take()
            && origin != self.project
        {
            self.history.record(origin);
        }
    }

    fn after_history_jump(&mut self) {
        let ids = edit::all_region_ids(&self.project);
        self.selection.regions.retain(|id| ids.contains(id));
        if let Some(track) = self.selection.track
            && self.project.track(track).is_none()
        {
            self.selection.track = None;
        }
        self.sync_plan();
    }

    fn selected_or_first_instrument(&self) -> Option<Id> {
        self.selection
            .track
            .filter(|id| {
                self.project
                    .track(*id)
                    .is_some_and(|t| t.kind == TrackKind::Instrument)
            })
            .or_else(|| {
                self.project
                    .tracks
                    .iter()
                    .find(|t| t.kind == TrackKind::Instrument)
                    .map(|t| t.id)
            })
    }

    fn select_track_offset(&mut self, offset: i64) {
        if self.project.tracks.is_empty() {
            return;
        }
        let current = self
            .selection
            .track
            .and_then(|id| self.project.tracks.iter().position(|t| t.id == id));
        let next = match current {
            Some(i) => (i as i64 + offset).clamp(0, self.project.tracks.len() as i64 - 1) as usize,
            None => 0,
        };
        self.selection.track = Some(self.project.tracks[next].id);
        let top = next as f32 * LANE_HEIGHT;
        if top < self.view.scroll_y {
            self.view.scroll_y = top;
        }
    }

    fn not_yet(&mut self, what: &str) {
        self.message = Some(format!("{what} is not available yet."));
    }

    pub(crate) fn run(&mut self, command: Command) {
        let regions = self.selection.regions.clone();
        let bar = ticks_per_bar(self.project.time_signature);
        let grid = grid_ticks(self.project.snap.grid, self.project.time_signature) as i64;
        match command {
            Command::PlayStop => {
                if self.transport_state() == TransportState::Stopped {
                    self.send(EngineCommand::Play);
                } else {
                    self.send(EngineCommand::Stop);
                }
            }
            Command::Record => {
                if self.transport_state() == TransportState::Stopped {
                    self.send(EngineCommand::Record);
                    self.message = Some(
                        "Recording is not available yet. Transport runs with count-in only.".into(),
                    );
                } else {
                    self.send(EngineCommand::Stop);
                }
            }
            Command::GoToBeginning => self.locate(0),
            Command::Rewind => {
                let tick = self.playhead_tick();
                self.locate(snap_floor(tick.saturating_sub(1), bar));
            }
            Command::Forward => {
                let tick = self.playhead_tick();
                self.locate(snap_floor(tick, bar) + bar);
            }
            Command::ToggleLoop => self.edit(|p| p.loop_range.enabled = !p.loop_range.enabled),
            Command::ToggleMetronome => self.edit(|p| p.metronome.enabled = !p.metronome.enabled),
            Command::ToggleCountIn => {
                self.edit(|p| p.metronome.count_in_enabled = !p.metronome.count_in_enabled);
            }
            Command::ToggleMixer => self.not_yet("The mixer"),
            Command::TogglePianoRoll | Command::ToggleEditor => self.not_yet("The piano roll"),
            Command::Quantize => self.not_yet("Quantize"),
            Command::Bounce => self.not_yet("Bounce"),
            Command::ToggleTrackMute => {
                if let Some(id) = self.selection.track {
                    self.edit_track(id, |t| t.mute = !t.mute);
                }
            }
            Command::ToggleTrackSolo => {
                if let Some(id) = self.selection.track {
                    self.edit_track(id, |t| t.solo = !t.solo);
                }
            }
            Command::ToggleTrackRecord => {
                if let Some(id) = self.selection.track {
                    self.edit_track(id, |t| t.record_enabled = !t.record_enabled);
                }
            }
            Command::NewTracks => {
                self.new_tracks = Some(NewTracksDialog {
                    kind: TrackKind::Instrument,
                    count: 1,
                });
            }
            Command::SplitAtPlayhead => {
                let tick = self.playhead_tick();
                let mut created = Vec::new();
                self.edit(|p| created = edit::split_regions_at(p, &regions, tick));
                self.selection.regions.extend(created);
            }
            Command::RepeatRegions => {
                let mut created = Vec::new();
                self.edit(|p| created = edit::repeat_regions(p, &regions));
                if !created.is_empty() {
                    self.selection.regions = created;
                }
            }
            Command::ToggleSnap => self.edit(|p| p.snap.enabled = !p.snap.enabled),
            Command::MuteRegions => self.edit(|p| edit::toggle_regions_muted(p, &regions)),
            Command::NudgeLeft => self.edit(|p| {
                edit::nudge_regions(p, &regions, -grid);
            }),
            Command::NudgeRight => self.edit(|p| {
                edit::nudge_regions(p, &regions, grid);
            }),
            Command::Undo => {
                if self.history.undo(&mut self.project) {
                    self.after_history_jump();
                }
            }
            Command::Redo => {
                if self.history.redo(&mut self.project) {
                    self.after_history_jump();
                }
            }
            Command::Copy => self.clipboard = edit::copy_regions(&self.project, &regions),
            Command::Cut => {
                self.clipboard = edit::copy_regions(&self.project, &regions);
                self.edit(|p| {
                    edit::delete_regions(p, &regions);
                });
                self.selection.regions.clear();
            }
            Command::Paste => match self.selected_or_first_instrument() {
                Some(track) if !self.clipboard.is_empty() => {
                    let tick = self.playhead_tick();
                    let clip = self.clipboard.clone();
                    let mut created = Vec::new();
                    self.edit(|p| created = edit::paste_regions(p, track, tick, &clip));
                    self.selection.track = Some(track);
                    self.selection.regions = created;
                }
                Some(_) => {}
                None => self.message = Some("Paste needs an instrument track.".into()),
            },
            Command::SelectAll => self.selection.regions = edit::all_region_ids(&self.project),
            Command::Delete => {
                self.edit(|p| {
                    edit::delete_regions(p, &regions);
                });
                self.selection.regions.clear();
            }
            Command::ZoomOut => self.zoom(0.8),
            Command::ZoomIn => self.zoom(1.25),
            Command::SelectPreviousTrack => self.select_track_offset(-1),
            Command::SelectNextTrack => self.select_track_offset(1),
            Command::NewProject => self.request(Pending::New),
            Command::Open => self.request(Pending::Open),
            Command::Save => {
                self.save();
            }
            Command::SaveAs => {
                self.save_as();
            }
        }
    }

    fn request(&mut self, pending: Pending) {
        if self.is_dirty() {
            self.pending = Some(pending);
        } else {
            self.proceed(pending);
        }
    }

    fn proceed(&mut self, pending: Pending) {
        match pending {
            Pending::New => {
                let project = Project::new("Untitled", self.sample_rate());
                self.replace_project(project, None);
            }
            Pending::Open => self.open(),
            Pending::Quit => self.allow_close = true,
        }
    }

    fn replace_project(&mut self, project: Project, dir: Option<PathBuf>) {
        self.send(EngineCommand::Stop);
        self.saved = project.clone();
        self.project = project;
        self.project_dir = dir;
        self.history.clear();
        self.selection = Selection::default();
        self.drag = None;
        self.renaming = None;
        self.live_origin = None;
        self.view.scroll_tick = 0.0;
        self.view.scroll_y = 0.0;
        self.sync_plan();
        self.locate(0);
        self.check_sample_rate();
    }

    fn open(&mut self) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title("Open project folder (.fnoob)")
            .pick_folder()
        else {
            return;
        };
        self.open_dir(dir);
    }

    fn open_dir(&mut self, dir: PathBuf) {
        if !dir.join(PROJECT_FILE).is_file() {
            self.message = Some(format!("{} has no {PROJECT_FILE}.", dir.display()));
            return;
        }
        match io::load(&dir) {
            Ok(project) => {
                self.message = None;
                self.replace_project(project, Some(dir));
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    fn save(&mut self) -> bool {
        match self.project_dir.clone() {
            Some(dir) => self.save_to(&dir),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Save project as")
            .set_file_name(format!("{}.{PROJECT_EXTENSION}", self.project.name))
            .save_file()
        else {
            return false;
        };
        let dir = path.with_extension(PROJECT_EXTENSION);
        if dir.is_file() {
            self.message = Some(format!(
                "{} is a file, not a project folder.",
                dir.display()
            ));
            return false;
        }
        if let Some(name) = dir.file_stem().and_then(|s| s.to_str()) {
            self.project.name = name.to_owned();
        }
        if self.save_to(&dir) {
            self.project_dir = Some(dir);
            true
        } else {
            false
        }
    }

    fn save_to(&mut self, dir: &Path) -> bool {
        match io::save(dir, &self.project) {
            Ok(()) => {
                self.saved = self.project.clone();
                self.message = Some(format!("Saved {}", dir.display()));
                true
            }
            Err(e) => {
                self.message = Some(format!("Save failed: {e}"));
                false
            }
        }
    }

    fn menu_item(&mut self, ui: &mut egui::Ui, text: &str, command: Command) {
        let mut button = egui::Button::new(text);
        if let Some(label) = shortcut_label(command) {
            button = button.shortcut_text(label);
        }
        if ui.add(button).clicked() {
            self.run(command);
            ui.close();
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                self.menu_item(ui, "New Project", Command::NewProject);
                self.menu_item(ui, "Open...", Command::Open);
                self.menu_item(ui, "Save", Command::Save);
                self.menu_item(ui, "Save As...", Command::SaveAs);
                ui.separator();
                if ui.button("Audio Settings...").clicked() {
                    self.show_settings = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                self.menu_item(ui, "Undo", Command::Undo);
                self.menu_item(ui, "Redo", Command::Redo);
                ui.separator();
                self.menu_item(ui, "Cut", Command::Cut);
                self.menu_item(ui, "Copy", Command::Copy);
                self.menu_item(ui, "Paste", Command::Paste);
                self.menu_item(ui, "Delete", Command::Delete);
                self.menu_item(ui, "Select All", Command::SelectAll);
                ui.separator();
                self.menu_item(ui, "Split at Playhead", Command::SplitAtPlayhead);
                self.menu_item(ui, "Repeat", Command::RepeatRegions);
                self.menu_item(ui, "Mute Regions", Command::MuteRegions);
                self.menu_item(ui, "Nudge Left", Command::NudgeLeft);
                self.menu_item(ui, "Nudge Right", Command::NudgeRight);
            });
            ui.menu_button("Track", |ui| {
                self.menu_item(ui, "New Tracks...", Command::NewTracks);
                let selected = self.selection.track;
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Delete Track"))
                    .clicked()
                {
                    if let Some(id) = selected {
                        self.edit(|p| {
                            edit::delete_track(p, id);
                        });
                        self.selection.track = None;
                        self.selection.regions.clear();
                    }
                    ui.close();
                }
                ui.separator();
                self.menu_item(ui, "Mute Track", Command::ToggleTrackMute);
                self.menu_item(ui, "Solo Track", Command::ToggleTrackSolo);
                self.menu_item(ui, "Record Enable Track", Command::ToggleTrackRecord);
            });
            ui.menu_button("Transport", |ui| {
                self.menu_item(ui, "Play / Stop", Command::PlayStop);
                self.menu_item(ui, "Record", Command::Record);
                self.menu_item(ui, "Go to Beginning", Command::GoToBeginning);
                self.menu_item(ui, "Rewind One Bar", Command::Rewind);
                self.menu_item(ui, "Forward One Bar", Command::Forward);
                ui.separator();
                self.menu_item(ui, "Loop", Command::ToggleLoop);
                self.menu_item(ui, "Metronome", Command::ToggleMetronome);
                self.menu_item(ui, "Count-in", Command::ToggleCountIn);
            });
            ui.menu_button("View", |ui| {
                self.menu_item(ui, "Zoom In", Command::ZoomIn);
                self.menu_item(ui, "Zoom Out", Command::ZoomOut);
                self.menu_item(ui, "Snap to Grid", Command::ToggleSnap);
            });
        });
    }

    fn toggle_button(ui: &mut egui::Ui, on: bool, text: &str, tip: &str) -> bool {
        ui.selectable_label(on, text).on_hover_text(tip).clicked()
    }

    fn control_bar(&mut self, ui: &mut egui::Ui) {
        let state = self.transport_state();
        let recording = self.audio.as_ref().is_some_and(|(_, e)| e.is_recording());
        ui.horizontal(|ui| {
            ui.add_enabled_ui(self.audio.is_some(), |ui| {
                let buttons = [
                    ("|<", "Go to Beginning", Command::GoToBeginning),
                    ("<<", "Rewind", Command::Rewind),
                    (">>", "Forward", Command::Forward),
                ];
                for (text, tip, command) in buttons {
                    if ui.button(text).on_hover_text(tip).clicked() {
                        self.run(command);
                    }
                }
                if ui.button("Stop").clicked() {
                    self.send(EngineCommand::Stop);
                }
                let play = if state == TransportState::Stopped {
                    "Play"
                } else {
                    "Playing"
                };
                if ui
                    .selectable_label(state != TransportState::Stopped, play)
                    .clicked()
                    && state == TransportState::Stopped
                {
                    self.send(EngineCommand::Play);
                }
                let record = RichText::new("Record").color(if recording {
                    Color32::from_rgb(235, 80, 80)
                } else {
                    ui.visuals().text_color()
                });
                if ui.selectable_label(recording, record).clicked() {
                    self.run(Command::Record);
                }
            });
            ui.separator();
            let position = Position::from_tick(self.playhead_tick(), self.project.time_signature);
            let text = if state == TransportState::CountIn {
                "count-in".to_owned()
            } else {
                position.to_string()
            };
            ui.label(RichText::new(text).monospace().size(18.0));
            ui.separator();
            let mut bpm = self.project.tempo_bpm;
            let tempo = ui
                .add(
                    egui::DragValue::new(&mut bpm)
                        .range(MIN_BPM..=MAX_BPM)
                        .speed(0.5)
                        .fixed_decimals(1)
                        .suffix(" bpm"),
                )
                .on_hover_text("Tempo");
            self.live_edit(&tempo, |p| p.tempo_bpm = bpm);
            let mut numerator = self.project.time_signature.numerator;
            let num = ui.add(
                egui::DragValue::new(&mut numerator)
                    .range(1..=16)
                    .speed(0.1),
            );
            self.live_edit(&num, |p| p.time_signature.numerator = numerator);
            ui.label("/");
            let mut denominator = self.project.time_signature.denominator;
            egui::ComboBox::from_id_salt("denominator")
                .width(40.0)
                .selected_text(denominator.to_string())
                .show_ui(ui, |ui| {
                    for d in DENOMINATORS {
                        ui.selectable_value(&mut denominator, d, d.to_string());
                    }
                });
            if denominator != self.project.time_signature.denominator {
                self.edit(|p| p.time_signature.denominator = denominator);
            }
            ui.separator();
            let p = &self.project;
            let (looping, metronome, count_in, snap) = (
                p.loop_range.enabled,
                p.metronome.enabled,
                p.metronome.count_in_enabled,
                p.snap.enabled,
            );
            if Self::toggle_button(ui, looping, "Loop", "Loop (C)") {
                self.run(Command::ToggleLoop);
            }
            if Self::toggle_button(ui, metronome, "Click", "Metronome (K)") {
                self.run(Command::ToggleMetronome);
            }
            if Self::toggle_button(
                ui,
                count_in,
                "Count-in",
                "Count-in before recording (Shift+K)",
            ) {
                self.run(Command::ToggleCountIn);
            }
            ui.separator();
            if Self::toggle_button(ui, snap, "Snap", "Snap to grid (Ctrl+G)") {
                self.run(Command::ToggleSnap);
            }
            let mut grid = self.project.snap.grid;
            egui::ComboBox::from_id_salt("grid")
                .width(56.0)
                .selected_text(grid.label())
                .show_ui(ui, |ui| {
                    for g in Grid::ALL {
                        ui.selectable_value(&mut grid, g, g.label());
                    }
                });
            if grid != self.project.snap.grid {
                self.edit(|p| p.snap.grid = grid);
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.audio {
                Some((output, _)) => ui.label(format!(
                    "{}  |  {} Hz  |  {} ch",
                    output.device_name, output.sample_rate, output.channels
                )),
                None => ui.colored_label(ui.visuals().warn_fg_color, "No audio output"),
            };
            if let Some(message) = &self.message {
                ui.separator();
                ui.label(message);
            }
        });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut chosen = None;
        let mut refresh = false;
        let mut reopen = false;
        egui::Window::new("Audio Settings")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Output device");
                let current = self.settings.output_device.as_deref();
                let selected = current
                    .and_then(|id| self.devices.iter().find(|d| d.id == id))
                    .map_or_else(
                        || {
                            current
                                .map_or("System default".to_owned(), |id| format!("Missing: {id}"))
                        },
                        |d| d.name.clone(),
                    );
                egui::ComboBox::from_id_salt("output_device")
                    .selected_text(selected)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(current.is_none(), "System default")
                            .clicked()
                        {
                            chosen = Some(None);
                        }
                        for device in &self.devices {
                            let is_current = current == Some(device.id.as_str());
                            if ui.selectable_label(is_current, &device.name).clicked() {
                                chosen = Some(Some(device.id.clone()));
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    refresh = ui.button("Refresh list").clicked();
                    reopen = ui.button("Reopen device").clicked();
                });
            });
        self.show_settings = open;
        if refresh {
            self.refresh_devices();
        }
        if reopen {
            self.message = None;
            self.open_device();
        }
        if let Some(id) = chosen {
            self.message = None;
            self.select_device(id);
        }
    }

    fn new_tracks_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.new_tracks else {
            return;
        };
        let mut create = false;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("new_tracks")).show(ctx, |ui| {
            ui.heading("New Tracks");
            ui.horizontal(|ui| {
                ui.radio_value(&mut dialog.kind, TrackKind::Instrument, "Instrument");
                ui.radio_value(&mut dialog.kind, TrackKind::Audio, "Audio");
            });
            ui.horizontal(|ui| {
                ui.label("Number of tracks");
                ui.add(egui::DragValue::new(&mut dialog.count).range(1..=16));
            });
            ui.horizontal(|ui| {
                create =
                    ui.button("Create").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
                close =
                    ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        });
        if create {
            let (kind, count) = (dialog.kind, dialog.count);
            let mut last = None;
            self.edit(|p| {
                for _ in 0..count {
                    last = Some(edit::add_track(p, kind));
                }
            });
            if last.is_some() {
                self.selection.track = last;
            }
        }
        if create || close {
            self.new_tracks = None;
        }
    }

    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("unsaved")).show(ctx, |ui| {
            ui.heading("Unsaved changes");
            ui.label(format!("Save changes to \"{}\"?", self.project.name));
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(0);
                }
                if ui.button("Don't Save").clicked() {
                    choice = Some(1);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => {
                self.pending = None;
                if self.save() {
                    self.proceed(pending);
                }
            }
            Some(1) => {
                self.pending = None;
                self.proceed(pending);
            }
            Some(_) => self.pending = None,
            None => {}
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}{} - feeling-noob",
            self.project.name,
            if self.is_dirty() { "*" } else { "" }
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn follow_playhead(&mut self, width: f32) {
        if self.transport_state() != TransportState::Playing || self.drag.is_some() {
            return;
        }
        let visible = width / self.view.pixels_per_tick;
        let tick = self.playhead_tick() as f32;
        if tick > self.view.scroll_tick + visible * 0.9 || tick < self.view.scroll_tick {
            self.view.scroll_tick = (tick - visible * 0.1).max(0.0);
        }
    }

    fn poll_engine(&mut self) {
        if let Some((output, engine)) = &mut self.audio {
            engine.collect_garbage();
            if let Some(kind) = output.take_error() {
                self.message = Some(format!(
                    "Audio device problem: {kind:?}. Reopen it in Audio Settings."
                ));
            }
        }
        if self.audio.is_none() {
            return;
        }
        if self.transport_state() == TransportState::Stopped {
            self.stopped_tick = self.playhead_tick();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_engine();
        self.handle_close(&ctx);
        if self.pending.is_none() && self.new_tracks.is_none() {
            for command in take_commands(&ctx) {
                self.run(command);
            }
        }
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("control_bar").show(ui, |ui| self.control_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.follow_playhead(ui.available_width() - crate::timeline::HEADER_WIDTH);
                self.tracks_area(ui);
            });
        self.settings_window(&ctx);
        self.new_tracks_window(&ctx);
        self.confirm_window(&ctx);
        self.update_title(&ctx);
        debug_assert!(
            (MIN_PIXELS_PER_TICK..=MAX_PIXELS_PER_TICK).contains(&self.view.pixels_per_tick)
        );
        if self.transport_state() != TransportState::Stopped {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}
