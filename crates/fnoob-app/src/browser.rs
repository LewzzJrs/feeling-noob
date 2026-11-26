use crate::app::App;
use crate::plugins::{InstrumentChoice, Owner};
use eframe::egui::{self, RichText};
use fnoob_core::catalog::CatalogEntry;
use fnoob_core::model::{Id, MAX_INSERTS, TrackKind};
use fnoob_engine::Command as EngineCommand;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_SAMPLES: usize = 2000;

#[derive(Debug, Clone, PartialEq)]
pub enum BrowserItem {
    Instrument(InstrumentChoice),
    Effect(CatalogEntry),
    Builtin(fnoob_core::effects::BuiltinEffect),
    Sample(PathBuf),
    MidiEffect(fnoob_core::midi_fx::MidiEffect),
    Template(PathBuf),
}

impl BrowserItem {
    pub fn label(&self) -> String {
        match self {
            Self::Instrument(choice) => choice.label(),
            Self::Effect(entry) => entry.class.name.clone(),
            Self::Builtin(effect) => effect.label().to_owned(),
            Self::Sample(path) => path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            Self::MidiEffect(effect) => effect.label().to_owned(),
            Self::Template(path) => path
                .file_stem()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    #[default]
    Instruments,
    Drums,
    AudioEffects,
    MidiEffects,
    Plugins,
    Samples,
    Templates,
    Folder(usize),
}

const DRUM_TAGS: [&str; 7] = [
    "Kick",
    "Snare",
    "Hihat",
    "Clap",
    "Cymbal",
    "Tom",
    "Percussion",
];

pub fn templates_dir() -> Option<PathBuf> {
    fnoob_core::settings::default_settings_path()
        .and_then(|p| p.parent().map(|d| d.join("templates")))
}

fn find_templates() -> Vec<PathBuf> {
    let Some(dir) = templates_dir() else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join(fnoob_core::io::PROJECT_FILE).is_file())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

#[derive(Default)]
pub struct Browser {
    pub open: bool,
    category: Category,
    tag: Option<&'static str>,
    search: String,
    samples: Vec<PathBuf>,
    scanned: bool,
    previewing: Option<PathBuf>,
    waveform: Option<(PathBuf, Arc<fnoob_engine::audio_data::AudioData>)>,
    templates: Vec<PathBuf>,
}

impl Browser {
    pub fn shown() -> Self {
        Self {
            open: true,
            ..Self::default()
        }
    }
}

fn find_audio_files(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = folders.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if fnoob_engine::decode::is_audio_file(&path) {
                found.push(path);
                if found.len() >= MAX_SAMPLES {
                    return found;
                }
            }
        }
    }
    found.sort();
    found
}

const SAMPLE_TAGS: [&str; 11] = [
    "Kick",
    "Snare",
    "Clap",
    "Hihat",
    "Cymbal",
    "Tom",
    "Percussion",
    "Loop",
    "Bass",
    "Vocal",
    "FX",
];

fn has_tag(name: &str, tag: &str) -> bool {
    let name = name.to_lowercase();
    let words: &[&str] = match tag {
        "Hihat" => &["hihat", "hi-hat", "hat", "hh"],
        "Percussion" => &["perc", "shaker", "conga", "bongo", "tamb"],
        "Cymbal" => &["cymbal", "crash", "ride", "splash"],
        "Vocal" => &["vocal", "vox", "voice"],
        other => return name.contains(&other.to_lowercase()),
    };
    words.iter().any(|w| name.contains(w))
}

