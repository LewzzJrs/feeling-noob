use crate::keys::take_commands;
use crate::midi::Take;
use crate::piano_roll::Editor;
use crate::timeline::{LANE_HEIGHT, MAX_PIXELS_PER_TICK, MIN_PIXELS_PER_TICK};
use eframe::egui::{self, Color32, Pos2, Rect, RichText};
use fnoob_core::edit;
use fnoob_core::history::History;
use fnoob_core::io::{self, PROJECT_EXTENSION, PROJECT_FILE};
use fnoob_core::keymap::{Command, shortcut_label};
use fnoob_core::model::{Grid, Id, Project, Region, Track, TrackKind};
use fnoob_core::settings::{
    BUFFER_SIZES, InputChoice, MidiInputChoice, RecordFormat, Settings, default_settings_path,
};
use fnoob_core::time::{
    MAX_BPM, MIN_BPM, Position, Timeline, grid_ticks, snap_floor, ticks_per_bar,
};
use fnoob_engine::audio_data::AudioPool;
use fnoob_engine::device::{
    AudioInputStream, AudioOutput, DeviceInfo, OutputDeviceInfo, list_output_devices, open_output,
};
use fnoob_engine::midi_in::{MidiConnection, MidiInputInfo};
use fnoob_engine::{Command as EngineCommand, EngineHandle, RenderPlan, TransportState};
use std::collections::HashSet;
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
    RegionResize {
        origin: Project,
        region: Id,
        anchor: Pos2,
    },
    Loop {
        origin: Project,
        anchor: u64,
    },
    Notes {
        origin: Project,
        region: Id,
        primary: usize,
        anchor: Pos2,
        resize: bool,
    },
    Velocity {
        origin: Project,
        index: usize,
    },
}

