use crate::app::App;
use eframe::egui;
use fnoob_core::settings::RecordFormat;
use fnoob_engine::RenderPlan;
use fnoob_engine::bounce::{BounceJob, BounceRequest, start_bounce};
use fnoob_platform::plugin::Plugin;
use std::path::PathBuf;

const TAIL_SECONDS: u64 = 2;

pub struct BounceDialog {
    format: RecordFormat,
    stems: bool,
}

pub struct ActiveBounce {
    job: BounceJob,
    _plugins: Vec<Plugin>,
    freeze: Option<(fnoob_core::model::Id, String)>,
    stems: Option<StemQueue>,
}

pub struct StemQueue {
    format: RecordFormat,
    left: Vec<(fnoob_core::model::Id, PathBuf)>,
    done: usize,
    total: usize,
    dir: PathBuf,
}

const FREEZE_GAIN_DB: f32 = 3.010_3;

impl App {
    pub(crate) fn open_bounce_dialog(&mut self) {
        if self.bounce.is_none() {
            self.bounce_dialog = Some(BounceDialog {
                format: RecordFormat::Int24,
                stems: false,
            });
        }
    }

    fn bounce_range(&self) -> Option<(u64, u64, String)> {
        let timeline = self.timeline();
        let range = self.project.loop_range;
        if range.enabled {
            let meter = self.project.meter();
            let label = format!(
                "Loop area {} to {}",
                meter.position(range.start_tick),
                meter.position(range.end_tick)
            );
            let start = timeline.tick_to_sample(range.start_tick);
            let end = timeline.tick_to_sample(range.end_tick);
            return (end > start).then_some((start, end - start, label));
        }
        let end = self.project.end_tick();
        if end == 0 {
            return None;
        }
        let frames = timeline.tick_to_sample(end) + u64::from(self.sample_rate()) * TAIL_SECONDS;
        Some((0, frames, "Whole song plus a 2 second tail".to_owned()))
    }

    fn start_bounce(&mut self, format: RecordFormat, path: PathBuf) {
        let project = self.project.clone();
        self.start_bounce_of(project, format, path, None);
    }

