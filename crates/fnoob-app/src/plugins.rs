use crate::app::App;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use eframe::egui;
use fnoob_core::catalog::{
    Catalog, CatalogEntry, STANDARD_VST3_FOLDER, ScanReport, default_catalog_path, find_vst3,
};
use fnoob_core::effects::BuiltinEffect;
use fnoob_core::model::{
    Id, Instrument, MAX_INSERTS, PluginSlot, Project, SamplerParams, SynthParams,
};
use fnoob_engine::Command as EngineCommand;
use fnoob_engine::effects::BuiltinProcessor;
use fnoob_engine::plan::{MAX_BLOCK, MAX_PLUGINS, PluginMap};
use fnoob_platform::plugin::{ClassInfo, Editor, Module, Plugin, cid_from_hex, instantiate};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SCAN_TIMEOUT: Duration = Duration::from_secs(10);
const SCAN_WORKERS: usize = 4;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq)]
pub enum InstrumentChoice {
    BasicSynth,
    FmSynth,
    Sampler,
    DrumSampler,
    Vst3(CatalogEntry),
}

impl InstrumentChoice {
    pub const BUILTIN: [Self; 4] = [
        Self::BasicSynth,
        Self::FmSynth,
        Self::Sampler,
        Self::DrumSampler,
    ];

    pub fn label(&self) -> String {
        match self {
            Self::BasicSynth => "Basic Synth".to_owned(),
            Self::FmSynth => "FM Synth".to_owned(),
            Self::Sampler => "Sampler".to_owned(),
            Self::DrumSampler => "Drum Sampler".to_owned(),
            Self::Vst3(entry) => entry.class.name.clone(),
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::BasicSynth => "Built-in synthesizer".to_owned(),
            Self::FmSynth => "Two-operator FM synthesizer".to_owned(),
            Self::Sampler => "Plays one sample across the keyboard".to_owned(),
            Self::DrumSampler => "16 one-shot pads".to_owned(),
            Self::Vst3(entry) => entry.class.vendor.clone(),
        }
    }
}

pub type BouncePlugins = (
    PluginMap,
    Vec<(usize, Box<fnoob_platform::plugin::PluginProcessor>)>,
    Vec<(usize, Box<BuiltinProcessor>)>,
    Vec<Plugin>,
);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Track(Id),
    Return(usize),
}

enum LoadedKind {
    Vst3 {
        plugin: Plugin,
        editor: Option<Editor>,
    },
    Builtin(BuiltinEffect),
}

struct LoadedPlugin {
    kind: LoadedKind,
    engine_slot: usize,
}

struct ScanJob {
    reports: Receiver<ScanReport>,
    total: usize,
    done: usize,
}

pub struct PluginHost {
    pub catalog: Catalog,
    catalog_path: Option<PathBuf>,
    scan: Option<ScanJob>,
    modules: HashMap<String, Arc<Module>>,
    loaded: HashMap<Id, LoadedPlugin>,
    failed: HashMap<Id, String>,
    free: Vec<usize>,
    pub show_manager: bool,
    pub edited: bool,
}

impl PluginHost {
    pub fn new() -> (Self, Option<String>) {
        let catalog_path = default_catalog_path();
        let (catalog, problem) = match catalog_path.as_deref().map(Catalog::load) {
            Some(Ok(catalog)) => (catalog, None),
            Some(Err(e)) => (Catalog::default(), Some(format!("Plugin list: {e}"))),
            None => (Catalog::default(), None),
        };
        let host = Self {
            catalog,
            catalog_path,
            scan: None,
            modules: HashMap::new(),
            loaded: HashMap::new(),
            failed: HashMap::new(),
            free: (0..MAX_PLUGINS).rev().collect(),
            show_manager: false,
            edited: false,
        };
        (host, problem)
    }

    pub fn map(&self) -> PluginMap {
        self.loaded
            .iter()
            .map(|(&id, loaded)| (id, loaded.engine_slot))
            .collect()
    }

    pub fn problem(&self, slot: Id) -> Option<&str> {
        self.failed.get(&slot).map(String::as_str)
    }
}

fn project_slots(project: &Project) -> Vec<&PluginSlot> {
    project
        .tracks
        .iter()
        .flat_map(|t| t.plugin_slots())
        .chain(project.returns.iter().flat_map(|r| r.inserts.iter()))
        .collect()
}

