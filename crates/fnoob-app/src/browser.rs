use crate::app::App;
use crate::plugins::Owner;
use eframe::egui::{self, RichText};
use fnoob_core::catalog::CatalogEntry;
use fnoob_core::model::{Id, MAX_INSERTS, TrackKind};
use fnoob_engine::Command as EngineCommand;
use fnoob_engine::audio_data::load_wav;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_SAMPLES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserItem {
    Instrument(Option<CatalogEntry>),
    Effect(CatalogEntry),
    Sample(PathBuf),
}

impl BrowserItem {
    pub fn label(&self) -> String {
        match self {
            Self::Instrument(None) => "Basic Synth".to_owned(),
            Self::Instrument(Some(entry)) | Self::Effect(entry) => entry.class.name.clone(),
            Self::Sample(path) => path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        }
    }
}

#[derive(Default)]
pub struct Browser {
    pub open: bool,
    search: String,
    samples: Vec<PathBuf>,
    scanned: bool,
    previewing: Option<PathBuf>,
}

impl Browser {
    pub fn shown() -> Self {
        Self {
            open: true,
            ..Self::default()
        }
    }
}

fn find_wavs(folders: &[PathBuf]) -> Vec<PathBuf> {
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
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
            {
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
        self.browser.samples = find_wavs(&self.sample_folders());
        self.browser.scanned = true;
    }

    fn preview_sample(&mut self, path: &Path) {
        if self.browser.previewing.as_deref() == Some(path) {
            self.browser.previewing = None;
            self.send(EngineCommand::PreviewAudio(None));
            return;
        }
        match load_wav(path, self.sample_rate()) {
            Ok(data) => {
                self.browser.previewing = Some(path.to_path_buf());
                self.send(EngineCommand::PreviewAudio(Some(Arc::new(data))));
            }
            Err(e) => self.message = Some(format!("Cannot preview: {e}")),
        }
    }

    pub(crate) fn apply_browser_item(&mut self, item: &BrowserItem, track: Option<Id>, tick: u64) {
        match item {
            BrowserItem::Instrument(entry) => {
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
                self.set_instrument(target, entry.as_ref());
                self.selection.track = Some(target);
            }
            BrowserItem::Effect(entry) => {
                let Some(target) = track.or(self.selection.track) else {
                    self.message = Some("Select a track first, or drop the effect on one.".into());
                    return;
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
        ui.horizontal(|ui| {
            ui.label(RichText::new("Browser").strong());
            if ui.small_button("Close").clicked() {
                self.browser.open = false;
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.browser.search)
                .hint_text("Search")
                .desired_width(f32::INFINITY),
        );
        ui.label(
            RichText::new("Drag items onto a track, or double-click to add.")
                .small()
                .weak(),
        );
        let search = self.browser.search.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::CollapsingHeader::new("Instruments")
                .default_open(true)
                .show(ui, |ui| {
                    if matches(&search, "Basic Synth") {
                        self.browser_row(ui, BrowserItem::Instrument(None), "Built-in synthesizer");
                    }
                    let entries: Vec<CatalogEntry> = self
                        .plugins
                        .catalog
                        .plugins
                        .iter()
                        .filter(|e| e.class.instrument && matches(&search, &e.class.name))
                        .cloned()
                        .collect();
                    for entry in entries {
                        let detail = entry.class.vendor.clone();
                        self.browser_row(ui, BrowserItem::Instrument(Some(entry)), &detail);
                    }
                });
            egui::CollapsingHeader::new("Audio Effects")
                .default_open(true)
                .show(ui, |ui| {
                    let entries: Vec<CatalogEntry> = self
                        .plugins
                        .catalog
                        .plugins
                        .iter()
                        .filter(|e| !e.class.instrument && matches(&search, &e.class.name))
                        .cloned()
                        .collect();
                    if entries.is_empty() {
                        ui.label(RichText::new("No effects. Open File > Plugin Manager.").weak());
                    }
                    for entry in entries {
                        let detail = entry.class.vendor.clone();
                        self.browser_row(ui, BrowserItem::Effect(entry), &detail);
                    }
                });
            egui::CollapsingHeader::new("Samples")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("Add folder...").clicked()
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
                    });
                    let folders = self.settings.sample_folders.clone();
                    let mut remove = None;
                    for (index, folder) in folders.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(folder).small().weak());
                            if ui
                                .small_button("x")
                                .on_hover_text("Remove folder")
                                .clicked()
                            {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        self.settings.sample_folders.remove(index);
                        self.save_settings();
                        self.rescan_samples();
                    }
                    let samples: Vec<PathBuf> = self
                        .browser
                        .samples
                        .iter()
                        .filter(|p| matches(&search, &p.to_string_lossy()))
                        .cloned()
                        .collect();
                    if samples.is_empty() {
                        ui.label(RichText::new("No WAV files found.").weak());
                    }
                    for path in samples {
                        let detail = path.display().to_string();
                        self.browser_row(ui, BrowserItem::Sample(path), &detail);
                    }
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            find_wavs(std::slice::from_ref(&root)),
            [root.join("kicks").join("k1.WAV")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
