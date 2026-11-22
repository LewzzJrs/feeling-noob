use crate::app::App;
use eframe::egui::{self, Response, Slider, Ui};
use fnoob_core::effects::{BuiltinEffect, DelaySync, MAX_DELAY_MS};
use fnoob_core::model::{Id, Instrument, PluginSlot, Project};

pub(crate) fn slot_mut(project: &mut Project, id: Id) -> Option<&mut PluginSlot> {
    project
        .tracks
        .iter_mut()
        .flat_map(|t| {
            let instrument = match &mut t.instrument {
                Some(Instrument::Vst3(slot)) => Some(slot),
                _ => None,
            };
            instrument.into_iter().chain(t.strip.inserts.iter_mut())
        })
        .chain(project.bus.inserts.iter_mut())
        .find(|s| s.id == id)
}

fn slot_of(project: &Project, id: Id) -> Option<&PluginSlot> {
    project
        .tracks
        .iter()
        .flat_map(|t| t.plugin_slots())
        .chain(project.bus.inserts.iter())
        .find(|s| s.id == id)
}

fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    text: &str,
    suffix: &str,
) -> Response {
    ui.add(Slider::new(value, range).text(text).suffix(suffix))
}

fn log_slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    text: &str,
    suffix: &str,
) -> Response {
    ui.add(
        Slider::new(value, range)
            .logarithmic(true)
            .text(text)
            .suffix(suffix),
    )
}

pub fn effect_controls(ui: &mut Ui, effect: &mut BuiltinEffect) -> Vec<Response> {
    let mut responses = Vec::new();
    match effect {
        BuiltinEffect::Eq(p) => {
            responses.push(log_slider(
                ui,
                &mut p.low_cut_hz,
                0.0..=1000.0,
                "Low cut (0 = off)",
                " Hz",
            ));
            responses.push(slider(
                ui,
                &mut p.low_gain_db,
                -24.0..=24.0,
                "Low gain",
                " dB",
            ));
            responses.push(log_slider(
                ui,
                &mut p.low_hz,
                20.0..=1000.0,
                "Low freq",
                " Hz",
            ));
            responses.push(slider(
                ui,
                &mut p.mid_gain_db,
                -24.0..=24.0,
                "Mid gain",
                " dB",
            ));
            responses.push(log_slider(
                ui,
                &mut p.mid_hz,
                100.0..=10_000.0,
                "Mid freq",
                " Hz",
            ));
            responses.push(log_slider(ui, &mut p.mid_q, 0.1..=10.0, "Mid Q", ""));
            responses.push(slider(
                ui,
                &mut p.high_gain_db,
                -24.0..=24.0,
                "High gain",
                " dB",
            ));
            responses.push(log_slider(
                ui,
                &mut p.high_hz,
                1000.0..=20_000.0,
                "High freq",
                " Hz",
            ));
        }
        BuiltinEffect::Compressor(p) => {
            responses.push(slider(
                ui,
                &mut p.threshold_db,
                -60.0..=0.0,
                "Threshold",
                " dB",
            ));
            responses.push(log_slider(ui, &mut p.ratio, 1.0..=20.0, "Ratio", ":1"));
            responses.push(log_slider(
                ui,
                &mut p.attack_ms,
                0.1..=200.0,
                "Attack",
                " ms",
            ));
            responses.push(log_slider(
                ui,
                &mut p.release_ms,
                5.0..=2000.0,
                "Release",
                " ms",
            ));
            responses.push(slider(ui, &mut p.makeup_db, 0.0..=24.0, "Makeup", " dB"));
        }
        BuiltinEffect::Delay(p) => {
            ui.horizontal_wrapped(|ui| {
                ui.label("Time");
                for sync in DelaySync::ALL {
                    let response = ui.selectable_value(&mut p.sync, sync, sync.label());
                    responses.push(response);
                }
            });
            if p.sync == DelaySync::Free {
                responses.push(log_slider(
                    ui,
                    &mut p.time_ms,
                    1.0..=MAX_DELAY_MS,
                    "Time",
                    " ms",
                ));
            }
            responses.push(slider(ui, &mut p.feedback, 0.0..=0.95, "Feedback", ""));
            responses.push(slider(ui, &mut p.mix, 0.0..=1.0, "Mix", ""));
            responses.push(ui.checkbox(&mut p.ping_pong, "Ping-pong"));
        }
        BuiltinEffect::Reverb(p) => {
            responses.push(slider(ui, &mut p.size, 0.0..=1.0, "Size", ""));
            responses.push(slider(ui, &mut p.damping, 0.0..=1.0, "Damping", ""));
            responses.push(slider(ui, &mut p.width, 0.0..=1.0, "Width", ""));
            responses.push(slider(ui, &mut p.mix, 0.0..=1.0, "Mix", ""));
        }
        BuiltinEffect::Limiter(p) => {
            responses.push(slider(ui, &mut p.gain_db, 0.0..=24.0, "Gain", " dB"));
            responses.push(slider(ui, &mut p.ceiling_db, -24.0..=0.0, "Ceiling", " dB"));
            responses.push(log_slider(
                ui,
                &mut p.release_ms,
                1.0..=1000.0,
                "Release",
                " ms",
            ));
        }
        BuiltinEffect::Utility(p) => {
            responses.push(slider(ui, &mut p.gain_db, -60.0..=24.0, "Gain", " dB"));
            responses.push(slider(ui, &mut p.width, 0.0..=2.0, "Width", ""));
            ui.horizontal(|ui| {
                responses.push(ui.checkbox(&mut p.mono, "Mono"));
                responses.push(ui.checkbox(&mut p.invert_left, "Invert L"));
                responses.push(ui.checkbox(&mut p.invert_right, "Invert R"));
            });
        }
    }
    responses
}

impl App {
    pub(crate) fn device_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.device_window else {
            return;
        };
        let Some(slot) = slot_of(&self.project, id).cloned() else {
            self.device_window = None;
            return;
        };
        let Some(effect) = slot.builtin else {
            self.device_window = None;
            return;
        };
        let mut open = true;
        let mut next = effect;
        let mut reset = false;
        let mut bypass = slot.bypass;
        let mut responses = Vec::new();
        egui::Window::new(format!("{} (built-in)", effect.label()))
            .id(egui::Id::unique("device_window"))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    responses.push(ui.checkbox(&mut bypass, "Bypass"));
                    reset = ui.button("Reset").clicked();
                });
                ui.separator();
                responses.extend(effect_controls(ui, &mut next));
            });
        for response in &responses {
            self.live_edit(response, |p| {
                if let Some(target) = slot_mut(p, id) {
                    target.builtin = Some(next);
                    target.bypass = bypass;
                }
            });
        }
        if reset
            && let Some(default) = BuiltinEffect::all()
                .into_iter()
                .find(|d| d.same_kind(&effect))
        {
            self.edit(|p| {
                if let Some(target) = slot_mut(p, id) {
                    target.builtin = Some(default);
                }
            });
        }
        if !open {
            self.device_window = None;
        }
    }
}
