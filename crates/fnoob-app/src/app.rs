use crate::keys::take_commands;
use crate::midi::Take;
use crate::piano_roll::Editor;
use crate::timeline::{LANE_HEIGHT, MAX_PIXELS_PER_TICK, MIN_PIXELS_PER_TICK};
use eframe::egui::{self, Pos2, Rect};
use fnoob_core::edit;
use fnoob_core::history::History;
use fnoob_core::io::{self, PROJECT_EXTENSION, PROJECT_FILE};
use fnoob_core::keymap::{Command, shortcut_label};
use fnoob_core::model::{AutomationParam, Id, Project, Region, Track, TrackKind};
use fnoob_core::settings::{
    BUFFER_SIZES, InputChoice, MidiInputChoice, RecordFormat, Settings, default_settings_path,
};
use fnoob_core::time::{Timeline, grid_ticks, ticks_per_bar};
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
    RegionTrim {
        origin: Project,
        region: Id,
        anchor: Pos2,
    },
    Automation {
        origin: Project,
        track: Id,
        param: AutomationParam,
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
            | Drag::Velocity { origin, .. }
            | Drag::RegionTrim { origin, .. }
            | Drag::Automation { origin, .. } => origin,
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
    pub tracks: Vec<Id>,
    pub anchor: Option<Id>,
}

pub struct View {
    pub pixels_per_tick: f32,
    pub scroll_tick: f32,
    pub scroll_y: f32,
    pub follow: bool,
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
    pub(crate) clipboard: Vec<Region>,
    pub(crate) system_clipboard: Option<String>,
    pub(crate) track_clipboard: Vec<Track>,
    pub(crate) header_focus: bool,
    pub(crate) view: View,
    pub(crate) drag: Option<Drag>,
    pub(crate) renaming: Option<(Id, String)>,
    pub(crate) editor: Editor,
    pub(crate) mixer_open: bool,
    pub(crate) devices_open: bool,
    pub(crate) played: std::collections::VecDeque<fnoob_core::capture::Played>,
    pub(crate) chain_target: crate::chain::ChainTarget,
    pub(crate) browser: crate::browser::Browser,
    pub(crate) inspector: crate::inspector::Inspector,
    pub(crate) draw_mode: bool,
    pub(crate) returns_visible: bool,
    pub(crate) io_open: bool,
    pub(crate) sends_open: bool,
    pub(crate) insert_tick: u64,
    pub(crate) time_selection: Option<crate::arrange::TimeSelection>,
    pub(crate) time_drag: Option<(u64, usize)>,
    pub(crate) time_clipboard: Option<fnoob_core::range::RangeClip>,
    pub(crate) zoom_history: crate::arrange::ZoomHistory,
    pub(crate) lanes_width: f32,
    pub(crate) automation_arm: bool,
    quit_requested: bool,
    pub(crate) takes_open: HashSet<Id>,
    pub(crate) comp_drag: Option<(Id, usize, u64)>,
    pub(crate) tap_times: Vec<f64>,
    pub(crate) cpu: f32,
    pub(crate) info: Option<String>,
    pub(crate) detail_devices: bool,
    pub(crate) automation_view: bool,
    pub(crate) session: crate::session::SessionUi,
    pub(crate) automation_params: std::collections::HashMap<Id, AutomationParam>,
    pub(crate) marker_rename: Option<(Id, String)>,
    pub(crate) musical: crate::musical::MusicalTyping,
    pub(crate) plugins: crate::plugins::PluginHost,
    pub(crate) meters: crate::mixer::Meters,
    pub(crate) focus: Focus,
    editor_rect: Option<Rect>,
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
        let mut project = Project::new("Untitled", DEFAULT_SAMPLE_RATE);
        edit::default_set(&mut project);
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
            system_clipboard: None,
            track_clipboard: Vec::new(),
            header_focus: false,
            view: View {
                pixels_per_tick: 0.04,
                scroll_tick: 0.0,
                scroll_y: 0.0,
                follow: true,
            },
            drag: None,
            renaming: None,
            editor: Editor::default(),
            mixer_open: true,
            devices_open: true,
            played: std::collections::VecDeque::new(),
            chain_target: crate::chain::ChainTarget::default(),
            browser: crate::browser::Browser::shown(),
            inspector: crate::inspector::Inspector::default(),
            draw_mode: false,
            returns_visible: true,
            io_open: true,
            sends_open: true,
            insert_tick: 0,
            time_selection: None,
            time_drag: None,
            time_clipboard: None,
            zoom_history: crate::arrange::ZoomHistory::default(),
            lanes_width: 800.0,
            automation_arm: false,
            quit_requested: false,
            takes_open: HashSet::new(),
            comp_drag: None,
            tap_times: Vec::new(),
            cpu: 0.0,
            info: None,
            detail_devices: true,
            automation_view: false,
            automation_params: std::collections::HashMap::new(),
            marker_rename: None,
            musical: crate::musical::MusicalTyping::default(),
            plugins,
            meters: crate::mixer::Meters::default(),
            focus: Focus::Tracks,
            editor_rect: None,
            session: crate::session::SessionUi::default(),
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
        self.project.timeline(self.sample_rate())
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
        self.send_outputs();
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