fn project_slots_mut(project: &mut Project) -> Vec<&mut PluginSlot> {
    let mut slots = Vec::new();
    for track in &mut project.tracks {
        if let Some(Instrument::Vst3(slot)) = &mut track.instrument {
            slots.push(slot);
        }
        slots.extend(track.strip.inserts.iter_mut());
    }
    slots.extend(
        project
            .returns
            .iter_mut()
            .flat_map(|r| r.inserts.iter_mut()),
    );
    slots
}

fn scanner_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let scanner = exe.with_file_name("fnoob-scan.exe");
    scanner.is_file().then_some(scanner)
}

fn scan_one(scanner: &Path, plugin: &Path) -> ScanReport {
    let path = plugin.display().to_string();
    let failed = |reason: String| ScanReport {
        path: path.clone(),
        classes: Vec::new(),
        error: Some(reason),
    };
    let child = Command::new(scanner)
        .arg(plugin)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return failed(format!("the scanner could not start: {e}")),
    };
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(out) = &mut stdout {
            let _ = out.read_to_string(&mut text);
        }
        text
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > SCAN_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return failed("no answer within 10 seconds".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(e) => return failed(e.to_string()),
        }
    }
    let text = reader.join().unwrap_or_default();
    let line = text.lines().rev().find(|l| l.trim_start().starts_with('{'));
    match line.map(serde_json_report) {
        Some(Ok(report)) => ScanReport { path, ..report },
        Some(Err(e)) => failed(e),
        None => failed("the plugin crashed while it was scanned".into()),
    }
}

fn serde_json_report(line: &str) -> Result<ScanReport, String> {
    fnoob_core::catalog::parse_report(line)
}

fn start_scan(folders: Vec<PathBuf>, scanner: PathBuf) -> ScanJob {
    let files = find_vst3(&folders);
    let total = files.len();
    let queue = Arc::new(Mutex::new(files));
    let (sender, reports) = channel();
    for _ in 0..SCAN_WORKERS {
        let (queue, sender, scanner): (_, Sender<ScanReport>, _) =
            (Arc::clone(&queue), sender.clone(), scanner.clone());
        std::thread::spawn(move || {
            loop {
                let next = queue.lock().ok().and_then(|mut q| q.pop());
                let Some(path) = next else {
                    break;
                };
                if sender.send(scan_one(&scanner, &path)).is_err() {
                    break;
                }
            }
        });
    }
    ScanJob {
        reports,
        total,
        done: 0,
    }
}

fn decode(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() {
        None
    } else {
        STANDARD.decode(text).ok()
    }
}

impl App {
    fn plugin_folders(&self) -> Vec<PathBuf> {
        std::iter::once(STANDARD_VST3_FOLDER.to_owned())
            .chain(self.settings.plugin_folders.iter().cloned())
            .map(PathBuf::from)
            .collect()
    }

    pub(crate) fn start_plugin_scan(&mut self) {
        if self.plugins.scan.is_some() {
            return;
        }
        match scanner_path() {
            Some(scanner) => {
                self.plugins.scan = Some(start_scan(self.plugin_folders(), scanner));
                self.message = Some("Scanning plugins...".into());
            }
            None => {
                self.message = Some(
                    "fnoob-scan.exe is missing next to the app. Build it with: cargo build --workspace"
                        .into(),
                );
            }
        }
    }

    pub(crate) fn poll_plugins(&mut self) {
        if let Some(job) = &mut self.plugins.scan {
            while let Ok(report) = job.reports.try_recv() {
                job.done += 1;
                self.plugins.catalog.add_report(report);
            }
            if job.done >= job.total {
                self.plugins.scan = None;
                if let Some(path) = &self.plugins.catalog_path
                    && let Err(e) = self.plugins.catalog.save(path)
                {
                    self.message = Some(format!("Plugin list not saved: {e}"));
                } else {
                    self.message = Some(format!(
                        "Plugin scan done: {} plugins, {} failed.",
                        self.plugins.catalog.plugins.len(),
                        self.plugins.catalog.failed.len()
                    ));
                }
                self.plugins.failed.clear();
                self.sync_plan();
            }
        }
        let mut edited = false;
        for loaded in self.plugins.loaded.values_mut() {
            if let LoadedKind::Vst3 { plugin, editor } = &mut loaded.kind {
                if editor.as_ref().is_some_and(Editor::is_closed) {
                    *editor = None;
                }
                edited |= plugin.take_edited();
            }
        }
        self.plugins.edited |= edited;
    }