fn matches(search: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    search
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

impl App {
    fn sample_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = self
            .settings
            .sample_folders
            .iter()
            .map(PathBuf::from)
            .collect();
        if let Some(dir) = &self.project_dir {
            folders.push(dir.join(fnoob_core::io::AUDIO_DIR));
        }
        folders
    }

    fn rescan_samples(&mut self) {
        self.browser.samples = find_audio_files(&self.sample_folders());
        self.browser.templates = find_templates();
        self.browser.scanned = true;
    }

    pub(crate) fn save_as_template(&mut self) {
        let Some(dir) = templates_dir() else {
            self.message = Some("No place to keep templates on this computer.".into());
            return;
        };
        let mut project = self.project.clone();
        let had_audio = !project.audio_files.is_empty();
        fnoob_core::edit::strip_audio(&mut project);
        let stem = crate::audio::file_stem_for(&project.name);
        let mut target = dir.join(format!("{stem}.{}", fnoob_core::io::PROJECT_EXTENSION));
        let mut n = 2;
        while target.exists() {
            target = dir.join(format!("{stem} {n}.{}", fnoob_core::io::PROJECT_EXTENSION));
            n += 1;
        }
        match std::fs::create_dir_all(&target)
            .map_err(|e| e.to_string())
            .and_then(|()| fnoob_core::io::save(&target, &project).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                self.rescan_samples();
                self.message = Some(if had_audio {
                    "Saved as a template. Audio clips and samples are not kept in templates.".into()
                } else {
                    "Saved as a template.".into()
                });
            }
            Err(e) => self.message = Some(format!("Cannot save the template. {e}")),
        }
    }

    pub(crate) fn new_from_template(&mut self, path: &Path) {
        match fnoob_core::io::load(path) {
            Ok(mut project) => {
                project.name = "Untitled".into();
                fnoob_core::edit::strip_audio(&mut project);
                self.replace_project(project, None);
            }
            Err(e) => self.message = Some(format!("Cannot open the template. {e}")),
        }
    }

    fn waveform_preview(&mut self, ui: &mut egui::Ui) {
        let Some((path, data)) = self.browser.waveform.clone() else {
            return;
        };
        ui.separator();
        ui.label(
            RichText::new(
                path.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            )
            .small(),
        );
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
        let columns = rect.width().max(1.0) as u64;
        let color = ui.visuals().strong_text_color().gamma_multiply(0.7);
        for column in 0..columns {
            let from = data.frames * column / columns;
            let to = data.frames * (column + 1) / columns;
            let level = data.peak(from, to.max(from + 1)).min(1.0);
            let x = rect.left() + column as f32;
            let half = rect.height() * 0.5 * level;
            painter.vline(
                x,
                rect.center().y - half..=rect.center().y + half,
                egui::Stroke::new(1.0, color),
            );
        }
        let seconds = data.frames as f64 / f64::from(self.sample_rate().max(1));
        ui.label(
            RichText::new(format!("{seconds:.2} s, {} channel(s)", data.channels))
                .small()
                .weak(),
        );
    }

    fn preview_sample(&mut self, path: &Path) {
        if self.browser.previewing.as_deref() == Some(path) {
            self.browser.previewing = None;
            self.send(EngineCommand::PreviewAudio(None));
            return;
        }
        match fnoob_engine::decode::load_any(path, self.sample_rate()) {
            Ok(data) => {
                let data = Arc::new(data);
                self.browser.previewing = Some(path.to_path_buf());
                self.browser.waveform = Some((path.to_path_buf(), Arc::clone(&data)));
                self.send(EngineCommand::PreviewAudio(Some(data)));
            }
            Err(e) => self.message = Some(format!("Cannot preview: {e}")),
        }
    }

    pub(crate) fn apply_browser_item(&mut self, item: &BrowserItem, track: Option<Id>, tick: u64) {
        match item {
            BrowserItem::Instrument(choice) => {
                let target = track.filter(|id| {
                    self.project
                        .track(*id)
                        .is_some_and(|t| t.kind == TrackKind::Instrument)
                });
                let target = match target {
                    Some(id) => id,
                    None => {
                        let mut created = 0;
                        self.edit(|p| {
                            created = fnoob_core::edit::add_track(p, TrackKind::Instrument)
                        });
                        created
                    }
                };
                self.set_instrument(target, choice);
                self.selection.track = Some(target);
            }
            BrowserItem::Builtin(effect) => {
                let target = match track {
                    Some(id) => id,
                    None => {
                        let mut created = 0;
                        self.edit(|p| created = fnoob_core::edit::add_track(p, TrackKind::Audio));
                        created
                    }
                };
                let used = self
                    .project
                    .track(target)
                    .map_or(MAX_INSERTS, |t| t.strip.inserts.len());
                if used >= MAX_INSERTS {
                    self.message = Some("All insert slots on this track are full.".into());
                    return;
                }
                self.set_builtin_insert(Owner::Track(target), used, *effect);
                self.selection.track = Some(target);
            }
            BrowserItem::Effect(entry) => {
                let target = match track {
                    Some(id) => id,
                    None => {
                        let mut created = 0;
                        self.edit(|p| created = fnoob_core::edit::add_track(p, TrackKind::Audio));
                        created
                    }
                };
                let used = self
                    .project
                    .track(target)
                    .map_or(MAX_INSERTS, |t| t.strip.inserts.len());
                if used >= MAX_INSERTS {
                    self.message = Some("All insert slots on this track are full.".into());
                    return;
                }
                self.set_insert(Owner::Track(target), used, Some(entry));
                self.selection.track = Some(target);
            }
            BrowserItem::Sample(path) => {
                self.import_files(std::slice::from_ref(path), track, tick);
            }
            BrowserItem::MidiEffect(effect) => {
                let target = track.filter(|id| {
                    self.project
                        .track(*id)
                        .is_some_and(|t| t.kind == TrackKind::Instrument)
                });
                let target = match target {
                    Some(id) => id,
                    None => {
                        let mut created = 0;
                        self.edit(|p| {
                            created = fnoob_core::edit::add_track(p, TrackKind::Instrument)
                        });
                        created
                    }
                };
                let full = self
                    .project
                    .track(target)
                    .is_some_and(|t| t.midi_effects.len() >= fnoob_core::midi_fx::MAX_MIDI_EFFECTS);
                if full {
                    self.message = Some("This track already has 4 MIDI effects.".into());
                    return;
                }
                let effect = *effect;
                self.edit(|p| {
                    if let Some(t) = p.track_mut(target) {
                        t.midi_effects.push(fnoob_core::midi_fx::MidiDevice {
                            effect,
                            bypass: false,
                        });
                    }
                });
                self.selection.track = Some(target);
                self.show_devices();
            }
            BrowserItem::Template(path) => {
                let path = path.clone();
                self.request(crate::app::Pending::Template(path));
            }
        }
    }

    fn browser_row(&mut self, ui: &mut egui::Ui, item: BrowserItem, detail: &str) {
        let id = ui.scope_id().with(("browser", item.label(), detail));
        let label = item.label();
        let mut apply = false;
        let mut preview = None;
        ui.horizontal(|ui| {
            let response = ui
                .dnd_drag_source(id, item.clone(), |ui| {
                    ui.add(
                        egui::Label::new(&label)
                            .truncate()
                            .sense(egui::Sense::click()),
                    );
                })
                .response;
            let response = if detail.is_empty() {
                response
            } else {
                response.on_hover_text(detail)
            };
            if response.double_clicked() {
                apply = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("+")
                    .on_hover_text("Add to the selected track")
                    .clicked()
                {
                    apply = true;
                }
                if let BrowserItem::Sample(path) = &item {
                    let playing = self.browser.previewing.as_ref() == Some(path);
                    if ui
                        .small_button(if playing { "Stop" } else { "Play" })
                        .on_hover_text("Preview")
                        .clicked()
                    {
                        preview = Some(path.clone());
                    }
                }
            });
        });
        if apply {
            let tick = self.playhead_tick();
            self.apply_browser_item(&item, self.selection.track, tick);
        }
        if let Some(path) = preview {
            self.preview_sample(&path);
        }
    }

    pub(crate) fn browser_panel(&mut self, ui: &mut egui::Ui) {
        if !self.browser.scanned {
            self.rescan_samples();
        }
        let search = crate::ui_kit::hint(
            ui.add(
                egui::TextEdit::singleline(&mut self.browser.search)
                    .hint_text("Search")
                    .desired_width(f32::INFINITY),
            ),
            "Search every category. Leave empty to browse one category.",
        );
        let _ = search;
        let search = self.browser.search.clone();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(132.0, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(132.0);
                    self.category_list(ui);
                },
            );
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("browser_items")
                    .show(ui, |ui| {
                        if search.trim().is_empty() {
                            self.category_items(ui, self.browser.category, "");
                        } else {
                            for category in [
                                Category::Instruments,
                                Category::AudioEffects,
                                Category::MidiEffects,
                                Category::Plugins,
                                Category::Samples,
                                Category::Templates,
                            ] {
                                self.category_items(ui, category, &search);
                            }
                        }
                    });
                self.waveform_preview(ui);
            });
        });
    }

    fn category_list(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Categories").small().weak());
        for (category, name) in [
            (Category::Instruments, "🎹 Instruments"),
            (Category::Drums, "🥁 Drums"),
            (Category::AudioEffects, "🎚 Audio Effects"),
            (Category::MidiEffects, "🎼 MIDI Effects"),
            (Category::Plugins, "🔌 Plug-ins"),
            (Category::Samples, "🎵 Samples"),
            (Category::Templates, "📄 Templates"),
        ] {
            ui.selectable_value(&mut self.browser.category, category, name);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Places").small().weak());
        let folders = self.settings.sample_folders.clone();
        let mut remove = None;
        for (index, folder) in folders.iter().enumerate() {
            let name = Path::new(folder)
                .file_name()
                .map_or_else(|| folder.clone(), |n| n.to_string_lossy().into_owned());
            let response = ui
                .selectable_value(
                    &mut self.browser.category,
                    Category::Folder(index),
                    format!("📁 {name}"),
                )
                .on_hover_text(folder);
            response.context_menu(|ui| {
                if ui.button("Remove from Places").clicked() {
                    remove = Some(index);
                    ui.close();
                }
            });
        }
        if let Some(index) = remove {
            self.settings.sample_folders.remove(index);
            self.save_settings();
            self.browser.category = Category::Samples;
            self.rescan_samples();
        }
        if crate::ui_kit::hint(
            ui.small_button("Add Folder..."),
            "Add a folder of samples to Places.",
        )
        .clicked()
            && let Some(folder) = rfd::FileDialog::new()
                .set_title("Add sample folder")
                .pick_folder()
        {
            self.settings
                .sample_folders
                .push(folder.display().to_string());
            self.save_settings();
            self.rescan_samples();
        }
        if ui.small_button("Refresh").clicked() {
            self.rescan_samples();
        }
    }

    fn category_items(&mut self, ui: &mut egui::Ui, category: Category, search: &str) {
        let titled = !search.is_empty();
        let mut items: Vec<(BrowserItem, String)> = Vec::new();
        match category {
            Category::Instruments => {
                for choice in InstrumentChoice::BUILTIN {
                    let detail = choice.detail();
                    items.push((BrowserItem::Instrument(choice), detail));
                }
            }
            Category::AudioEffects => {
                for effect in fnoob_core::effects::BuiltinEffect::all() {
                    items.push((BrowserItem::Builtin(effect), "Built-in effect".to_owned()));
                }
            }
            Category::MidiEffects => {
                for effect in fnoob_core::midi_fx::MidiEffect::all() {
                    items.push((
                        BrowserItem::MidiEffect(effect),
                        "MIDI effect for MIDI tracks".to_owned(),
                    ));
                }
            }
            Category::Drums => {
                items.push((
                    BrowserItem::Instrument(InstrumentChoice::DrumSampler),
                    "16 one-shot pads".to_owned(),
                ));
                for path in &self.browser.samples {
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    if DRUM_TAGS.iter().any(|tag| has_tag(&name, tag)) {
                        items.push((
                            BrowserItem::Sample(path.clone()),
                            path.display().to_string(),
                        ));
                    }
                }
            }
            Category::Templates => {
                for path in &self.browser.templates {
                    items.push((
                        BrowserItem::Template(path.clone()),
                        "Start a new project from this template".to_owned(),
                    ));
                }
            }
            Category::Plugins => {
                for entry in &self.plugins.catalog.plugins {
                    let detail = entry.class.vendor.clone();
                    let item = if entry.class.instrument {
                        BrowserItem::Instrument(InstrumentChoice::Vst3(entry.clone()))
                    } else {
                        BrowserItem::Effect(entry.clone())
                    };
                    items.push((item, detail));
                }
            }
            Category::Samples | Category::Folder(_) => {
                let root = match category {
                    Category::Folder(index) => self.settings.sample_folders.get(index).cloned(),
                    _ => None,
                };
                for path in &self.browser.samples {
                    if root.as_ref().is_none_or(|r| path.starts_with(r)) {
                        items.push((
                            BrowserItem::Sample(path.clone()),
                            path.display().to_string(),
                        ));
                    }
                }
            }
        }
        items.retain(|(item, _)| matches(search, &item.label()));
        if matches!(category, Category::Samples | Category::Folder(_)) {
            if !titled {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                    for tag in SAMPLE_TAGS {
                        let on = self.browser.tag == Some(tag);
                        if ui
                            .selectable_label(on, RichText::new(tag).small())
                            .clicked()
                        {
                            self.browser.tag = if on { None } else { Some(tag) };
                        }
                    }
                });
            }
            if let Some(tag) = self.browser.tag {
                items.retain(|(item, _)| has_tag(&item.label(), tag));
            }
        }
        if titled && !items.is_empty() {
            let title = match category {
                Category::Instruments => "Instruments",
                Category::Drums => "Drums",
                Category::AudioEffects => "Audio Effects",
                Category::MidiEffects => "MIDI Effects",
                Category::Plugins => "Plug-ins",
                Category::Samples | Category::Folder(_) => "Samples",
                Category::Templates => "Templates",
            };
            ui.label(RichText::new(title).small().weak());
        }
        if items.is_empty() && !titled {
            let text = match category {
                Category::Plugins => "No plug-ins found. Open File > Plugin Manager.",
                Category::Samples | Category::Folder(_) => {
                    "No WAV files. Use Add Folder... under Places."
                }
                Category::Templates => "No templates yet. Use File > Save as Template.",
                _ => "Nothing here.",
            };
            ui.label(RichText::new(text).weak());
        }
        for (item, detail) in items {
            self.browser_row(ui, item, &detail);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_tags_match_common_names() {
        assert!(has_tag("Kick 01.wav", "Kick"));
        assert!(has_tag("OpenHat_120.wav", "Hihat"));
        assert!(has_tag("big crash.wav", "Cymbal"));
        assert!(!has_tag("snare.wav", "Kick"));
    }

    #[test]
    fn search_needs_every_word() {
        assert!(matches("", "anything"));
        assert!(matches("verb val", "ValhallaVintageVerb"));
        assert!(!matches("verb delay", "ValhallaVintageVerb"));
    }

    #[test]
    fn finds_wav_files_in_nested_folders() {
        let root = std::env::temp_dir().join(format!("fnoob-browser-{}", std::process::id()));
        std::fs::create_dir_all(root.join("kicks")).unwrap();
        std::fs::write(root.join("kicks").join("k1.WAV"), b"").unwrap();
        std::fs::write(root.join("notes.txt"), b"").unwrap();
        assert_eq!(
            find_audio_files(std::slice::from_ref(&root)),
            [root.join("kicks").join("k1.WAV")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
