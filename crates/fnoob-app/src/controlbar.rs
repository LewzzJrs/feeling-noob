use crate::app::App;
use crate::ui_kit::{Icon, hint, icon_button};
use eframe::egui::{self, Color32, Layout, RichText, Ui};
use fnoob_core::keymap::{Command, shortcut_label};
use fnoob_core::model::Grid;
use fnoob_core::scale::{NOTE_NAMES, ScaleKind};
use fnoob_core::session::LaunchQuantize;
use fnoob_core::time::{MAX_BPM, MIN_BPM, ticks_per_bar};
use fnoob_engine::{Command as EngineCommand, TransportState};

const DENOMINATORS: [u32; 4] = [2, 4, 8, 16];
const RECORD: Color32 = Color32::from_rgb(235, 80, 80);
const PLAYING: Color32 = Color32::from_rgb(90, 200, 110);
const TAP_RESET_SECONDS: f64 = 2.0;
const TAP_COUNT: usize = 5;

fn with_keys(text: &str, command: Command) -> String {
    match shortcut_label(command) {
        Some(keys) => format!("{text} ({keys})"),
        None => text.to_owned(),
    }
}

pub fn tap_tempo(taps: &[f64]) -> Option<f64> {
    if taps.len() < 2 {
        return None;
    }
    let span = taps[taps.len() - 1] - taps[0];
    let interval = span / (taps.len() - 1) as f64;
    (interval > 0.0).then(|| (60.0 / interval).clamp(MIN_BPM, MAX_BPM))
}

impl App {
    fn switch(&mut self, ui: &mut Ui, on: bool, text: &str, info: &str, command: Command) {
        if hint(ui.selectable_label(on, text), &with_keys(info, command)).clicked() {
            self.run(command);
        }
    }

    fn tap(&mut self, now: f64) {
        if self
            .tap_times
            .last()
            .is_some_and(|last| now - last > TAP_RESET_SECONDS)
        {
            self.tap_times.clear();
        }
        self.tap_times.push(now);
        if self.tap_times.len() > TAP_COUNT {
            self.tap_times.remove(0);
        }
        if let Some(bpm) = tap_tempo(&self.tap_times) {
            let bpm = (bpm * 100.0).round() / 100.0;
            self.edit(|p| p.tempo_bpm = bpm);
        }
    }