    fn load_slot(
        &mut self,
        slot: &PluginSlot,
    ) -> Result<(Plugin, Box<fnoob_platform::plugin::PluginProcessor>), String> {
        let entry = self
            .plugins
            .catalog
            .find(&slot.class_id)
            .cloned()
            .or_else(|| {
                Path::new(&slot.path_hint).exists().then(|| CatalogEntry {
                    path: slot.path_hint.clone(),
                    class: fnoob_core::catalog::ScanClass {
                        class_id: slot.class_id.clone(),
                        name: slot.name.clone(),
                        vendor: String::new(),
                        instrument: false,
                    },
                })
            })
            .ok_or_else(|| format!("{} is not installed", slot.name))?;
        let cid = cid_from_hex(&slot.class_id).ok_or("invalid class id")?;
        let module = match self.plugins.modules.get(&entry.path) {
            Some(module) => Arc::clone(module),
            None => {
                let module = Module::load(Path::new(&entry.path)).map_err(|e| e.to_string())?;
                self.plugins
                    .modules
                    .insert(entry.path.clone(), Arc::clone(&module));
                module
            }
        };
        let info = ClassInfo {
            cid,
            name: entry.class.name.clone(),
            category: "Audio Module Class".into(),
            sub_categories: String::new(),
            vendor: entry.class.vendor.clone(),
        };
        let component = decode(&slot.component_state);
        let controller = slot.controller_state.as_deref().and_then(decode);
        let state = component.as_deref().map(|c| (c, controller.as_deref()));
        instantiate(
            &module,
            &info,
            f64::from(self.sample_rate()),
            MAX_BLOCK,
            state,
        )
        .map_err(|e| e.to_string())
    }

    pub(crate) fn sync_plugins(&mut self) {
        if self.audio.is_none() {
            return;
        }
        let wanted: Vec<PluginSlot> = project_slots(&self.project).into_iter().cloned().collect();
        let ids: HashSet<Id> = wanted.iter().map(|s| s.id).collect();
        let stale: Vec<Id> = self
            .plugins
            .loaded
            .keys()
            .filter(|id| !ids.contains(id))
            .copied()
            .collect();
        for id in stale {
            if let Some(loaded) = self.plugins.loaded.remove(&id) {
                self.send(EngineCommand::RemovePlugin(loaded.engine_slot));
                self.plugins.free.push(loaded.engine_slot);
            }
        }
        self.plugins.failed.retain(|id, _| ids.contains(id));
        for slot in wanted {
            if let (Some(effect), Some(loaded)) =
                (slot.builtin, self.plugins.loaded.get_mut(&slot.id))
                && let LoadedKind::Builtin(current) = &mut loaded.kind
            {
                if *current != effect {
                    *current = effect;
                    let engine_slot = loaded.engine_slot;
                    self.send(EngineCommand::SetBuiltinParams {
                        slot: engine_slot,
                        params: effect,
                    });
                }
                continue;
            }
            if self.plugins.loaded.contains_key(&slot.id)
                || self.plugins.failed.contains_key(&slot.id)
            {
                continue;
            }
            let Some(engine_slot) = self.plugins.free.pop() else {
                self.plugins
                    .failed
                    .insert(slot.id, "too many plugins".into());
                continue;
            };
            if let Some(effect) = slot.builtin {
                let rate = self.sample_rate();
                self.send(EngineCommand::InstallBuiltin {
                    slot: engine_slot,
                    effect: Box::new(BuiltinProcessor::new(effect, rate)),
                });
                self.plugins.loaded.insert(
                    slot.id,
                    LoadedPlugin {
                        kind: LoadedKind::Builtin(effect),
                        engine_slot,
                    },
                );
                continue;
            }
            match self.load_slot(&slot) {
                Ok((plugin, processor)) => {
                    plugin.take_edited();
                    self.send(EngineCommand::InstallPlugin {
                        slot: engine_slot,
                        plugin: processor,
                    });
                    self.plugins.loaded.insert(
                        slot.id,
                        LoadedPlugin {
                            kind: LoadedKind::Vst3 {
                                plugin,
                                editor: None,
                            },
                            engine_slot,
                        },
                    );
                }
                Err(reason) => {
                    self.plugins.free.push(engine_slot);
                    self.message = Some(format!("{}: {reason}", slot.name));
                    self.plugins.failed.insert(slot.id, reason);
                }
            }
        }
    }

