use crate::app::App;
use eframe::egui;
use fnoob_core::io::{self, AUDIO_DIR};
use fnoob_core::model::Project;
use fnoob_core::settings::default_settings_path;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub const AUTOSAVE_EVERY: Duration = Duration::from_secs(60);
const UNUSED_DIR: &str = "unused";

pub enum Recovery {
    Untitled(Box<Project>),
    Autosave {
        dir: PathBuf,
        autosave: Box<Project>,
        saved: Option<Box<Project>>,
        when: SystemTime,
    },
}

pub struct Autosave {
    pub last_try: Instant,
    pub written: Option<Project>,
}

impl Default for Autosave {
    fn default() -> Self {
        Self {
            last_try: Instant::now(),
            written: None,
        }
    }
}

pub fn recovery_dir() -> Option<PathBuf> {
    default_settings_path().map(|p| p.with_file_name("recovery"))
}

fn ago(when: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(when)
        .map_or(0, |d| d.as_secs());
    match secs {
        0..=59 => "less than a minute ago".to_owned(),
        60..=3599 => format!("{} minutes ago", secs / 60),
        3600..=86_399 => format!("{} hours ago", secs / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}

fn referenced_audio(projects: &[&Project]) -> HashSet<String> {
    projects
        .iter()
        .flat_map(|p| p.audio_files.iter())
        .map(|f| f.path.replace('\\', "/").to_lowercase())
        .collect()
}

pub fn unused_audio(dir: &Path, projects: &[&Project]) -> Vec<String> {
    let used = referenced_audio(projects);
    let Ok(entries) = std::fs::read_dir(dir.join(AUDIO_DIR)) else {
        return Vec::new();
    };
    let mut unused: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(|n| format!("{AUDIO_DIR}/{n}")))
        .filter(|rel| !used.contains(&rel.to_lowercase()))
        .collect();
    unused.sort();
    unused
}

impl App {
    fn autosave_dir(&self) -> Option<PathBuf> {
        self.project_dir.clone().or_else(recovery_dir)
    }

    pub(crate) fn tick_autosave(&mut self, ctx: &egui::Context) {
        let elapsed = self.autosave.last_try.elapsed();
        if elapsed < AUTOSAVE_EVERY {
            if self.is_dirty() {
                ctx.request_repaint_after(AUTOSAVE_EVERY - elapsed + Duration::from_millis(50));
            }
            return;
        }
        self.autosave.last_try = Instant::now();
        if !self.is_dirty() || self.autosave.written.as_ref() == Some(&self.project) {
            return;
        }
        let Some(dir) = self.autosave_dir() else {
            return;
        };
        self.capture_plugin_states();
        match io::save_autosave(&dir, &self.project) {
            Ok(()) => self.autosave.written = Some(self.project.clone()),
            Err(e) => self.message = Some(format!("Autosave failed: {e}")),
        }
    }

    pub(crate) fn discard_autosave(&mut self) {
        if let Some(dir) = self.autosave_dir() {
            io::remove_autosave(&dir);
        }
        self.autosave.written = None;
    }

    pub(crate) fn after_manual_save(&mut self, was_untitled: bool) {
        if was_untitled && let Some(dir) = recovery_dir() {
            io::remove_autosave(&dir);
        }
        self.autosave.written = None;
        if let Some(dir) = self.project_dir.clone() {
            self.settings.add_recent(&dir);
            self.save_settings();
        }
    }

    pub(crate) fn check_startup_recovery(&mut self) {
        let Some(dir) = recovery_dir() else {
            return;
        };
        if let Ok(project) = io::load_autosave(&dir) {
            self.recovery = Some(Recovery::Untitled(Box::new(project)));
        }
    }

    pub(crate) fn offer_autosave(&mut self, dir: &Path) -> bool {
        let Some(when) = io::newer_autosave(dir) else {
            return false;
        };
        let Ok(autosave) = io::load_autosave(dir) else {
            return false;
        };
        self.recovery = Some(Recovery::Autosave {
            dir: dir.to_path_buf(),
            autosave: Box::new(autosave),
            saved: io::load(dir).ok().map(Box::new),
            when,
        });
        true
    }

    pub(crate) fn recovery_window(&mut self, ctx: &egui::Context) {
        let Some(recovery) = &self.recovery else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("recovery")).show(ctx, |ui| match recovery {
            Recovery::Untitled(project) => {
                ui.heading("Recover unsaved project");
                ui.label(format!(
                    "\"{}\" was not saved when the app closed last time.",
                    project.name
                ));
                ui.horizontal(|ui| {
                    if ui.button("Recover").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(false);
                    }
                });
            }
            Recovery::Autosave { when, saved, .. } => {
                ui.heading("Newer autosave found");
                ui.label(format!(
                    "This project has an autosave from {}, newer than the last save.",
                    ago(*when)
                ));
                ui.horizontal(|ui| {
                    if ui.button("Open autosave").clicked() {
                        choice = Some(true);
                    }
                    if ui
                        .add_enabled(saved.is_some(), egui::Button::new("Open last save"))
                        .clicked()
                    {
                        choice = Some(false);
                    }
                });
            }
        });
        let Some(use_autosave) = choice else {
            return;
        };
        match self.recovery.take() {
            Some(Recovery::Untitled(project)) => {
                if use_autosave {
                    let blank = Project::new("Untitled", project.sample_rate);
                    self.replace_project(*project, None);
                    self.saved = blank;
                } else if let Some(dir) = recovery_dir() {
                    io::remove_autosave(&dir);
                }
            }
            Some(Recovery::Autosave {
                dir,
                autosave,
                saved,
                ..
            }) => {
                if use_autosave {
                    let previous = saved.map(|s| *s);
                    self.replace_project(*autosave, Some(dir.clone()));
                    if let Some(previous) = previous {
                        self.saved = previous;
                    } else {
                        self.saved = Project::new("", self.project.sample_rate);
                    }
                } else if let Some(saved) = saved {
                    self.replace_project(*saved, Some(dir.clone()));
                }
                self.settings.add_recent(&dir);
                self.save_settings();
            }
            None => {}
        }
    }

    pub(crate) fn project_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_project_settings;
        egui::Window::new("Project Settings")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Sample rate: {} Hz", self.project.sample_rate));
                ui.label(match &self.project_dir {
                    Some(dir) => format!("Folder: {}", dir.display()),
                    None => "Not saved yet".to_owned(),
                });
                ui.horizontal(|ui| {
                    ui.label("Count-in length");
                    let mut bars = self.project.metronome.count_in_bars;
                    let response =
                        ui.add(egui::DragValue::new(&mut bars).range(1..=8).suffix(" bars"));
                    self.live_edit(&response, |p| p.metronome.count_in_bars = bars);
                });
            });
        self.show_project_settings = open;
    }

    pub(crate) fn find_unused_audio(&mut self) {
        let Some(dir) = self.project_dir.clone() else {
            self.message = Some("The project is not saved yet.".into());
            return;
        };
        let mut projects: Vec<&Project> = vec![&self.project, &self.saved];
        projects.extend(self.history.snapshots());
        let unused = unused_audio(&dir, &projects);
        if unused.is_empty() {
            self.message = Some("No unused audio files.".into());
        } else {
            self.cleanup = Some(unused);
        }
    }

    pub(crate) fn cleanup_window(&mut self, ctx: &egui::Context) {
        let Some(files) = &self.cleanup else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("cleanup")).show(ctx, |ui| {
            ui.heading("Unused audio files");
            ui.label(format!(
                "{} files in audio/ are not used by the project or its undo history. Move them to audio/{UNUSED_DIR}/? Nothing is deleted.",
                files.len()
            ));
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for file in files {
                    ui.label(file);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Move").clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(false);
                }
            });
        });
        if choice == Some(true)
            && let (Some(dir), Some(files)) = (self.project_dir.clone(), self.cleanup.clone())
        {
            let target = dir.join(AUDIO_DIR).join(UNUSED_DIR);
            let moved = std::fs::create_dir_all(&target).map(|()| {
                files
                    .iter()
                    .filter(|rel| {
                        let source = dir.join(rel);
                        source
                            .file_name()
                            .is_some_and(|name| std::fs::rename(&source, target.join(name)).is_ok())
                    })
                    .count()
            });
            self.message = Some(match moved {
                Ok(n) => format!("Moved {n} files to audio/{UNUSED_DIR}/."),
                Err(e) => format!("Could not move files: {e}"),
            });
        }
        if choice.is_some() {
            self.cleanup = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnoob_core::edit::{add_audio_file, add_track};
    use fnoob_core::model::TrackKind;

    #[test]
    fn unused_files_ignore_ones_any_snapshot_uses() {
        let dir = std::env::temp_dir().join(format!("fnoob-unused-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(AUDIO_DIR)).unwrap();
        for name in ["a.wav", "b.wav", "c.wav"] {
            std::fs::write(dir.join(AUDIO_DIR).join(name), b"x").unwrap();
        }
        let mut now = Project::new("t", 48_000);
        add_track(&mut now, TrackKind::Audio);
        add_audio_file(&mut now, "audio/a.wav".into(), 1, 10);
        let mut old = now.clone();
        add_audio_file(&mut old, r"audio\B.wav".into(), 1, 10);
        assert_eq!(unused_audio(&dir, &[&now, &old]), ["audio/c.wav"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ages_read_naturally() {
        let now = SystemTime::now();
        assert_eq!(ago(now), "less than a minute ago");
        assert_eq!(ago(now - Duration::from_secs(300)), "5 minutes ago");
    }
}