    fn start_stems(&mut self, format: RecordFormat, dir: PathBuf) {
        let left: Vec<(fnoob_core::model::Id, PathBuf)> = self
            .project
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let name = format!("{:02} {}.wav", i + 1, crate::audio::file_stem_for(&t.name));
                (t.id, dir.join(name))
            })
            .collect();
        let queue = StemQueue {
            format,
            total: left.len(),
            left,
            done: 0,
            dir,
        };
        self.next_stem(queue);
    }

    fn next_stem(&mut self, mut queue: StemQueue) {
        if queue.left.is_empty() {
            self.message = Some(format!(
                "Exported {} stems to {}",
                queue.done,
                queue.dir.display()
            ));
            return;
        }
        let (id, path) = queue.left.remove(0);
        let mut project = self.project.clone();
        for track in &mut project.tracks {
            track.solo = track.id == id;
            if track.id == id {
                track.mute = false;
                if matches!(track.output, fnoob_core::model::Output::Track(_)) {
                    track.output = fnoob_core::model::Output::Main;
                }
            }
        }
        let format = queue.format;
        self.start_bounce_of(project, format, path, Some(queue));
    }

    fn start_bounce_of(
        &mut self,
        mut project: fnoob_core::model::Project,
        format: RecordFormat,
        path: PathBuf,
        stems: Option<StemQueue>,
    ) {
        let Some((start, frames, _)) = self.bounce_range() else {
            self.message = Some("There is nothing to bounce.".into());
            return;
        };
        let (map, processors, builtins, plugins) = self.bounce_plugins();
        project.loop_range.enabled = false;
        let rate = self.sample_rate();
        let mut plan = RenderPlan::build(&project, rate, &self.pool, &map);
        plan.metronome = false;
        plan.count_in_beats = 0;
        let request = BounceRequest {
            plan: Box::new(plan),
            sample_rate: rate,
            start,
            frames,
            path,
            format,
            plugins: processors,
            builtins,
        };
        match start_bounce(request) {
            Ok(job) => {
                self.bounce = Some(ActiveBounce {
                    job,
                    _plugins: plugins,
                    freeze: None,
                    stems,
                })
            }
            Err(e) => self.message = Some(format!("Bounce failed: {e}")),
        }
    }

    pub(crate) fn freeze_track(&mut self, id: fnoob_core::model::Id) {
        use fnoob_core::model::{SILENT_DB, TrackKind};
        let Some(track) = self.project.track(id).cloned() else {
            return;
        };
        if track.frozen.is_some() {
            self.edit_track(id, |t| t.frozen = None);
            self.message = Some(format!("{} is no longer frozen.", track.name));
            return;
        }
        if track.kind == TrackKind::Group {
            self.message = Some("Group tracks cannot be frozen; freeze their tracks.".into());
            return;
        }
        if self.bounce.is_some() {
            return;
        }
        let end = track
            .regions
            .iter()
            .map(|r| r.end_tick())
            .max()
            .unwrap_or(0);
        if end == 0 {
            self.message = Some("This track has no clips to freeze.".into());
            return;
        }
        if self.project_dir.is_none() && !self.save_as() {
            self.message = Some(
                "Frozen audio is kept in the project folder, so save the project first.".into(),
            );
            return;
        }
        let Some(dir) = self.project_dir.clone() else {
            return;
        };
        let stem = format!("{} Freeze", crate::audio::file_stem_for(&track.name));
        let rel = crate::audio::unique_audio_path(&dir, &stem, &std::collections::HashSet::new());
        let rate = self.sample_rate();
        let frames = self.timeline().tick_to_sample(end) + u64::from(rate) * TAIL_SECONDS;
        let (map, processors, builtins, plugins) = self.bounce_plugins();
        let mut project = self.project.clone();
        project.loop_range.enabled = false;
        project.master.volume_db = FREEZE_GAIN_DB;
        for other in &mut project.tracks {
            if other.id == id {
                other.mute = false;
                other.solo = false;
                other.group = None;
                other.output = fnoob_core::model::Output::Main;
                other.strip.volume_db = 0.0;
                other.strip.pan = 0.0;
                for send in &mut other.strip.sends {
                    send.level_db = SILENT_DB;
                }
                other.automation = Default::default();
            } else {
                other.mute = true;
                other.solo = false;
            }
        }
        let mut plan = RenderPlan::build(&project, rate, &self.pool, &map);
        plan.metronome = false;
        plan.count_in_beats = 0;
        let request = BounceRequest {
            plan: Box::new(plan),
            sample_rate: rate,
            start: 0,
            frames,
            path: dir.join(&rel),
            format: RecordFormat::Float32,
            plugins: processors,
            builtins,
        };
        match start_bounce(request) {
            Ok(job) => {
                self.bounce = Some(ActiveBounce {
                    job,
                    _plugins: plugins,
                    freeze: Some((id, rel)),
                    stems: None,
                })
            }
            Err(e) => self.message = Some(format!("Freeze failed: {e}")),
        }
    }

    fn finish_freeze(&mut self, id: fnoob_core::model::Id, rel: &str, path: &std::path::Path) {
        let rate = self.project.sample_rate;
        match fnoob_engine::decode::load_any(path, rate) {
            Ok(data) => {
                let (channels, frames) = (data.channels, data.frames);
                let mut file = None;
                self.edit(|p| {
                    let added =
                        fnoob_core::edit::add_audio_file(p, rel.to_owned(), channels, frames);
                    if let Some(track) = p.track_mut(id) {
                        track.frozen = Some(added);
                    }
                    file = Some(added);
                });
                if let Some(file) = file {
                    self.pool.insert(file, std::sync::Arc::new(data));
                }
                self.sync_plan();
                self.message =
                    Some("Track frozen. Its devices are off until you unfreeze it.".into());
            }
            Err(e) => self.message = Some(format!("Freeze failed: {e}")),
        }
    }

    pub(crate) fn bounce_windows(&mut self, ctx: &egui::Context) {
        if let Some((mut format, mut stems)) =
            self.bounce_dialog.as_ref().map(|d| (d.format, d.stems))
        {
            let range = self.bounce_range();
            let rate = self.sample_rate();
            let mut choice = None;
            egui::Modal::new(egui::Id::unique("bounce_dialog")).show(ctx, |ui| {
                ui.heading("Bounce to WAV");
                match &range {
                    Some((_, frames, label)) => {
                        ui.label(label);
                        ui.label(format!(
                            "{:.1} seconds, stereo, {} Hz",
                            *frames as f64 / f64::from(rate),
                            rate
                        ));
                    }
                    None => {
                        ui.label("The project has no regions to bounce.");
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Format");
                    for option in RecordFormat::ALL {
                        ui.selectable_value(&mut format, option, option.label());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Render");
                    ui.selectable_value(&mut stems, false, "Main mix");
                    ui.selectable_value(&mut stems, true, "All tracks (stems)");
                });
                ui.label(if stems {
                    "One file per track, with its effects, fader and sends. Files go into a folder you pick."
                } else {
                    "Muted tracks are left out. The metronome is not included."
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(range.is_some(), egui::Button::new("Bounce..."))
                        .clicked()
                    {
                        choice = Some(true);
                    }
                    if ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        choice = Some(false);
                    }
                });
            });
            if let Some(dialog) = &mut self.bounce_dialog {
                dialog.format = format;
                dialog.stems = stems;
            }
            match choice {
                Some(true) if stems => {
                    self.bounce_dialog = None;
                    let mut picker = rfd::FileDialog::new().set_title("Folder for the stems");
                    if let Some(dir) = self.project_dir.as_ref().and_then(|d| d.parent()) {
                        picker = picker.set_directory(dir);
                    }
                    if let Some(dir) = picker.pick_folder() {
                        self.start_stems(format, dir);
                    }
                }
                Some(true) => {
                    self.bounce_dialog = None;
                    let mut picker = rfd::FileDialog::new()
                        .set_title("Bounce to WAV")
                        .add_filter("WAV audio", &["wav"])
                        .set_file_name(format!("{}.wav", self.project.name));
                    if let Some(dir) = self.project_dir.as_ref().and_then(|d| d.parent()) {
                        picker = picker.set_directory(dir);
                    }
                    if let Some(path) = picker.save_file() {
                        self.start_bounce(format, path.with_extension("wav"));
                    }
                }
                Some(false) => self.bounce_dialog = None,
                None => {}
            }
        }
        let Some(active) = &self.bounce else {
            return;
        };
        let progress = active.job.progress();
        let mut cancel = false;
        let heading = match (&active.freeze, &active.stems) {
            (Some(_), _) => "Freezing...".to_owned(),
            (None, Some(queue)) => {
                format!("Exporting stem {} of {}...", queue.done + 1, queue.total)
            }
            (None, None) => "Bouncing...".to_owned(),
        };
        egui::Modal::new(egui::Id::unique("bounce_progress")).show(ctx, |ui| {
            ui.heading(&heading);
            ui.add(egui::ProgressBar::new(progress).show_percentage());
            cancel = ui.button("Cancel").clicked();
        });
        if cancel {
            active.job.cancel();
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        if active.job.is_finished()
            && let Some(active) = self.bounce.take()
        {
            let outcome = active.job.finish();
            if let Some((id, rel)) = active.freeze {
                drop(outcome.engine);
                match outcome.result {
                    Ok(path) => self.finish_freeze(id, &rel, &path),
                    Err(e) if e == "cancelled" => self.message = Some("Freeze cancelled.".into()),
                    Err(e) => self.message = Some(format!("Freeze failed: {e}")),
                }
                return;
            }
            if let Some(mut queue) = active.stems {
                drop(outcome.engine);
                match outcome.result {
                    Ok(_) => {
                        queue.done += 1;
                        self.next_stem(queue);
                    }
                    Err(e) if e == "cancelled" => {
                        self.message = Some(format!(
                            "Stem export cancelled after {} of {} files.",
                            queue.done, queue.total
                        ))
                    }
                    Err(e) => self.message = Some(format!("Stem export failed: {e}")),
                }
                return;
            }
            self.message = Some(match outcome.result {
                Ok(path) => format!("Bounced to {}", path.display()),
                Err(e) if e == "cancelled" => "Bounce cancelled.".to_owned(),
                Err(e) => format!("Bounce failed: {e}"),
            });
            drop(outcome.engine);
        }
    }
}