    pub(crate) fn bounce_plugins(&mut self) -> BouncePlugins {
        self.capture_plugin_states();
        let slots: Vec<PluginSlot> = project_slots(&self.project)
            .into_iter()
            .filter(|s| self.plugins.loaded.contains_key(&s.id))
            .cloned()
            .collect();
        let mut map = PluginMap::new();
        let mut processors = Vec::new();
        let mut builtins = Vec::new();
        let mut plugins = Vec::new();
        let rate = self.sample_rate();
        for slot in slots {
            if let Some(effect) = slot.builtin {
                let index = processors.len() + builtins.len();
                map.insert(slot.id, index);
                builtins.push((index, Box::new(BuiltinProcessor::new(effect, rate))));
                continue;
            }
            match self.load_slot(&slot) {
                Ok((plugin, processor)) => {
                    let index = processors.len() + builtins.len();
                    map.insert(slot.id, index);
                    processors.push((index, processor));
                    plugins.push(plugin);
                }
                Err(e) => self.message = Some(format!("{}: {e}", slot.name)),
            }
        }
        (map, processors, builtins, plugins)
    }

    pub(crate) fn capture_plugin_states(&mut self) {
        let loaded = &self.plugins.loaded;
        let mut problems = Vec::new();
        for slot in project_slots_mut(&mut self.project) {
            let Some(LoadedKind::Vst3 { plugin, .. }) = loaded.get(&slot.id).map(|l| &l.kind)
            else {
                continue;
            };
            match plugin.state() {
                Ok((component, controller)) => {
                    slot.component_state = STANDARD.encode(component);
                    slot.controller_state = controller.map(|c| STANDARD.encode(c));
                }
                Err(e) => problems.push(format!("{}: {e}", slot.name)),
            }
        }
        if !problems.is_empty() {
            self.message = Some(problems.join("; "));
        }
    }

