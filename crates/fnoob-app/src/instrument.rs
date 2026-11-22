use crate::app::App;
use crate::piano_roll::note_name;
use eframe::egui::{self, Response, RichText, Slider, Ui};
use fnoob_core::model::{
    DRUM_BASE_PITCH, DRUM_PADS, DrumParams, Id, Instrument, MAX_ENVELOPE_MS, MAX_LEVEL_DB,
    MIN_LEVEL_DB, Project, SamplerParams, SynthParams, Waveform,
};
use fnoob_engine::Command as EngineCommand;

fn set_instrument(project: &mut Project, track: Id, instrument: Instrument) {
    if let Some(t) = project.track_mut(track) {
        t.instrument = Some(instrument);
    }
}

#[derive(Clone, Copy)]
enum SampleTarget {
    Sampler,
    Pad(usize),
}

struct EnvelopeFields<'a> {
    attack_ms: &'a mut f32,
    decay_ms: &'a mut f32,
    sustain: &'a mut f32,
    release_ms: &'a mut f32,
    level_db: &'a mut f32,
}

fn envelope_controls(ui: &mut Ui, fields: EnvelopeFields) -> Vec<Response> {
    let time = 0.0..=MAX_ENVELOPE_MS;
    let mut responses = Vec::new();
    for (label, value) in [
        ("Attack", fields.attack_ms),
        ("Decay", fields.decay_ms),
        ("Release", fields.release_ms),
    ] {
        responses.push(
            ui.add(
                Slider::new(value, time.clone())
                    .logarithmic(true)
                    .suffix(" ms")
                    .text(label),
            ),
        );
    }
    responses.push(ui.add(Slider::new(fields.sustain, 0.0..=1.0).text("Sustain")));
    responses.push(
        ui.add(
            Slider::new(fields.level_db, MIN_LEVEL_DB..=MAX_LEVEL_DB)
                .suffix(" dB")
                .text("Level"),
        ),
    );
    responses
}

impl App {
    pub(crate) fn open_instrument(&mut self, track: Id) {
        if let Some(Instrument::Vst3(slot)) = self
            .project
            .track(track)
            .and_then(|t| t.instrument.as_ref())
        {
            let slot = slot.id;
            self.open_plugin_editor(slot);
            return;
        }
        self.selection.track = Some(track);
        self.chain_target = crate::chain::ChainTarget::SelectedTrack;
        self.show_devices();
    }

