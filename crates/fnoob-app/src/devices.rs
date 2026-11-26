use eframe::egui::{Response, Slider, Ui};
use fnoob_core::effects::{BuiltinEffect, DelaySync, FilterMode, MAX_DELAY_MS, SaturatorCurve};
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
        .chain(
            project
                .returns
                .iter_mut()
                .flat_map(|r| r.inserts.iter_mut()),
        )
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
        BuiltinEffect::AutoFilter(p) => {
            ui.horizontal(|ui| {
                for mode in FilterMode::ALL {
                    responses.push(ui.selectable_value(&mut p.mode, mode, mode.label()));
                }
            });
            responses.push(log_slider(
                ui,
                &mut p.cutoff_hz,
                20.0..=20_000.0,
                "Cutoff",
                " Hz",
            ));
            responses.push(log_slider(
                ui,
                &mut p.resonance,
                0.3..=12.0,
                "Resonance",
                "",
            ));
            responses.push(slider(ui, &mut p.lfo_hz, 0.0..=20.0, "LFO rate", " Hz"));
            responses.push(slider(ui, &mut p.lfo_depth, 0.0..=1.0, "LFO amount", ""));
        }
        BuiltinEffect::Chorus(p) => {
            responses.push(log_slider(ui, &mut p.rate_hz, 0.01..=10.0, "Rate", " Hz"));
            responses.push(slider(ui, &mut p.depth_ms, 0.0..=10.0, "Depth", " ms"));
            responses.push(slider(ui, &mut p.delay_ms, 1.0..=30.0, "Delay", " ms"));
            responses.push(slider(ui, &mut p.feedback, 0.0..=0.9, "Feedback", ""));
            responses.push(slider(ui, &mut p.mix, 0.0..=1.0, "Mix", ""));
        }
        BuiltinEffect::Saturator(p) => {
            ui.horizontal(|ui| {
                for curve in SaturatorCurve::ALL {
                    responses.push(ui.selectable_value(&mut p.curve, curve, curve.label()));
                }
            });
            responses.push(slider(ui, &mut p.drive_db, 0.0..=36.0, "Drive", " dB"));
            responses.push(slider(ui, &mut p.output_db, -24.0..=12.0, "Output", " dB"));
            responses.push(slider(ui, &mut p.mix, 0.0..=1.0, "Mix", ""));
        }
        BuiltinEffect::Gate(p) => {
            responses.push(slider(
                ui,
                &mut p.threshold_db,
                -80.0..=0.0,
                "Threshold",
                " dB",
            ));
            responses.push(log_slider(
                ui,
                &mut p.attack_ms,
                0.1..=100.0,
                "Attack",
                " ms",
            ));
            responses.push(slider(ui, &mut p.hold_ms, 0.0..=500.0, "Hold", " ms"));
            responses.push(log_slider(
                ui,
                &mut p.release_ms,
                5.0..=2000.0,
                "Release",
                " ms",
            ));
            responses.push(slider(ui, &mut p.floor_db, -80.0..=0.0, "Floor", " dB"));
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
