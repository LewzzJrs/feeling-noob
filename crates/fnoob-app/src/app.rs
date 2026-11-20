use eframe::egui;
use fnoob_core::settings::{Settings, default_settings_path};
use fnoob_core::time::{MAX_BPM, MIN_BPM, bar_and_beat, samples_per_beat};
use fnoob_engine::device::{AudioOutput, OutputDeviceInfo, list_output_devices, open_output};
use fnoob_engine::{Command, EngineHandle, TransportSettings};
use std::path::PathBuf;
use std::time::Duration;

const BEATS_PER_BAR: u32 = 4;

pub struct App {
    settings: Settings,
    settings_path: Option<PathBuf>,
    devices: Vec<OutputDeviceInfo>,
    output: Option<(AudioOutput, EngineHandle)>,
    bpm: f64,
    metronome: bool,
    show_settings: bool,
    message: Option<String>,
}

impl App {
    pub fn new() -> Self {
        let settings_path = default_settings_path();
        let mut message = None;
        let settings = match settings_path.as_deref().map(Settings::load) {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                message = Some(format!("{e}. Using default settings."));
                Settings::default()
            }
            None => {
                message = Some("APPDATA is not set. Settings will not be saved.".into());
                Settings::default()
            }
        };
        let mut app = Self {
            settings,
            settings_path,
            devices: Vec::new(),
            output: None,
            bpm: 120.0,
            metronome: true,
            show_settings: false,
            message,
        };
        app.refresh_devices();
        app.open_device();
        app
    }

    fn transport_settings(&self) -> TransportSettings {
        TransportSettings {
            bpm: self.bpm,
            beats_per_bar: BEATS_PER_BAR,
            metronome: self.metronome,
        }
    }

    fn refresh_devices(&mut self) {
        match list_output_devices() {
            Ok(devices) => self.devices = devices,
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    fn open_device(&mut self) {
        self.output = None;
        match open_output(
            self.settings.output_device.as_deref(),
            self.transport_settings(),
        ) {
            Ok(output) => self.output = Some(output),
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    fn select_device(&mut self, id: Option<String>) {
        if self.settings.output_device == id {
            return;
        }
        self.settings.output_device = id;
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            self.message = Some(e.to_string());
        }
        self.open_device();
    }

    fn send(&mut self, command: Command) {
        if let Some((_, engine)) = &mut self.output
            && engine.send(command).is_err()
        {
            self.message = Some("Audio engine is not responding.".into());
        }
    }

    fn poll_device_errors(&mut self) {
        if let Some((output, _)) = &mut self.output
            && let Some(kind) = output.take_error()
        {
            self.message = Some(format!(
                "Audio device problem: {kind:?}. Choose the device again in Settings."
            ));
        }
    }

    fn transport_bar(&mut self, ui: &mut egui::Ui) {
        let playing = self.output.as_ref().is_some_and(|(_, e)| e.is_playing());
        ui.horizontal(|ui| {
            if ui.button("To Start").clicked() {
                self.send(Command::Locate(0));
            }
            if ui.button(if playing { "Stop" } else { "Play" }).clicked() {
                self.send(if playing {
                    Command::Stop
                } else {
                    Command::Play
                });
            }
            ui.separator();
            let position = match &self.output {
                Some((output, engine)) => {
                    let spb = samples_per_beat(output.sample_rate, self.bpm);
                    let (bar, beat) = bar_and_beat(engine.position(), spb, BEATS_PER_BAR);
                    format!("{bar:>4} . {beat}")
                }
                None => "   - . -".into(),
            };
            ui.monospace(position);
            ui.separator();
            ui.label("Tempo");
            let tempo = ui.add(
                egui::DragValue::new(&mut self.bpm)
                    .range(MIN_BPM..=MAX_BPM)
                    .speed(0.5)
                    .fixed_decimals(1),
            );
            if tempo.changed() {
                self.send(Command::SetTempo(self.bpm));
            }
            ui.label(format!("{BEATS_PER_BAR}/4"));
            ui.separator();
            if ui.checkbox(&mut self.metronome, "Metronome").changed() {
                self.send(Command::SetMetronome(self.metronome));
            }
            ui.separator();
            if ui.button("Settings").clicked() {
                self.show_settings = true;
            }
        });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut chosen = None;
        let mut refresh = false;
        let mut reopen = false;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Audio output device");
                let current = self.settings.output_device.as_deref();
                let selected = current
                    .and_then(|id| self.devices.iter().find(|d| d.id == id))
                    .map_or_else(
                        || {
                            current
                                .map_or("System default".to_owned(), |id| format!("Missing: {id}"))
                        },
                        |d| d.name.clone(),
                    );
                egui::ComboBox::from_id_salt("output_device")
                    .selected_text(selected)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(current.is_none(), "System default")
                            .clicked()
                        {
                            chosen = Some(None);
                        }
                        for device in &self.devices {
                            let is_current = current == Some(device.id.as_str());
                            if ui.selectable_label(is_current, &device.name).clicked() {
                                chosen = Some(Some(device.id.clone()));
                            }
                        }
                    });
                if ui.button("Refresh device list").clicked() {
                    refresh = true;
                }
                if ui.button("Reopen device").clicked() {
                    reopen = true;
                }
            });
        self.show_settings = open;
        if refresh {
            self.refresh_devices();
        }
        if reopen {
            self.message = None;
            self.open_device();
        }
        if let Some(id) = chosen {
            self.message = None;
            self.select_device(id);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_device_errors();
        egui::Panel::top("transport").show(ui, |ui| self.transport_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            match &self.output {
                Some((output, _)) => ui.label(format!(
                    "Output: {}  |  {} Hz  |  {} ch",
                    output.device_name, output.sample_rate, output.channels
                )),
                None => ui.label("No audio output. Open Settings to choose a device."),
            };
            if let Some(message) = &self.message {
                ui.colored_label(ui.visuals().warn_fg_color, message);
            }
        });
        let ctx = ui.ctx().clone();
        self.settings_window(&ctx);
        if self.output.as_ref().is_some_and(|(_, e)| e.is_playing()) {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}