    pub(crate) fn title_selection(&self) -> Vec<Id> {
        let chosen: Vec<Id> = self
            .project
            .tracks
            .iter()
            .map(|t| t.id)
            .filter(|id| self.selection.tracks.contains(id))
            .collect();
        if chosen.is_empty() {
            self.selection.track.into_iter().collect()
        } else {
            chosen
        }
    }

    pub(crate) fn is_title_selected(&self, id: Id) -> bool {
        self.selection.track == Some(id) || self.header_focus && self.selection.tracks.contains(&id)
    }

    fn clip_names(&self) -> Option<String> {
        let names: Vec<&str> = self.clipboard.iter().map(|r| r.name.as_str()).collect();
        (!names.is_empty()).then(|| {
            let text = names.join("\n");
            if text.trim().is_empty() {
                "clips".to_owned()
            } else {
                text
            }
        })
    }

    pub(crate) fn title_command(&mut self, id: Id, command: Command) {
        if !(self.header_focus && self.is_title_selected(id)) {
            self.session.selected = None;
            self.focus_title(id, false, false);
        }
        self.run(command);
    }

    pub(crate) fn focus_title(&mut self, id: Id, shift: bool, ctrl: bool) {
        self.time_selection = None;
        let order: Vec<Id> = self.project.tracks.iter().map(|t| t.id).collect();
        if shift
            && self.header_focus
            && let Some(anchor) = self.selection.anchor.or(self.selection.track)
            && let (Some(a), Some(b)) = (
                order.iter().position(|t| *t == anchor),
                order.iter().position(|t| *t == id),
            )
        {
            self.selection.tracks = order[a.min(b)..=a.max(b)].to_vec();
        } else if ctrl && self.header_focus {
            if self.selection.tracks.is_empty() {
                self.selection.tracks.extend(self.selection.track);
            }
            if let Some(i) = self.selection.tracks.iter().position(|t| *t == id) {
                self.selection.tracks.remove(i);
            } else {
                self.selection.tracks.push(id);
            }
            self.selection.anchor = Some(id);
        } else {
            self.selection.tracks = vec![id];
            self.selection.anchor = Some(id);
        }
        self.selection.track = Some(id);
        self.selection.regions.clear();
        self.header_focus = true;
    }

