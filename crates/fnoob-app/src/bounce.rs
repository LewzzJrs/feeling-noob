use crate::app::App;
use eframe::egui;
use fnoob_core::settings::RecordFormat;
use fnoob_core::time::Position;
use fnoob_engine::RenderPlan;
use fnoob_engine::bounce::{BounceJob, BounceRequest, start_bounce};
use fnoob_platform::plugin::Plugin;
use std::path::PathBuf;

const TAIL_SECONDS: u64 = 2;

pub struct BounceDialog {
    format: RecordFormat,
}

pub struct ActiveBounce {
    job: BounceJob,
    _plugins: Vec<Plugin>,
}

impl App {
    pub(crate) fn open_bounce_dialog(&mut self) {
        if self.bounce.is_none() {
            self.bounce_dialog = Some(BounceDialog {
                format: RecordFormat::Int24,
            });
        }
    }

    fn bounce_range(&self) -> Option<(u64, u64, String)> {
        let timeline = self.timeline();
        let range = self.project.loop_range;
        if range.enabled {
            let ts = self.project.time_signature;
            let label = format!(
                "Loop area {} to {}",
                Position::from_tick(range.start_tick, ts),
                Position::from_tick(range.end_tick, ts)
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
        let Some((start, frames, _)) = self.bounce_range() else {
            self.message = Some("There is nothing to bounce.".into());
            return;
        };
        let (map, processors, builtins, plugins) = self.bounce_plugins();
        let mut project = self.project.clone();
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
                })
            }
            Err(e) => self.message = Some(format!("Bounce failed: {e}")),
        }
    }

    pub(crate) fn bounce_windows(&mut self, ctx: &egui::Context) {
        if let Some(mut format) = self.bounce_dialog.as_ref().map(|d| d.format) {
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
                ui.label("Muted tracks are left out. The metronome is not included.");
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
            }
            match choice {
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
        egui::Modal::new(egui::Id::unique("bounce_progress")).show(ctx, |ui| {
            ui.heading("Bouncing...");
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
            self.message = Some(match outcome.result {
                Ok(path) => format!("Bounced to {}", path.display()),
                Err(e) if e == "cancelled" => "Bounce cancelled.".to_owned(),
                Err(e) => format!("Bounce failed: {e}"),
            });
            drop(outcome.engine);
        }
    }
}