    pub(crate) fn release_engine_plugins(&mut self) {
        let slots: Vec<usize> = self
            .plugins
            .loaded
            .values()
            .map(|l| l.engine_slot)
            .collect();
        if slots.is_empty() {
            return;
        }
        for slot in slots {
            self.send(EngineCommand::RemovePlugin(slot));
        }
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(10));
            if let Some((_, engine)) = &mut self.audio {
                engine.collect_garbage();
            }
        }
        for (_, loaded) in self.plugins.loaded.drain() {
            self.plugins.free.push(loaded.engine_slot);
        }
        self.plugins.failed.clear();
    }

    pub(crate) fn open_plugin_editor(&mut self, slot: Id) {
        let Some(loaded) = self.plugins.loaded.get_mut(&slot) else {
            return;
        };
        let LoadedKind::Vst3 { plugin, editor } = &mut loaded.kind else {
            self.reveal_device(slot);
            return;
        };
        if let Some(editor) = editor {
            editor.focus();
            return;
        }
        let title = format!("{} - feeling-noob", plugin.info.name);
        match plugin.open_editor(&title) {
            Ok(opened) => *editor = Some(opened),
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    pub(crate) fn set_builtin_insert(&mut self, owner: Owner, index: usize, effect: BuiltinEffect) {
        self.capture_plugin_states();
        self.edit(|p| {
            let slot = PluginSlot::builtin(p.alloc_id(), effect);
            let inserts = match owner {
                Owner::Track(id) => match p.track_mut(id) {
                    Some(t) => &mut t.strip.inserts,
                    None => return,
                },
                Owner::Return(index) => match p.returns.get_mut(index) {
                    Some(r) => &mut r.inserts,
                    None => return,
                },
            };
            if index < inserts.len() {
                inserts[index] = slot;
            } else if inserts.len() < MAX_INSERTS {
                inserts.push(slot);
            }
        });
    }

    pub(crate) fn is_plugin_loaded(&self, slot: Id) -> bool {
        self.plugins.loaded.contains_key(&slot)
    }

    fn new_slot(project: &mut Project, entry: &CatalogEntry) -> PluginSlot {
        PluginSlot {
            id: project.alloc_id(),
            class_id: entry.class.class_id.clone(),
            name: entry.class.name.clone(),
            path_hint: entry.path.clone(),
            bypass: false,
            component_state: String::new(),
            controller_state: None,
            builtin: None,
            sidechain: None,
        }
    }

    pub(crate) fn set_insert(&mut self, owner: Owner, index: usize, entry: Option<&CatalogEntry>) {
        self.capture_plugin_states();
        self.edit(|p| {
            let slot = entry.map(|e| Self::new_slot(p, e));
            let inserts = match owner {
                Owner::Track(id) => match p.track_mut(id) {
                    Some(t) => &mut t.strip.inserts,
                    None => return,
                },
                Owner::Return(index) => match p.returns.get_mut(index) {
                    Some(r) => &mut r.inserts,
                    None => return,
                },
            };
            match slot {
                Some(slot) if index < inserts.len() => inserts[index] = slot,
                Some(slot) if inserts.len() < MAX_INSERTS => inserts.push(slot),
                Some(_) => {}
                None if index < inserts.len() => {
                    inserts.remove(index);
                }
                None => {}
            }
        });
    }

    pub(crate) fn toggle_bypass(&mut self, owner: Owner, index: usize) {
        self.edit(|p| {
            let inserts = match owner {
                Owner::Track(id) => p.track_mut(id).map(|t| &mut t.strip.inserts),
                Owner::Return(index) => p.returns.get_mut(index).map(|r| &mut r.inserts),
            };
            if let Some(slot) = inserts.and_then(|i| i.get_mut(index)) {
                slot.bypass = !slot.bypass;
            }
        });
    }

    pub(crate) fn set_instrument(&mut self, track: Id, choice: &InstrumentChoice) {
        self.capture_plugin_states();
        self.edit(|p| {
            let instrument = match choice {
                InstrumentChoice::Vst3(e) => Instrument::Vst3(Self::new_slot(p, e)),
                InstrumentChoice::BasicSynth => Instrument::BasicSynth(SynthParams::default()),
                InstrumentChoice::FmSynth => {
                    Instrument::FmSynth(fnoob_core::model::FmParams::default())
                }
                InstrumentChoice::Sampler => Instrument::Sampler(SamplerParams::default()),
                InstrumentChoice::DrumSampler => Instrument::DrumSampler(Box::default()),
            };
            if let Some(t) = p.track_mut(track) {
                let old = t
                    .instrument
                    .as_ref()
                    .map_or_else(String::new, |i| i.label().to_owned());
                t.instrument = Some(instrument);
                fnoob_core::edit::name_after_instrument(t, &old);
            }
        });
    }

    pub(crate) fn plugin_manager(&mut self, ctx: &egui::Context) {
        let mut open = self.plugins.show_manager;
        let mut rescan = false;
        let mut add_folder = false;
        let mut remove_folder = None;
        egui::Window::new("Plugin Manager")
            .open(&mut open)
            .default_size([560.0, 480.0])
            .show(ctx, |ui| {
                ui.label("Folders");
                ui.label(STANDARD_VST3_FOLDER);
                for (index, folder) in self.settings.plugin_folders.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(folder);
                        if ui.small_button("Remove").clicked() {
                            remove_folder = Some(index);
                        }
                    });
                }
                ui.horizontal(|ui| {
                    add_folder = ui.button("Add folder...").clicked();
                    let scanning = self.plugins.scan.as_ref();
                    if let Some(job) = scanning {
                        ui.spinner();
                        ui.label(format!("Scanning {} of {}", job.done, job.total));
                    } else {
                        rescan = ui.button("Rescan").clicked();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("plugin_list").striped(true).show(ui, |ui| {
                        for entry in &self.plugins.catalog.plugins {
                            ui.label(&entry.class.name);
                            ui.label(&entry.class.vendor);
                            ui.label(if entry.class.instrument {
                                "Instrument"
                            } else {
                                "Effect"
                            });
                            ui.label("OK");
                            ui.end_row();
                        }
                        for failed in &self.plugins.catalog.failed {
                            let name = Path::new(&failed.path)
                                .file_stem()
                                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                            ui.label(name);
                            ui.label("");
                            ui.label("");
                            ui.colored_label(ui.visuals().warn_fg_color, "Failed")
                                .on_hover_text(&failed.reason);
                            ui.end_row();
                        }
                    });
                });
            });
        self.plugins.show_manager = open;
        if let Some(index) = remove_folder {
            self.settings.plugin_folders.remove(index);
            self.save_settings();
        }
        if add_folder
            && let Some(folder) = rfd::FileDialog::new()
                .set_title("Add plugin folder")
                .pick_folder()
        {
            self.settings
                .plugin_folders
                .push(folder.display().to_string());
            self.save_settings();
        }
        if rescan {
            self.start_plugin_scan();
        }
    }
}
