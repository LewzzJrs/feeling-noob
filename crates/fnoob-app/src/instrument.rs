use crate::app::App;
use eframe::egui;
use fnoob_core::model::{
    Id, Instrument, MAX_ENVELOPE_MS, MAX_LEVEL_DB, MIN_LEVEL_DB, Project, SynthParams, Waveform,
};

fn set_synth(project: &mut Project, track: Id, params: SynthParams) {
    if let Some(t) = project.track_mut(track) {
        t.instrument = Some(Instrument::BasicSynth(params));
    }
}

impl App {
    pub(crate) fn synth_settings_window(&mut self, ctx: &egui::Context) {
        let Some(track) = self.synth_window else {
            return;
        };
        let Some((name, params)) = self
            .project
            .track(track)
            .and_then(|t| t.synth().map(|p| (t.name.clone(), p)))
        else {
            self.synth_window = None;
            return;
        };
        let mut open = true;
        egui::Window::new(format!("Basic Synth - {name}"))
            .id(egui::Id::unique("synth_settings"))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let mut waveform = params.waveform;
                ui.horizontal(|ui| {
                    ui.label("Waveform");
                    for w in Waveform::ALL {
                        ui.selectable_value(&mut waveform, w, w.label());
                    }
                });
                if waveform != params.waveform {
                    self.edit(|p| set_synth(p, track, SynthParams { waveform, ..params }));
                }
                let time = 0.0..=MAX_ENVELOPE_MS;
                let mut next = params;
                let rows = [
                    ("Attack", &mut next.attack_ms, time.clone(), " ms"),
                    ("Decay", &mut next.decay_ms, time.clone(), " ms"),
                    ("Release", &mut next.release_ms, time, " ms"),
                ];
                let mut responses = Vec::new();
                for (label, value, range, suffix) in rows {
                    let response = ui.add(
                        egui::Slider::new(value, range)
                            .logarithmic(true)
                            .suffix(suffix)
                            .text(label),
                    );
                    responses.push(response);
                }
                responses
                    .push(ui.add(egui::Slider::new(&mut next.sustain, 0.0..=1.0).text("Sustain")));
                responses.push(
                    ui.add(
                        egui::Slider::new(&mut next.level_db, MIN_LEVEL_DB..=MAX_LEVEL_DB)
                            .suffix(" dB")
                            .text("Level"),
                    ),
                );
                for response in &responses {
                    self.live_edit(response, |p| set_synth(p, track, next));
                }
                if ui.button("Reset").clicked() {
                    self.edit(|p| set_synth(p, track, SynthParams::default()));
                }
            });
        if !open {
            self.synth_window = None;
        }
    }
}