    pub(crate) fn locate(&mut self, tick: u64) {
        self.stopped_tick = tick;
        if self.transport_state() == TransportState::Stopped {
            self.insert_tick = tick;
        }
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
        let _bar = ticks_per_bar(self.project.time_signature);
        let grid = grid_ticks(self.project.snap.grid, self.project.time_signature) as i64;
        match command {
            Command::PlayStop => {
                let start = self.insert_tick;
                if self.transport_state() != TransportState::Stopped {
                    self.send(EngineCommand::Stop);
                    self.locate(start);
                } else if self.rate_matches() {
                    self.locate(start);
                    self.send(EngineCommand::Play);
                }
            }
            Command::PlayFromInsert => {
                let start = self.insert_tick;
                if self.rate_matches() {
                    self.locate(start);
                    if self.transport_state() == TransportState::Stopped {
                        self.send(EngineCommand::Play);
                    }
                }
            }
            Command::InsertToPlayhead => {
                self.insert_tick = self.playhead_tick();
                self.time_selection = None;
            }
            Command::ZoomToSelection => self.zoom_to_selection(),
            Command::ZoomBack => {
                if let Some(state) = self.zoom_history.pop() {
                    self.view.pixels_per_tick = state.pixels_per_tick;
                    self.view.scroll_tick = state.scroll_tick;
                }
            }
            Command::FitSong => {
                let end = crate::arrange::song_end(&self.project);
                self.zoom_to(0, end);
            }
            Command::ContinuePlay => {
                if self.transport_state() != TransportState::Stopped {
                    self.send(EngineCommand::Stop);
                } else if self.rate_matches() {
                    self.send(EngineCommand::Play);
                }
            }
            Command::BackToArrangement => self.send(EngineCommand::BackToArrangement),
            Command::ShowClipView => self.show_clip(),
            Command::ShowDeviceView => self.show_devices(),
            Command::ToggleReturns => self.returns_visible = !self.returns_visible,
            Command::ToggleInOut => self.io_open = !self.io_open,
            Command::ToggleSends => self.sends_open = !self.sends_open,
            Command::OpenSettings => self.show_settings = true,
            Command::Quit => self.quit_requested = true,
            Command::QuantizeOff
            | Command::QuantizeSixteenth
            | Command::QuantizeEighth
            | Command::QuantizeQuarter
            | Command::QuantizeBar => {
                use fnoob_core::session::LaunchQuantize as Q;
                let quantize = match command {
                    Command::QuantizeOff => Q::None,
                    Command::QuantizeSixteenth => Q::Sixteenth,
                    Command::QuantizeEighth => Q::Eighth,
                    Command::QuantizeQuarter => Q::Quarter,
                    _ => Q::Bar,
                };
                self.edit(|p| p.launch_quantize = quantize);
            }
            Command::NarrowGrid | Command::WidenGrid => {
                let all = fnoob_core::model::Grid::ALL;
                let index = all
                    .iter()
                    .position(|g| *g == self.project.snap.grid)
                    .unwrap_or(0);
                let next = if command == Command::NarrowGrid {
                    (index + 1).min(all.len() - 1)
                } else {
                    index.saturating_sub(1)
                };
                self.edit(|p| p.snap.grid = all[next]);
            }
            Command::InsertMidiClip => self.insert_midi_clip(),
            Command::LaunchSelected => {
                self.launch_selected_clip();
            }
            Command::StopSelectedTrack => {
                if let Some(track) = self.selection.track {
                    self.send(EngineCommand::StopClips(Some(track)));
                }
            }
            Command::TrackActivator(index) => {
                if let Some(id) = self.project.tracks.get(usize::from(index)).map(|t| t.id) {
                    self.edit_track(id, |t| t.mute = !t.mute);
                }
            }
            Command::ShowTakeLanes => {
                if let Some(track) = self.selection.track
                    && !self.takes_open.remove(&track)
                {
                    self.takes_open.insert(track);
                }
            }
            Command::SetLoopStart | Command::SetLoopEnd => {
                let tick = self.playhead_tick();
                let bar = ticks_per_bar(self.project.time_signature);
                self.edit(|p| {
                    let range = &mut p.loop_range;
                    if command == Command::SetLoopStart {
                        range.start_tick = tick;
                        if range.end_tick <= tick {
                            range.end_tick = tick + bar;
                        }
                    } else if tick > range.start_tick {
                        range.end_tick = tick;
                    }
                });
            }
            Command::LoopShorter
            | Command::LoopLonger
            | Command::LoopHalve
            | Command::LoopDouble => {
                let bar = ticks_per_bar(self.project.time_signature);
                self.edit(|p| {
                    let range = &mut p.loop_range;
                    let length = range.end_tick - range.start_tick;
                    let new = match command {
                        Command::LoopShorter => length.saturating_sub(bar).max(bar),
                        Command::LoopLonger => length + bar,
                        Command::LoopHalve => (length / 2).max(grid as u64),
                        _ => length * 2,
                    };
                    range.end_tick = range.start_tick + new;
                });
            }
            Command::MoveLeft | Command::MoveRight => {
                let step = grid;
                let delta = if command == Command::MoveLeft {
                    -step
                } else {
                    step
                };
                if regions.is_empty() {
                    let tick = (self.playhead_tick() as i64 + delta).max(0) as u64;
                    self.locate(tick);
                } else {
                    self.edit(|p| {
                        edit::nudge_regions(p, &regions, delta);
                    });
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
                let meter = self.project.meter();
                self.locate(meter.bar_floor(tick.saturating_sub(1)));
            }
            Command::Forward => {
                let tick = self.playhead_tick();
                let meter = self.project.meter();
                self.locate(meter.bar_start(meter.bar_at(tick).0 + 1));
            }
            Command::ToggleLoop => match self.time_selection.clone() {
                Some(selection) => self.edit(|p| {
                    p.loop_range.start_tick = selection.start;
                    p.loop_range.end_tick = selection.end;
                    p.loop_range.enabled = true;
                }),
                None => self.edit(|p| p.loop_range.enabled = !p.loop_range.enabled),
            },
            Command::ToggleMetronome => self.edit(|p| p.metronome.enabled = !p.metronome.enabled),
            Command::ToggleCountIn => {
                self.edit(|p| p.metronome.count_in_enabled = !p.metronome.count_in_enabled);
            }
            Command::ToggleMixer => self.toggle_mixer(),
            Command::ToggleDetail => self.toggle_detail(),
            Command::SwitchDetail => self.switch_detail(),
            Command::ToggleBrowser => self.browser.open = !self.browser.open,
            Command::ToggleDrawMode => self.draw_mode = !self.draw_mode,
            Command::ToggleFollow => self.view.follow = !self.view.follow,
            Command::NewAudioTrack | Command::NewMidiTrack => {
                let kind = if command == Command::NewAudioTrack {
                    TrackKind::Audio
                } else {
                    TrackKind::Instrument
                };
                let mut created = 0;
                self.edit(|p| created = edit::add_track(p, kind));
                self.selection.track = Some(created);
                self.selection.regions.clear();
            }
            Command::Rename => self.rename_selection(),
            Command::NewReturnTrack => {
                let mut added = None;
                self.edit(|p| added = edit::add_return(p, "Return", None));
                match added {
                    Some(index) => {
                        self.chain_target = crate::chain::ChainTarget::Return(index);
                        self.show_devices();
                    }
                    None => {
                        self.message = Some("A project can have at most 8 return tracks.".into())
                    }
                }
            }
            Command::InsertScene if !self.session.open && self.insert_silence() => {}
            Command::InsertScene => {
                let at = self
                    .session
                    .selected
                    .and_then(|id| fnoob_core::session::clip_position(&self.project, id))
                    .map_or(self.project.scenes.len(), |(_, scene)| scene + 1);
                self.edit(|p| {
                    fnoob_core::session::insert_scene(p, at);
                });
            }
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
            Command::MuteRegions if regions.is_empty() => {
                if let Some(id) = self.selection.track {
                    self.edit_track(id, |t| t.mute = !t.mute);
                }
            }
            Command::MuteRegions => {
                let ids = if regions.is_empty() {
                    self.regions_in_selection()
                } else {
                    regions.clone()
                };
                self.edit(|p| edit::toggle_regions_muted(p, &ids));
            }
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
            Command::Copy | Command::Cut | Command::Delete
                if self.header_focus && regions.is_empty() && self.selection.track.is_some() =>
            {
                let ids = self.title_selection();
                let Some(&track) = ids.first() else {
                    return;
                };
                if command != Command::Delete {
                    self.capture_plugin_states();
                    self.track_clipboard = edit::copy_tracks(&self.project, &ids);
                    self.clipboard.clear();
                    self.time_clipboard = None;
                    let names: Vec<&str> = self
                        .track_clipboard
                        .iter()
                        .map(|t| t.name.as_str())
                        .collect();
                    self.system_clipboard = Some(names.join("\n"));
                }
                if command != Command::Copy {
                    let index = self.project.tracks.iter().position(|t| t.id == track);
                    self.edit(|p| {
                        for id in &ids {
                            edit::delete_track(p, *id);
                        }
                    });
                    self.selection.tracks.clear();
                    self.selection.track = index.and_then(|i| {
                        let i = i.min(self.project.tracks.len().saturating_sub(1));
                        self.project.tracks.get(i).map(|t| t.id)
                    });
                }
            }
            Command::Copy | Command::Cut | Command::Delete | Command::Duplicate
                if regions.is_empty() && !self.header_focus && self.time_selection.is_some() =>
            {
                self.selection_command(command);
            }
            Command::CutTime
            | Command::PasteTime
            | Command::DuplicateTime
            | Command::DeleteTime => self.time_command(command),
            Command::Consolidate => self.consolidate(),
            Command::FreezeTrack => {
                if let Some(id) = self.selection.track {
                    self.freeze_track(id);
                }
            }
            Command::FlattenTrack => {
                let Some(id) = self.selection.track else {
                    return;
                };
                let mut done = None;
                self.edit(|p| done = edit::flatten_track(p, id));
                if done.is_none() {
                    self.message = Some("Freeze the track first, then flatten it.".into());
                }
            }
            Command::GroupTracks => {
                let ids = self.title_selection();
                let mut group = None;
                self.edit(|p| group = edit::group_tracks(p, &ids));
                match group {
                    Some(id) => {
                        self.selection.track = Some(id);
                        self.selection.tracks = vec![id];
                        self.header_focus = true;
                    }
                    None => {
                        self.message = Some(
                            "Select tracks that are not in a group yet, then group them.".into(),
                        );
                    }
                }
            }
            Command::Ungroup => {
                let group = self.selection.track.and_then(|id| {
                    let track = self.project.track(id)?;
                    if track.kind == TrackKind::Group {
                        Some(id)
                    } else {
                        track.group
                    }
                });
                match group {
                    Some(id) => {
                        self.edit(|p| {
                            edit::ungroup(p, id);
                        });
                        self.selection.track = None;
                        self.selection.tracks.clear();
                    }
                    None => self.message = Some("Select a group track to ungroup it.".into()),
                }
            }
            Command::Crop => self.crop_selection(),
            Command::Reverse => self.reverse_clips(),
            Command::CreateFade => self.fade_selection(),
            Command::Paste if self.time_clipboard.is_some() => self.paste_time_clip(false),
            Command::Paste if !self.track_clipboard.is_empty() && self.clipboard.is_empty() => {
                let tracks = self.track_clipboard.clone();
                let after = self
                    .title_selection()
                    .last()
                    .copied()
                    .or(self.selection.track);
                let mut created = Vec::new();
                self.edit(|p| created = edit::paste_tracks(p, after, &tracks));
                if let Some(&first) = created.first() {
                    self.selection.track = Some(first);
                    self.selection.anchor = Some(first);
                    self.selection.tracks = created.clone();
                    self.selection.regions.clear();
                    self.header_focus = true;
                }
            }
            Command::Copy => {
                self.clipboard = edit::copy_regions(&self.project, &regions);
                self.track_clipboard.clear();
                self.time_clipboard = None;
                self.system_clipboard = self.clip_names();
            }
            Command::Cut => {
                self.track_clipboard.clear();
                self.time_clipboard = None;
                self.clipboard = edit::copy_regions(&self.project, &regions);
                self.system_clipboard = self.clip_names();
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
            Command::Delete if self.session.open && self.delete_selected_clip() => {}
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
            Command::AddMarker => {
                let tick = self.playhead_tick();
                self.edit(|p| {
                    edit::add_marker(p, tick);
                });
            }
            Command::PreviousMarker | Command::NextMarker => {
                let tick = self.playhead_tick();
                let target = if command == Command::NextMarker {
                    self.project.markers.iter().find(|m| m.tick > tick)
                } else {
                    self.project.markers.iter().rev().find(|m| m.tick < tick)
                };
                if let Some(marker) = target {
                    let tick = marker.tick;
                    self.locate(tick);
                }
            }
            Command::ToggleAutomation => self.automation_view = !self.automation_view,
            Command::ToggleMusicalTyping => self.toggle_musical_typing(),
            Command::ToggleClipGrid => self.session.open = !self.session.open,
            Command::CaptureMidi => self.capture_midi(),
            Command::Duplicate if self.session.open && self.duplicate_selected_clip() => {}
            Command::Duplicate if !regions.is_empty() => {
                let mut created = Vec::new();
                self.edit(|p| created = edit::repeat_regions(p, &regions));
                if !created.is_empty() {
                    self.selection.regions = created;
                }
            }
            Command::Duplicate => {
                let ids = if self.header_focus {
                    self.title_selection()
                } else {
                    self.selection.track.into_iter().collect()
                };
                if !ids.is_empty() {
                    self.capture_plugin_states();
                    let after = ids.last().copied();
                    let mut created = Vec::new();
                    self.edit(|p| {
                        let copies = edit::copy_tracks(p, &ids);
                        created = edit::paste_tracks(p, after, &copies);
                        for (copy, id) in copies.iter().zip(&created) {
                            if let Some(t) = p.track_mut(*id) {
                                t.name = format!("{} copy", copy.name);
                            }
                        }
                    });
                    if let Some(&first) = created.first() {
                        self.selection.track = Some(first);
                        self.selection.anchor = Some(first);
                        self.selection.tracks = created;
                        self.selection.regions.clear();
                    }
                }
            }
            Command::SaveAs => {
                self.save_as();
            }
        }
    }

    pub(crate) fn request(&mut self, pending: Pending) {
        if self.is_dirty() {
            self.pending = Some(pending);
        } else {
            self.proceed(pending);
        }
    }

    fn proceed(&mut self, pending: Pending) {
        match pending {
            Pending::New => {
                let mut project = Project::new("Untitled", self.sample_rate());
                edit::default_set(&mut project);
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

    fn menu_toggle(&mut self, ui: &mut egui::Ui, text: &str, on: bool, command: Command) {
        let mut button = egui::Button::selectable(on, text);
        if let Some(label) = shortcut_label(command) {
            button = button.shortcut_text(label);
        }
        if ui.add(button).clicked() {
            self.run(command);
            ui.close();
        }
    }

    fn menu_action(&mut self, ui: &mut egui::Ui, text: &str, action: impl FnOnce(&mut Self)) {
        if ui.button(text).clicked() {
            action(self);
            ui.close();
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                self.menu_item(ui, "New Project", Command::NewProject);
                self.menu_item(ui, "Open Project...", Command::Open);
                let recent = self.settings.recent_projects.clone();
                ui.add_enabled_ui(!recent.is_empty(), |ui| {
                    ui.menu_button("Open Recent Project", |ui| {
                        for path in recent {
                            let exists = Path::new(&path).join(PROJECT_FILE).is_file();
                            if ui.add_enabled(exists, egui::Button::new(&path)).clicked() {
                                self.request(Pending::OpenPath(PathBuf::from(&path)));
                                ui.close();
                            }
                        }
                    });
                });
                ui.separator();
                self.menu_action(ui, "Import Audio File...", |app| app.import_audio());
                self.menu_action(ui, "Move Unused Audio Files...", |app| {
                    app.find_unused_audio()
                });
                ui.separator();
                self.menu_item(ui, "Save Project", Command::Save);
                self.menu_item(ui, "Save Project As...", Command::SaveAs);
                ui.separator();
                self.menu_item(ui, "Export Audio...", Command::Bounce);
                ui.separator();
                self.menu_action(ui, "Project Settings...", |app| {
                    app.show_project_settings = true
                });
                ui.separator();
                self.menu_item(ui, "Quit", Command::Quit);
            });
            ui.menu_button("Edit", |ui| {
                self.menu_item(ui, "Undo", Command::Undo);
                self.menu_item(ui, "Redo", Command::Redo);
                ui.separator();
                self.menu_item(ui, "Cut", Command::Cut);
                self.menu_item(ui, "Copy", Command::Copy);
                self.menu_item(ui, "Paste", Command::Paste);
                self.menu_item(ui, "Duplicate", Command::Duplicate);
                self.menu_item(ui, "Delete", Command::Delete);
                ui.separator();
                self.menu_item(ui, "Activate/Deactivate Clip(s)", Command::MuteRegions);
                ui.separator();
                self.menu_item(ui, "Select All", Command::SelectAll);
                self.menu_item(ui, "Loop Selection", Command::ToggleLoop);
                ui.separator();
                self.menu_item(ui, "Group Tracks", Command::GroupTracks);
                self.menu_item(ui, "Ungroup Tracks", Command::Ungroup);
                ui.separator();
                self.menu_item(ui, "Cut Time", Command::CutTime);
                self.menu_item(ui, "Paste Time", Command::PasteTime);
                self.menu_item(ui, "Duplicate Time", Command::DuplicateTime);
                self.menu_item(ui, "Delete Time", Command::DeleteTime);
                ui.separator();
                self.menu_item(ui, "Rename", Command::Rename);
                ui.separator();
                self.menu_item(ui, "Mute Track", Command::ToggleTrackMute);
                self.menu_item(ui, "Solo Track", Command::ToggleTrackSolo);
                self.menu_item(ui, "Arm Track", Command::ToggleTrackRecord);
                self.menu_item(ui, "Freeze / Unfreeze Track", Command::FreezeTrack);
                self.menu_item(ui, "Freeze and Flatten Track", Command::FlattenTrack);
                ui.separator();
                self.menu_item(ui, "Split", Command::SplitAtPlayhead);
                self.menu_item(ui, "Consolidate", Command::Consolidate);
                self.menu_item(ui, "Crop Clip(s)", Command::Crop);
                self.menu_item(ui, "Reverse Clip(s)", Command::Reverse);
                self.menu_item(ui, "Repeat Clip(s)", Command::RepeatRegions);
                self.menu_item(ui, "Nudge Left", Command::NudgeLeft);
                self.menu_item(ui, "Nudge Right", Command::NudgeRight);
                ui.separator();
                self.menu_item(ui, "Quantize", Command::Quantize);
            });
            ui.menu_button("Create", |ui| {
                self.menu_item(ui, "Insert Audio Track", Command::NewAudioTrack);
                self.menu_item(ui, "Insert MIDI Track", Command::NewMidiTrack);
                self.menu_item(ui, "Insert Return Track", Command::NewReturnTrack);
                self.menu_item(ui, "Insert Several Tracks...", Command::NewTracks);
                ui.separator();
                let insert = if self.session.open {
                    "Insert Scene"
                } else {
                    "Insert Silence"
                };
                self.menu_item(ui, insert, Command::InsertScene);
                ui.separator();
                self.menu_item(ui, "Insert Empty MIDI Clip", Command::InsertMidiClip);
                self.menu_item(ui, "Capture MIDI", Command::CaptureMidi);
                ui.separator();
                self.menu_item(ui, "Create Fade", Command::CreateFade);
                ui.separator();
                self.menu_item(ui, "Add Locator", Command::AddMarker);
            });
            ui.menu_button("Playback", |ui| {
                self.menu_item(ui, "Play / Stop", Command::PlayStop);
                self.menu_item(ui, "Continue Playback", Command::ContinuePlay);
                self.menu_item(ui, "Play From Insert Marker", Command::PlayFromInsert);
                self.menu_item(ui, "Return Play Position to 1.1.1", Command::GoToBeginning);
                self.menu_item(
                    ui,
                    "Move Insert Marker to Playhead",
                    Command::InsertToPlayhead,
                );
                ui.separator();
                self.menu_item(ui, "Record", Command::Record);
                let punch = self.project.punch;
                if ui
                    .add(egui::Button::selectable(punch.punch_in, "Punch In"))
                    .clicked()
                {
                    self.edit(|p| p.punch.punch_in = !p.punch.punch_in);
                    ui.close();
                }
                if ui
                    .add(egui::Button::selectable(punch.punch_out, "Punch Out"))
                    .clicked()
                {
                    self.edit(|p| p.punch.punch_out = !p.punch.punch_out);
                    ui.close();
                }
                ui.separator();
                let metronome = self.project.metronome;
                self.menu_toggle(ui, "Metronome", metronome.enabled, Command::ToggleMetronome);
                self.menu_toggle(
                    ui,
                    "Count-In",
                    metronome.count_in_enabled,
                    Command::ToggleCountIn,
                );
                self.menu_toggle(
                    ui,
                    "Loop",
                    self.project.loop_range.enabled,
                    Command::ToggleLoop,
                );
                ui.separator();
                self.menu_item(ui, "Back to Timeline", Command::BackToArrangement);
            });
            ui.menu_button("View", |ui| {
                self.menu_item(ui, "Toggle Timeline / Clip Grid", Command::ToggleClipGrid);
                ui.separator();
                self.menu_toggle(ui, "Clip View", self.editor.open, Command::ShowClipView);
                self.menu_toggle(
                    ui,
                    "Device View",
                    self.devices_open,
                    Command::ShowDeviceView,
                );
                self.menu_item(ui, "Toggle Clip / Device View", Command::SwitchDetail);
                self.menu_toggle(
                    ui,
                    "Detail Area",
                    self.detail_visible(),
                    Command::ToggleDetail,
                );
                ui.separator();
                self.menu_toggle(ui, "Browser", self.browser.open, Command::ToggleBrowser);
                self.menu_toggle(ui, "Mixer", self.mixer_open, Command::ToggleMixer);
                self.menu_toggle(ui, "In/Out", self.io_open, Command::ToggleInOut);
                self.menu_toggle(ui, "Sends", self.sends_open, Command::ToggleSends);
                self.menu_toggle(ui, "Returns", self.returns_visible, Command::ToggleReturns);
                self.menu_item(ui, "Take Lanes", Command::ShowTakeLanes);
                self.menu_toggle(
                    ui,
                    "Automation",
                    self.automation_view,
                    Command::ToggleAutomation,
                );
                ui.separator();
                self.menu_item(ui, "Zoom In", Command::ZoomIn);
                self.menu_item(ui, "Zoom Out", Command::ZoomOut);
                self.menu_item(ui, "Zoom to Selection", Command::ZoomToSelection);
                self.menu_item(ui, "Zoom Back", Command::ZoomBack);
                self.menu_item(ui, "Fit Song", Command::FitSong);
                self.menu_toggle(ui, "Follow", self.view.follow, Command::ToggleFollow);
            });
            ui.menu_button("Navigate", |ui| {
                self.menu_action(ui, "Timeline", |app| app.session.open = false);
                self.menu_action(ui, "Clip Grid", |app| app.session.open = true);
                self.menu_item(ui, "Clip View", Command::ShowClipView);
                self.menu_item(ui, "Device View", Command::ShowDeviceView);
                self.menu_item(ui, "Browser", Command::ToggleBrowser);
                self.menu_item(ui, "Mixer", Command::ToggleMixer);
                ui.separator();
                self.menu_item(ui, "Previous Track", Command::SelectPreviousTrack);
                self.menu_item(ui, "Next Track", Command::SelectNextTrack);
                self.menu_item(ui, "Previous Locator", Command::PreviousMarker);
                self.menu_item(ui, "Next Locator", Command::NextMarker);
            });
            ui.menu_button("Options", |ui| {
                self.menu_toggle(
                    ui,
                    "Computer MIDI Keyboard",
                    self.musical.on,
                    Command::ToggleMusicalTyping,
                );
                let scale = self.project.scale.enabled;
                if ui
                    .add(egui::Button::selectable(scale, "Highlight Scales"))
                    .clicked()
                {
                    self.edit(|p| p.scale.enabled = !scale);
                    ui.close();
                }
                ui.separator();
                self.menu_item(ui, "Narrow Grid", Command::NarrowGrid);
                self.menu_item(ui, "Widen Grid", Command::WidenGrid);
                self.menu_toggle(
                    ui,
                    "Snap to Grid",
                    self.project.snap.enabled,
                    Command::ToggleSnap,
                );
                ui.separator();
                self.menu_toggle(ui, "Draw Mode", self.draw_mode, Command::ToggleDrawMode);
                let overdub = self.project.midi_overdub;
                if ui
                    .add(egui::Button::selectable(
                        overdub,
                        "MIDI Arrangement Overdub",
                    ))
                    .clicked()
                {
                    self.edit(|p| p.midi_overdub = !overdub);
                    ui.close();
                }
                ui.separator();
                self.menu_action(ui, "Plugin Manager...", |app| {
                    app.plugins.show_manager = true
                });
                self.menu_item(ui, "Audio and MIDI Settings...", Command::OpenSettings);
            });
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
        if !self.view.follow
            || self.transport_state() != TransportState::Playing
            || self.drag.is_some()
        {
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
        self.poll_played();
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
        if self.musical.on && self.pending.is_none() && self.marker_rename.is_none() {
            self.take_musical_keys(&ctx);
        }
        if self.pending.is_none()
            && self.new_tracks.is_none()
            && self.marker_rename.is_none()
            && self.recovery.is_none()
            && self.bounce.is_none()
            && self.bounce_dialog.is_none()
        {
            for command in take_commands(&ctx) {
                self.run(command);
            }
        }
        self.info = crate::ui_kit::take_info(&ctx);
        if let Some(text) = self.system_clipboard.take() {
            ctx.copy_text(text);
        }
        if std::mem::take(&mut self.quit_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("control_bar").show(ui, |ui| self.control_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        self.editor_rect = None;
        self.update_meters();
        if self.browser.open {
            egui::Panel::left("browser")
                .resizable(true)
                .default_size(320.0)
                .size_range(240.0..=560.0)
                .show(ui, |ui| self.browser_panel(ui));
        }
        if self.detail_visible() {
            let panel = egui::Panel::bottom("detail")
                .resizable(true)
                .default_size(300.0)
                .size_range(160.0..=900.0)
                .show(ui, |ui| self.detail_area(ui));
            if self.editor.open {
                self.editor_rect = Some(panel.response.rect);
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.session.open {
                    egui::Frame::NONE
                        .inner_margin(6.0)
                        .show(ui, |ui| self.session_view(ui));
                } else {
                    self.follow_playhead(ui.available_width() - crate::timeline::HEADER_WIDTH);
                    self.tracks_area(ui);
                }
            });
        self.settings_window(&ctx);
        self.project_settings_window(&ctx);
        self.cleanup_window(&ctx);
        self.recovery_window(&ctx);
        self.marker_rename_window(&ctx);
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
        } else if self.audio.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