impl Drag {
    pub fn into_origin(self) -> Project {
        match self {
            Drag::Regions { origin, .. }
            | Drag::RegionResize { origin, .. }
            | Drag::Loop { origin, .. }
            | Drag::Notes { origin, .. }
            | Drag::Velocity { origin, .. } => origin,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tracks,
    Editor,
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

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    Quit,
}

struct NewTracksDialog {
    kind: TrackKind,
    count: u32,
}

pub struct App {
    pub(crate) settings: Settings,
    settings_path: Option<PathBuf>,
    devices: Vec<OutputDeviceInfo>,
    pub(crate) midi_inputs: Vec<MidiInputInfo>,
    pub(crate) midi: Option<MidiConnection>,
    pub(crate) live_sent: Option<Option<Id>>,
    pub(crate) take: Option<Take>,
    pub(crate) audio: Option<(AudioOutput, EngineHandle)>,
    pub(crate) input_devices: Vec<DeviceInfo>,
    pub(crate) input: Option<AudioInputStream>,
    pub(crate) pool: AudioPool,
    pub(crate) missing: HashSet<Id>,
    pub(crate) audio_take: Option<crate::audio::AudioTake>,
    pub(crate) project: Project,
    pub(crate) saved: Project,
    pub(crate) project_dir: Option<PathBuf>,
    pub(crate) autosave: crate::files::Autosave,
    pub(crate) recovery: Option<crate::files::Recovery>,
    pub(crate) show_project_settings: bool,
    pub(crate) cleanup: Option<Vec<String>>,
    pub(crate) bounce_dialog: Option<crate::bounce::BounceDialog>,
    pub(crate) bounce: Option<crate::bounce::ActiveBounce>,
    pub(crate) history: History,
    pub(crate) selection: Selection,
    clipboard: Vec<Region>,
    pub(crate) view: View,
    pub(crate) drag: Option<Drag>,
    pub(crate) renaming: Option<(Id, String)>,
    pub(crate) editor: Editor,
    pub(crate) mixer_open: bool,
    pub(crate) plugins: crate::plugins::PluginHost,
    pub(crate) meters: crate::mixer::Meters,
    pub(crate) focus: Focus,
    editor_rect: Option<Rect>,
    pub(crate) synth_window: Option<Id>,
    live_origin: Option<(egui::Id, Project)>,
    stopped_tick: u64,
    show_settings: bool,
    new_tracks: Option<NewTracksDialog>,
    pending: Option<Pending>,
    allow_close: bool,
    title: String,
    pub(crate) message: Option<String>,
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
        let (plugins, plugin_problem) = crate::plugins::PluginHost::new();
        if plugin_problem.is_some() {
            message = plugin_problem;
        }
        let mut app = Self {
            settings,
            settings_path,
            devices: Vec::new(),
            input_devices: Vec::new(),
            input: None,
            pool: AudioPool::new(),
            missing: HashSet::new(),
            audio_take: None,
            midi_inputs: Vec::new(),
            midi: None,
            live_sent: None,
            take: None,
            audio: None,
            saved: project.clone(),
            project,
            project_dir: None,
            autosave: crate::files::Autosave::default(),
            recovery: None,
            show_project_settings: false,
            cleanup: None,
            bounce_dialog: None,
            bounce: None,
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
            editor: Editor::default(),
            mixer_open: false,
            plugins,
            meters: crate::mixer::Meters::default(),
            focus: Focus::Tracks,
            editor_rect: None,
            synth_window: None,
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
        app.refresh_midi_inputs();
        app.refresh_input_devices();
        app.open_device();
        if let Some((output, _)) = &app.audio {
            app.project.sample_rate = output.sample_rate;
            app.saved = app.project.clone();
        }
        app.check_startup_recovery();
        if let Some(dir) = open {
            app.open_dir(dir);
        }
        if app.plugins.catalog.plugins.is_empty() && app.plugins.catalog.failed.is_empty() {
            app.start_plugin_scan();
        }
        app
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.audio
            .as_ref()
            .map_or(self.project.sample_rate, |(o, _)| o.sample_rate)
    }

    pub(crate) fn timeline(&self) -> Timeline {
        Timeline {
            sample_rate: self.sample_rate(),
            bpm: self.project.tempo_bpm,
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.project != self.saved || self.plugins.edited
    }

    pub(crate) fn playhead_tick(&self) -> u64 {
        match &self.audio {
            Some((_, engine)) => self.timeline().sample_to_tick(engine.position()),
            None => self.stopped_tick,
        }
    }

    pub(crate) fn transport_state(&self) -> TransportState {
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
        self.audio_take = None;
        self.input = None;
        self.capture_plugin_states();
        self.release_engine_plugins();
        self.audio = None;
        let (project, pool) = (&self.project, &self.pool);
        match open_output(
            self.settings.output_device.as_deref(),
            self.settings.buffer_frames,
            |rate| RenderPlan::with_audio(project, rate, pool),
        ) {
            Ok(audio) => self.audio = Some(audio),
            Err(e) => self.message = Some(e.to_string()),
        }
        self.live_sent = None;
        self.take = None;
        self.connect_midi();
        self.open_input_device();
        self.sync_plan();
        self.locate(tick);
        self.check_sample_rate();
    }

    pub(crate) fn rate_matches(&mut self) -> bool {
        match &self.audio {
            Some((output, _)) if output.sample_rate != self.project.sample_rate => {
                self.message = Some(format!(
                    "Playback is blocked: the project runs at {} Hz but the device runs at {} Hz. Choose a device at {} Hz.",
                    self.project.sample_rate, output.sample_rate, self.project.sample_rate
                ));
                false
            }
            _ => true,
        }
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
        self.save_settings();
        self.open_device();
    }

    pub(crate) fn save_settings(&mut self) {
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            self.message = Some(e.to_string());
        }
    }

    pub(crate) fn send(&mut self, command: EngineCommand) {
        if let Some((_, engine)) = &mut self.audio
            && engine.send(command).is_err()
        {
            self.message = Some("Audio engine is not responding.".into());
        }
    }

    pub(crate) fn sync_plan(&mut self) {
        let rate = self.sample_rate();
        self.sync_plugins();
        let plan = Box::new(RenderPlan::build(
            &self.project,
            rate,
            &self.pool,
            &self.plugins.map(),
        ));
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

    pub(crate) fn live_edit(
        &mut self,
        response: &egui::Response,
        change: impl FnOnce(&mut Project),
    ) {
        if response.changed() {
            if self
                .live_origin
                .as_ref()
                .is_none_or(|(id, _)| *id != response.id)
            {
                self.commit_live_edit();
                self.live_origin = Some((response.id, self.project.clone()));
            }
            change(&mut self.project);
            self.sync_plan();
        }
        let owns = self
            .live_origin
            .as_ref()
            .is_some_and(|(id, _)| *id == response.id);
        if owns
            && (response.drag_stopped()
                || response.lost_focus()
                || !response.has_focus() && !response.dragged())
        {
            self.commit_live_edit();
        }
    }

    fn commit_live_edit(&mut self) {
        if let Some((_, origin)) = self.live_origin.take()
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
        self.editor.clamp_selection(&self.project);
        self.sync_plan();
    }

    fn paste_target(&self) -> Option<Id> {
        let kind = self.clipboard.first()?.kind();
        let matches = |id: &Id| self.project.track(*id).is_some_and(|t| t.kind == kind);
        self.selection.track.filter(matches).or_else(|| {
            self.project
                .tracks
                .iter()
                .find(|t| t.kind == kind)
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

    pub(crate) fn run(&mut self, command: Command) {
        if self.focus == Focus::Editor && self.editor_command(command) {
            return;
        }
        let regions = self.selection.regions.clone();
        let bar = ticks_per_bar(self.project.time_signature);
        let grid = grid_ticks(self.project.snap.grid, self.project.time_signature) as i64;
        match command {
            Command::PlayStop => {
                if self.transport_state() != TransportState::Stopped {
                    self.send(EngineCommand::Stop);
                } else if self.rate_matches() {
                    self.send(EngineCommand::Play);
                }
            }
            Command::Record => {
                if self.transport_state() != TransportState::Stopped {
                    self.send(EngineCommand::Stop);
                } else if self.rate_matches() {
                    self.start_recording();
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
            Command::ToggleMixer => self.toggle_mixer(),
            Command::TogglePianoRoll | Command::ToggleEditor => self.toggle_editor(),
            Command::Quantize => {
                if regions.is_empty() {
                    self.editor_command(Command::Quantize);
                } else {
                    let grid = grid as u64;
                    self.edit(|p| {
                        edit::quantize_regions(p, &regions, grid);
                    });
                }
            }
            Command::Bounce => self.open_bounce_dialog(),
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
            Command::Paste => match self.paste_target() {
                Some(track) => {
                    let tick = self.playhead_tick();
                    let clip = self.clipboard.clone();
                    let mut created = Vec::new();
                    self.edit(|p| created = edit::paste_regions(p, track, tick, &clip));
                    self.selection.track = Some(track);
                    self.selection.regions = created;
                }
                None if self.clipboard.is_empty() => {}
                None => self.message = Some("Paste needs a track of the same kind.".into()),
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
            Pending::OpenPath(dir) => self.open_dir(dir),
            Pending::Quit => {
                self.discard_autosave();
                self.allow_close = true;
            }
        }
    }

    pub(crate) fn replace_project(&mut self, project: Project, dir: Option<PathBuf>) {
        self.send(EngineCommand::Stop);
        self.saved = project.clone();
        self.project = project;
        self.plugins.edited = false;
        self.project_dir = dir;
        self.history.clear();
        self.selection = Selection::default();
        self.drag = None;
        self.renaming = None;
        self.live_origin = None;
        self.take = None;
        self.audio_take = None;
        self.editor = Editor::default();
        self.synth_window = None;
        self.view.scroll_tick = 0.0;
        self.view.scroll_y = 0.0;
        self.pool.clear();
        self.missing.clear();
        self.load_audio_files();
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
        if self.offer_autosave(&dir) {
            return;
        }
        if !dir.join(PROJECT_FILE).is_file() {
            self.message = Some(format!("{} has no {PROJECT_FILE}.", dir.display()));
            return;
        }
        match io::load(&dir) {
            Ok(project) => {
                self.message = None;
                self.settings.add_recent(&dir);
                self.save_settings();
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

    pub(crate) fn save_as(&mut self) -> bool {
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
        if let Err(e) = self.copy_audio_files(&dir) {
            self.message = Some(format!("Save failed: {e}"));
            return false;
        }
        if let Some(name) = dir.file_stem().and_then(|s| s.to_str()) {
            self.project.name = name.to_owned();
        }
        if self.save_to(&dir) {
            self.project_dir = Some(dir);
            let missing: Vec<Id> = self.missing.drain().collect();
            if !missing.is_empty() {
                self.load_audio_files();
                self.sync_plan();
            }
            true
        } else {
            false
        }
    }

    fn save_to(&mut self, dir: &Path) -> bool {
        self.capture_plugin_states();
        let was_untitled = self.project_dir.is_none();
        match io::save(dir, &self.project) {
            Ok(()) => {
                self.saved = self.project.clone();
                self.plugins.edited = false;
                self.project_dir = Some(dir.to_path_buf());
                self.after_manual_save(was_untitled);
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
                let recent = self.settings.recent_projects.clone();
                ui.add_enabled_ui(!recent.is_empty(), |ui| {
                    ui.menu_button("Open Recent", |ui| {
                        for path in recent {
                            let exists = Path::new(&path).join(PROJECT_FILE).is_file();
                            if ui.add_enabled(exists, egui::Button::new(&path)).clicked() {
                                self.request(Pending::OpenPath(PathBuf::from(&path)));
                                ui.close();
                            }
                        }
                    });
                });
                self.menu_item(ui, "Save", Command::Save);
                self.menu_item(ui, "Save As...", Command::SaveAs);
                ui.separator();
                if ui.button("Project Settings...").clicked() {
                    self.show_project_settings = true;
                    ui.close();
                }
                if ui.button("Move Unused Audio Files...").clicked() {
                    self.find_unused_audio();
                    ui.close();
                }
                ui.separator();
                self.menu_item(ui, "Bounce to WAV...", Command::Bounce);
                ui.separator();
                if ui.button("Import Audio File...").clicked() {
                    self.import_audio();
                    ui.close();
                }
                if ui.button("Plugin Manager...").clicked() {
                    self.plugins.show_manager = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Audio and MIDI Settings...").clicked() {
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
                self.menu_item(ui, "Quantize", Command::Quantize);
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
                let synth = selected.filter(|id| {
                    self.project
                        .track(*id)
                        .is_some_and(|t| t.kind == TrackKind::Instrument)
                });
                if ui
                    .add_enabled(synth.is_some(), egui::Button::new("Instrument Settings..."))
                    .clicked()
                {
                    self.synth_window = synth;
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
                self.menu_item(ui, "Mixer", Command::ToggleMixer);
                self.menu_item(ui, "Piano Roll", Command::TogglePianoRoll);
                ui.separator();
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
                    && self.rate_matches()
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
                    "{}  |  {} Hz  |  {} ch  |  In: {}  |  MIDI: {}",
                    output.device_name,
                    output.sample_rate,
                    output.channels,
                    self.input
                        .as_ref()
                        .map_or("none", |i| i.device_name.as_str()),
                    self.midi.as_ref().map_or("none", |m| m.name.as_str())
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
        let mut midi_choice = None;
        let mut midi_refresh = false;
        let mut input_choice = None;
        let mut buffer_choice = None;
        let mut format_choice = None;
        egui::Window::new("Audio and MIDI Settings")
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
                ui.label("Input device");
                let input = &self.settings.input_device;
                let selected = match input {
                    InputChoice::SystemDefault => "System default".to_owned(),
                    InputChoice::Off => "Off".to_owned(),
                    InputChoice::Device(id) => self
                        .input_devices
                        .iter()
                        .find(|d| &d.id == id)
                        .map_or_else(|| format!("Missing: {id}"), |d| d.name.clone()),
                };
                egui::ComboBox::from_id_salt("input_device")
                    .selected_text(selected)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for (label, option) in [
                            ("System default", InputChoice::SystemDefault),
                            ("Off", InputChoice::Off),
                        ] {
                            if ui.selectable_label(*input == option, label).clicked() {
                                input_choice = Some(option);
                            }
                        }
                        for device in &self.input_devices {
                            let option = InputChoice::Device(device.id.clone());
                            if ui.selectable_label(*input == option, &device.name).clicked() {
                                input_choice = Some(option);
                            }
                        }
                    });
                ui.label("Buffer size");
                let buffer = self.settings.buffer_frames;
                egui::ComboBox::from_id_salt("buffer_size")
                    .selected_text(buffer.map_or("Device default".to_owned(), |b| {
                        format!("{b} samples")
                    }))
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(buffer.is_none(), "Device default").clicked() {
                            buffer_choice = Some(None);
                        }
                        for size in BUFFER_SIZES {
                            if ui
                                .selectable_label(buffer == Some(size), format!("{size} samples"))
                                .clicked()
                            {
                                buffer_choice = Some(Some(size));
                            }
                        }
                    });
                ui.label("Recording format");
                let format = self.settings.record_format;
                egui::ComboBox::from_id_salt("record_format")
                    .selected_text(format.label())
                    .show_ui(ui, |ui| {
                        for option in RecordFormat::ALL {
                            if ui.selectable_label(format == option, option.label()).clicked() {
                                format_choice = Some(option);
                            }
                        }
                    });
                if let Some((output, _)) = &self.audio {
                    let rate = f64::from(output.sample_rate) / 1000.0;
                    let out = output.latency_frames();
                    let inp = self.input.as_ref().map_or(0, |i| i.latency_frames());
                    ui.label(format!(
                        "Latency: output {:.1} ms, input {:.1} ms. Recordings are shifted by {} samples.",
                        out as f64 / rate,
                        inp as f64 / rate,
                        out + inp
                    ));
                }
                ui.horizontal(|ui| {
                    refresh = ui.button("Refresh list").clicked();
                    reopen = ui.button("Reopen devices").clicked();
                });
                ui.separator();
                ui.label("MIDI input");
                let choice = &self.settings.midi_input;
                let selected = match choice {
                    MidiInputChoice::FirstAvailable => "First available".to_owned(),
                    MidiInputChoice::Off => "Off".to_owned(),
                    MidiInputChoice::Port(id) => self
                        .midi_inputs
                        .iter()
                        .find(|m| &m.id == id)
                        .map_or_else(|| format!("Missing: {id}"), |m| m.name.clone()),
                };
                egui::ComboBox::from_id_salt("midi_input")
                    .selected_text(selected)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for (label, option) in [
                            ("First available", MidiInputChoice::FirstAvailable),
                            ("Off", MidiInputChoice::Off),
                        ] {
                            if ui.selectable_label(*choice == option, label).clicked() {
                                midi_choice = Some(option);
                            }
                        }
                        for input in &self.midi_inputs {
                            let option = MidiInputChoice::Port(input.id.clone());
                            if ui
                                .selectable_label(*choice == option, &input.name)
                                .clicked()
                            {
                                midi_choice = Some(option);
                            }
                        }
                    });
                let status = match &self.midi {
                    Some(connection) => format!("Connected: {}", connection.name),
                    None => "Not connected".to_owned(),
                };
                ui.label(status);
                midi_refresh = ui.button("Refresh MIDI inputs").clicked();
            });
        self.show_settings = open;
        if midi_refresh {
            self.message = None;
            self.refresh_midi_inputs();
            self.connect_midi();
        }
        if let Some(choice) = midi_choice
            && choice != self.settings.midi_input
        {
            self.settings.midi_input = choice;
            self.save_settings();
            self.message = None;
            self.connect_midi();
        }
        if let Some(choice) = input_choice
            && choice != self.settings.input_device
        {
            self.settings.input_device = choice;
            self.save_settings();
            self.message = None;
            self.open_input_device();
        }
        if let Some(choice) = format_choice
            && choice != self.settings.record_format
        {
            self.settings.record_format = choice;
            self.save_settings();
        }
        if let Some(choice) = buffer_choice
            && choice != self.settings.buffer_frames
        {
            self.settings.buffer_frames = choice;
            self.save_settings();
            self.message = None;
            self.open_device();
        }
        if refresh {
            self.refresh_devices();
            self.refresh_input_devices();
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
        let Some(pending) = self.pending.clone() else {
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
                self.discard_autosave();
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
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.is_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending = Some(Pending::Quit);
            } else {
                self.discard_autosave();
            }
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

    fn update_focus(&mut self, ctx: &egui::Context) {
        let pressed = ctx.input(|i| {
            if i.pointer.any_pressed() {
                i.pointer.interact_pos()
            } else {
                None
            }
        });
        if let Some(pos) = pressed {
            self.focus = match self.editor_rect {
                Some(rect) if rect.contains(pos) => Focus::Editor,
                _ => Focus::Tracks,
            };
        }
        if !self.editor.open {
            self.focus = Focus::Tracks;
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
                    "Audio device problem: {kind:?}. Reopen it in Audio and MIDI Settings."
                ));
            }
        }
        if let Some(input) = &mut self.input
            && let Some(kind) = input.take_error()
        {
            self.message = Some(format!(
                "Audio input problem: {kind:?}. Reopen it in Audio and MIDI Settings."
            ));
        }
        self.poll_take();
        self.poll_audio_take();
        self.poll_plugins();
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
        self.update_focus(&ctx);
        if self.pending.is_none()
            && self.new_tracks.is_none()
            && self.recovery.is_none()
            && self.bounce.is_none()
            && self.bounce_dialog.is_none()
        {
            for command in take_commands(&ctx) {
                self.run(command);
            }
        }
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("control_bar").show(ui, |ui| self.control_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        self.editor_rect = None;
        self.update_meters();
        if self.mixer_open {
            egui::Panel::bottom("mixer")
                .resizable(true)
                .default_size(400.0)
                .size_range(260.0..=900.0)
                .show(ui, |ui| self.mixer(ui));
        } else if self.editor.open {
            let panel = egui::Panel::bottom("piano_roll")
                .resizable(true)
                .default_size(320.0)
                .size_range(160.0..=900.0)
                .show(ui, |ui| self.piano_roll(ui));
            self.editor_rect = Some(panel.response.rect);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.follow_playhead(ui.available_width() - crate::timeline::HEADER_WIDTH);
                self.tracks_area(ui);
            });
        self.settings_window(&ctx);
        self.synth_settings_window(&ctx);
        self.project_settings_window(&ctx);
        self.cleanup_window(&ctx);
        self.recovery_window(&ctx);
        self.bounce_windows(&ctx);
        self.plugin_manager(&ctx);
        self.new_tracks_window(&ctx);
        self.confirm_window(&ctx);
        self.update_title(&ctx);
        debug_assert!(
            (MIN_PIXELS_PER_TICK..=MAX_PIXELS_PER_TICK).contains(&self.view.pixels_per_tick)
        );
        self.sync_live_track();
        self.update_preview();
        self.tick_autosave(&ctx);
        if self.transport_state() != TransportState::Stopped || self.editor.preview.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if self.mixer_open {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}