    pub(crate) fn control_bar(&mut self, ui: &mut Ui) {
        let state = self.transport_state();
        let recording = self.audio.as_ref().is_some_and(|(_, e)| e.is_recording());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            if hint(
                ui.selectable_label(self.browser.open, "≡"),
                &with_keys("Show or hide the browser.", Command::ToggleBrowser),
            )
            .clicked()
            {
                self.run(Command::ToggleBrowser);
            }
            ui.separator();
            if hint(
                ui.button("Tap"),
                "Tap tempo: click in time with the music to set the tempo.",
            )
            .clicked()
            {
                let now = ui.input(|i| i.time);
                self.tap(now);
            }
            let mut bpm = self.project.tempo_bpm;
            let tempo = ui.add(
                egui::DragValue::new(&mut bpm)
                    .range(MIN_BPM..=MAX_BPM)
                    .speed(0.2)
                    .fixed_decimals(2),
            );
            let tempo = hint(
                tempo,
                "Tempo in beats per minute. Drag or double-click to type.",
            );
            self.live_edit(&tempo, |p| p.tempo_bpm = bpm);
            let mut numerator = self.project.time_signature.numerator;
            let num = hint(
                ui.add(
                    egui::DragValue::new(&mut numerator)
                        .range(1..=16)
                        .speed(0.1),
                ),
                "Time signature: beats per bar.",
            );
            self.live_edit(&num, |p| p.time_signature.numerator = numerator);
            ui.label("/");
            let mut denominator = self.project.time_signature.denominator;
            egui::ComboBox::from_id_salt("denominator")
                .width(36.0)
                .selected_text(denominator.to_string())
                .show_ui(ui, |ui| {
                    for d in DENOMINATORS {
                        ui.selectable_value(&mut denominator, d, d.to_string());
                    }
                });
            if denominator != self.project.time_signature.denominator {
                self.edit(|p| p.time_signature.denominator = denominator);
            }
            let (metronome, count_in) = (
                self.project.metronome.enabled,
                self.project.metronome.count_in_enabled,
            );
            self.switch(
                ui,
                metronome,
                "Click",
                "Metronome.",
                Command::ToggleMetronome,
            );
            self.switch(
                ui,
                count_in,
                "1 Bar In",
                "Count-in: one bar of clicks before recording starts.",
                Command::ToggleCountIn,
            );
            let mut quantize = self.project.launch_quantize;
            let combo = egui::ComboBox::from_id_salt("launch_quantize")
                .width(64.0)
                .selected_text(quantize.label())
                .show_ui(ui, |ui| {
                    for q in LaunchQuantize::ALL {
                        ui.selectable_value(&mut quantize, q, q.label());
                    }
                });
            hint(
                combo.response,
                "Launch quantization: clips and scenes start on this grid.",
            );
            if quantize != self.project.launch_quantize {
                self.edit(|p| p.launch_quantize = quantize);
            }
            self.scale_controls(ui);
            ui.separator();
            self.switch(
                ui,
                self.view.follow,
                "Follow",
                "Follow: scroll the timeline with the playhead.",
                Command::ToggleFollow,
            );
            let position = self.project.meter().position(self.playhead_tick());
            let text = if state == TransportState::CountIn {
                "count-in".to_owned()
            } else {
                position.to_string()
            };
            hint(
                ui.label(RichText::new(text).monospace().size(16.0)),
                "Song position in bars, beats and sixteenths.",
            );
            ui.add_enabled_ui(self.audio.is_some(), |ui| {
                let playing = state != TransportState::Stopped;
                if hint(
                    icon_button(ui, Icon::Play, playing, PLAYING),
                    &with_keys("Play.", Command::PlayStop),
                )
                .clicked()
                    && !playing
                    && self.rate_matches()
                {
                    self.send(EngineCommand::Play);
                }
                let stop = hint(
                    icon_button(ui, Icon::Stop, false, PLAYING),
                    "Stop. Press again to go back to the start.",
                );
                if stop.clicked() {
                    if playing {
                        self.send(EngineCommand::Stop);
                    } else {
                        self.locate(0);
                    }
                }
                if hint(
                    icon_button(ui, Icon::Record, recording, RECORD),
                    &with_keys("Record on armed tracks.", Command::Record),
                )
                .clicked()
                {
                    self.run(Command::Record);
                }
            });
            if hint(
                ui.button("Capture"),
                &with_keys(
                    "Capture MIDI: turn what you just played into a clip.",
                    Command::CaptureMidi,
                ),
            )
            .clicked()
            {
                self.run(Command::CaptureMidi);
            }
            ui.separator();
            self.loop_controls(ui);
            ui.separator();
            self.switch(
                ui,
                self.draw_mode,
                "Draw",
                "Draw mode: click to add notes and clips.",
                Command::ToggleDrawMode,
            );
            let typing = self.musical.on;
            let keys = self.musical.label();
            self.switch(
                ui,
                typing,
                &keys,
                "Computer MIDI keyboard: play notes with the letter keys.",
                Command::ToggleMusicalTyping,
            );
            ui.separator();
            if let Some((_, engine)) = &self.audio {
                let load = engine.cpu_load();
                self.cpu = load.max(self.cpu * 0.97);
                hint(
                    ui.label(RichText::new(format!("CPU {:.0}%", self.cpu * 100.0)).monospace()),
                    "Audio processing load. Above 100% the sound breaks up.",
                );
            }
            ui.separator();
            let grid = self.session.open;
            self.switch(
                ui,
                !grid,
                "Timeline",
                "Timeline view: arrange regions over time.",
                Command::ToggleClipGrid,
            );
            self.switch(
                ui,
                grid,
                "Grid",
                "Clip grid view: launch clips and scenes.",
                Command::ToggleClipGrid,
            );
        });
    }

    fn snap_controls(&mut self, ui: &mut Ui) {
        let mut grid_size = self.project.snap.grid;
        let combo = egui::ComboBox::from_id_salt("grid")
            .width(48.0)
            .selected_text(grid_size.label())
            .show_ui(ui, |ui| {
                for g in Grid::ALL {
                    ui.selectable_value(&mut grid_size, g, g.label());
                }
            });
        hint(combo.response, "Grid size for snapping and editing.");
        if grid_size != self.project.snap.grid {
            self.edit(|p| p.snap.grid = grid_size);
        }
        let snap = self.project.snap.enabled;
        self.switch(ui, snap, "Snap", "Snap to grid.", Command::ToggleSnap);
    }

    fn scale_controls(&mut self, ui: &mut Ui) {
        let scale = self.project.scale;
        if hint(
            ui.selectable_label(scale.enabled, "Key"),
            "Scale mode: highlight the notes of the chosen key in the piano roll.",
        )
        .clicked()
        {
            self.edit(|p| p.scale.enabled = !p.scale.enabled);
        }
        let mut next = scale;
        let root = egui::ComboBox::from_id_salt("scale_root")
            .width(36.0)
            .selected_text(NOTE_NAMES[usize::from(scale.root % 12)])
            .show_ui(ui, |ui| {
                for (index, name) in NOTE_NAMES.iter().enumerate() {
                    ui.selectable_value(&mut next.root, index as u8, *name);
                }
            });
        hint(root.response, "Root note of the scale.");
        let kind = egui::ComboBox::from_id_salt("scale_kind")
            .width(96.0)
            .selected_text(scale.kind.label())
            .show_ui(ui, |ui| {
                for kind in ScaleKind::ALL {
                    ui.selectable_value(&mut next.kind, kind, kind.label());
                }
            });
        hint(kind.response, "Scale name.");
        if next != scale {
            self.edit(|p| {
                p.scale = next;
                p.scale.enabled = true;
            });
        }
    }

    fn loop_controls(&mut self, ui: &mut Ui) {
        let bar = ticks_per_bar(self.project.time_signature);
        let range = self.project.loop_range;
        let punch = self.project.punch;
        if hint(
            ui.selectable_label(punch.punch_in, "In"),
            "Punch in: recording starts at the start of the loop.",
        )
        .clicked()
        {
            self.edit(|p| p.punch.punch_in = !p.punch.punch_in);
        }
        self.switch(
            ui,
            range.enabled,
            "Loop",
            "Loop the loop area.",
            Command::ToggleLoop,
        );
        if hint(
            ui.selectable_label(punch.punch_out, "Out"),
            "Punch out: recording stops at the end of the loop.",
        )
        .clicked()
        {
            self.edit(|p| p.punch.punch_out = !p.punch.punch_out);
        }
        let mut start = range.start_tick / bar + 1;
        let response = hint(
            ui.add(egui::DragValue::new(&mut start).range(1..=9999).speed(0.1)),
            "Loop start (bar).",
        );
        let length_bars = range
            .end_tick
            .saturating_sub(range.start_tick)
            .div_ceil(bar)
            .max(1);
        self.live_edit(&response, |p| {
            p.loop_range.start_tick = (start - 1) * bar;
            p.loop_range.end_tick = p.loop_range.start_tick + length_bars * bar;
        });
        let mut length = length_bars;
        let response = hint(
            ui.add(
                egui::DragValue::new(&mut length)
                    .range(1..=999)
                    .speed(0.1)
                    .suffix(" bars"),
            ),
            "Loop length in bars.",
        );
        self.live_edit(&response, |p| {
            p.loop_range.end_tick = p.loop_range.start_tick + length * bar;
        });
    }

    fn position_status(&self) -> Option<String> {
        use crate::arrange::{clock_text, length_text, position_text, seconds_at};
        if self.session.open {
            return None;
        }
        let meter = self.project.meter();
        let timeline = self.timeline();
        let rate = self.sample_rate();
        let seconds = |tick| seconds_at(timeline, tick, rate);
        Some(match &self.time_selection {
            Some(selection) => format!(
                "Time Selection   Start: {}   End: {}   Length: {} (Duration: {})",
                position_text(&meter, selection.start),
                position_text(&meter, selection.end),
                length_text(selection.length(), self.project.time_signature),
                clock_text(seconds(selection.end) - seconds(selection.start)),
            ),
            None => format!(
                "Insert Mark {} (Time: {})",
                position_text(&meter, self.insert_tick),
                clock_text(seconds(self.insert_tick)),
            ),
        })
    }

    pub(crate) fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let text = self.info.clone().or_else(|| self.message.clone());
            match (text, self.position_status()) {
                (Some(text), _) => ui.label(text),
                (None, Some(status)) => ui.label(status),
                (None, None) => {
                    ui.label(RichText::new("Point at a control to learn what it does.").weak())
                }
            };
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                self.snap_controls(ui);
                ui.separator();
                if let Some(track) = self.selection.track.and_then(|id| self.project.track(id)) {
                    ui.label(RichText::new(&track.name).strong());
                }
                ui.separator();
                match &self.audio {
                    Some((output, _)) => ui.label(
                        RichText::new(format!(
                            "{}  |  In: {}  |  MIDI: {}",
                            output.device_name,
                            self.input
                                .as_ref()
                                .map_or("none", |i| i.device_name.as_str()),
                            self.midi.as_ref().map_or("none", |m| m.name.as_str())
                        ))
                        .small()
                        .weak(),
                    ),
                    None => ui.colored_label(ui.visuals().warn_fg_color, "No audio output"),
                };
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tapping_twice_a_second_gives_120_bpm() {
        assert_eq!(tap_tempo(&[1.0]), None);
        assert_eq!(tap_tempo(&[1.0, 1.5, 2.0, 2.5]), Some(120.0));
        assert_eq!(tap_tempo(&[0.0, 10.0]), Some(MIN_BPM.max(6.0)));
    }
}