    fn sample_name(&self, file: Option<Id>) -> String {
        file.and_then(|id| self.project.audio_file(id)).map_or_else(
            || "(empty)".to_owned(),
            |f| {
                f.path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&f.path)
                    .to_owned()
            },
        )
    }

    fn load_sample(&mut self, track: Id, target: SampleTarget) {
        let Some(source) = rfd::FileDialog::new()
            .set_title("Load sample")
            .add_filter("WAV audio", &["wav"])
            .pick_file()
        else {
            return;
        };
        let Some((rel, data)) = self.copy_into_project(&source) else {
            return;
        };
        let (channels, frames) = (data.channels, data.frames);
        let mut file_id = None;
        self.edit(|p| {
            let id = fnoob_core::edit::add_audio_file(p, rel, channels, frames);
            file_id = Some(id);
            match (
                p.track_mut(track).and_then(|t| t.instrument.as_mut()),
                target,
            ) {
                (Some(Instrument::Sampler(params)), SampleTarget::Sampler) => {
                    params.file = Some(id);
                }
                (Some(Instrument::DrumSampler(params)), SampleTarget::Pad(pad)) => {
                    params.pads[pad].file = Some(id);
                }
                _ => {}
            }
        });
        if let Some(id) = file_id {
            self.pool.insert(id, std::sync::Arc::new(data));
            self.sync_plan();
        }
    }

    pub(crate) fn sampler_controls(&mut self, ui: &mut Ui, track: Id, params: SamplerParams) {
        ui.horizontal(|ui| {
            ui.label("Sample");
            ui.label(RichText::new(self.sample_name(params.file)).strong());
            if ui.button("Load...").clicked() {
                self.load_sample(track, SampleTarget::Sampler);
            }
            if ui
                .add_enabled(params.file.is_some(), egui::Button::new("Clear"))
                .clicked()
            {
                self.edit(|p| {
                    set_instrument(
                        p,
                        track,
                        Instrument::Sampler(SamplerParams {
                            file: None,
                            ..params
                        }),
                    );
                });
            }
        });
        let mut next = params;
        let mut responses = vec![
            ui.add(
                Slider::new(&mut next.root_pitch, 0..=127)
                    .text("Root note")
                    .custom_formatter(|v, _| note_name(v as u8)),
            ),
        ];
        responses.extend(envelope_controls(
            ui,
            EnvelopeFields {
                attack_ms: &mut next.attack_ms,
                decay_ms: &mut next.decay_ms,
                sustain: &mut next.sustain,
                release_ms: &mut next.release_ms,
                level_db: &mut next.level_db,
            },
        ));
        for response in &responses {
            self.live_edit(response, |p| {
                set_instrument(p, track, Instrument::Sampler(next));
            });
        }
        if ui.button("Reset").clicked() {
            self.edit(|p| {
                set_instrument(
                    p,
                    track,
                    Instrument::Sampler(SamplerParams {
                        file: params.file,
                        ..SamplerParams::default()
                    }),
                );
            });
        }
    }

    pub(crate) fn drum_controls(&mut self, ui: &mut Ui, track: Id, params: DrumParams) {
        let mut next = params;
        let mut responses = vec![
            ui.add(
                Slider::new(&mut next.level_db, MIN_LEVEL_DB..=MAX_LEVEL_DB)
                    .suffix(" dB")
                    .text("Level"),
            ),
        ];
        ui.label(
            RichText::new(format!(
                "Pads play from {} upward. Load a WAV file into each pad.",
                note_name(DRUM_BASE_PITCH)
            ))
            .small()
            .weak(),
        );
        let mut load = None;
        let mut clear = None;
        let mut play = None;
        egui::Grid::new("drum_pads")
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                for row in (0..DRUM_PADS / 4).rev() {
                    for index in row * 4..row * 4 + 4 {
                        let pad = &mut next.pads[index];
                        let pitch = DRUM_BASE_PITCH + index as u8;
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(pad.file.is_some(), egui::Button::new("Play"))
                                    .clicked()
                                {
                                    play = Some(pitch);
                                }
                                ui.label(RichText::new(note_name(pitch)).strong());
                            });
                            ui.label(RichText::new(self.sample_name(pad.file)).small());
                            ui.horizontal(|ui| {
                                if ui.small_button("Load").clicked() {
                                    load = Some(index);
                                }
                                if ui
                                    .add_enabled(
                                        pad.file.is_some(),
                                        egui::Button::new("Clear").small(),
                                    )
                                    .clicked()
                                {
                                    clear = Some(index);
                                }
                            });
                            responses.push(
                                ui.add(
                                    Slider::new(&mut pad.gain_db, MIN_LEVEL_DB..=MAX_LEVEL_DB)
                                        .suffix(" dB"),
                                ),
                            );
                            responses
                                .push(ui.add(Slider::new(&mut pad.pan, -1.0..=1.0).text("Pan")));
                        });
                    }
                    ui.end_row();
                }
            });
        for response in &responses {
            self.live_edit(response, |p| {
                set_instrument(p, track, Instrument::DrumSampler(Box::new(next)));
            });
        }
        if let Some(pitch) = play {
            self.send(EngineCommand::Preview {
                track,
                pitch,
                velocity: 110,
            });
        }
        if let Some(index) = load {
            self.load_sample(track, SampleTarget::Pad(index));
        }
        if let Some(index) = clear {
            self.edit(|p| {
                if let Some(Instrument::DrumSampler(d)) =
                    p.track_mut(track).and_then(|t| t.instrument.as_mut())
                {
                    d.pads[index].file = None;
                }
            });
        }
    }

    pub(crate) fn synth_controls(&mut self, ui: &mut Ui, track: Id, params: SynthParams) {
        let mut waveform = params.waveform;
        ui.horizontal(|ui| {
            ui.label("Waveform");
            for w in Waveform::ALL {
                ui.selectable_value(&mut waveform, w, w.label());
            }
        });
        if waveform != params.waveform {
            self.edit(|p| {
                set_instrument(
                    p,
                    track,
                    Instrument::BasicSynth(SynthParams { waveform, ..params }),
                );
            });
        }
        let mut next = params;
        let responses = envelope_controls(
            ui,
            EnvelopeFields {
                attack_ms: &mut next.attack_ms,
                decay_ms: &mut next.decay_ms,
                sustain: &mut next.sustain,
                release_ms: &mut next.release_ms,
                level_db: &mut next.level_db,
            },
        );
        for response in &responses {
            self.live_edit(response, |p| {
                set_instrument(p, track, Instrument::BasicSynth(next));
            });
        }
        if ui.button("Reset").clicked() {
            self.edit(|p| set_instrument(p, track, Instrument::BasicSynth(SynthParams::default())));
        }
    }
}
