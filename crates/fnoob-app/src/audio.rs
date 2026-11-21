use crate::app::App;
use fnoob_core::edit;
use fnoob_core::io::AUDIO_DIR;
use fnoob_core::model::{Id, TrackKind};
use fnoob_core::settings::InputChoice;
use fnoob_engine::audio_data::load_wav;
use fnoob_engine::device::{DeviceError, list_input_devices, open_input};
use fnoob_engine::record::{RecordTarget, WavRecorder, start_recording};
use fnoob_engine::{Command as EngineCommand, TransportState};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

const START_TIMEOUT_FRAMES: u32 = 120;

pub struct AudioTake {
    recorder: WavRecorder,
    tracks: Vec<(Id, String)>,
    started: bool,
    waited: u32,
}

pub fn file_stem_for(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Audio".to_owned()
    } else {
        cleaned.to_owned()
    }
}

fn unique_audio_path(dir: &Path, stem: &str, taken: &HashSet<String>) -> String {
    (1..)
        .map(|n| format!("{AUDIO_DIR}/{stem}-{n:02}.wav"))
        .find(|rel| !taken.contains(rel) && !dir.join(rel).exists())
        .unwrap_or_default()
}

impl App {
    pub(crate) fn refresh_input_devices(&mut self) {
        match list_input_devices() {
            Ok(devices) => self.input_devices = devices,
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    pub(crate) fn open_input_device(&mut self) {
        self.input = None;
        let Some((output, _)) = &self.audio else {
            return;
        };
        let rate = output.sample_rate;
        let id = match &self.settings.input_device {
            InputChoice::Off => return,
            InputChoice::SystemDefault => None,
            InputChoice::Device(id) => Some(id.clone()),
        };
        match open_input(id.as_deref(), self.settings.buffer_frames, rate) {
            Ok((input, samples)) => {
                let channels = input.channels;
                self.send(EngineCommand::SetAudioInput { samples, channels });
                self.input = Some(input);
            }
            Err(DeviceError::NoDevice) if id.is_none() => {}
            Err(e) => self.message = Some(format!("Audio input: {e}")),
        }
    }

    pub(crate) fn input_channels(&self) -> u16 {
        self.input.as_ref().map_or(2, |i| i.channels)
    }

    pub(crate) fn load_audio_files(&mut self) {
        let mut problems = Vec::new();
        for file in &self.project.audio_files {
            if self.pool.contains_key(&file.id) || self.missing.contains(&file.id) {
                continue;
            }
            let Some(dir) = &self.project_dir else {
                self.missing.insert(file.id);
                continue;
            };
            match load_wav(&dir.join(&file.path), self.project.sample_rate) {
                Ok(data) => {
                    self.pool.insert(file.id, Arc::new(data));
                }
                Err(e) => {
                    self.missing.insert(file.id);
                    problems.push(format!("{}: {e}", file.path));
                }
            }
        }
        if !problems.is_empty() {
            self.message = Some(format!(
                "Some audio files cannot be played. {}",
                problems.join("; ")
            ));
        }
    }

    pub(crate) fn armed_audio_tracks(&self) -> Vec<Id> {
        self.project
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio && t.record_enabled)
            .map(|t| t.id)
            .collect()
    }

    pub(crate) fn prepare_audio_take(&mut self) -> Result<bool, String> {
        let armed = self.armed_audio_tracks();
        if armed.is_empty() {
            return Ok(false);
        }
        let Some(dir) = self.project_dir.clone() else {
            return Err("Save the project before recording audio.".into());
        };
        let Some(input) = &self.input else {
            return Err("No audio input is open. Choose one in Audio and MIDI Settings.".into());
        };
        let (channels, rate) = (input.channels, input.sample_rate);
        let mut taken = HashSet::new();
        let mut targets = Vec::new();
        let mut tracks = Vec::new();
        for id in armed {
            let Some(track) = self.project.track(id) else {
                continue;
            };
            let input = track.audio_input.unwrap_or_default();
            let rel = unique_audio_path(&dir, &file_stem_for(&track.name), &taken);
            taken.insert(rel.clone());
            targets.push(RecordTarget {
                path: dir.join(&rel),
                first_channel: input.first_channel,
                channels: input.channels,
            });
            tracks.push((id, rel));
        }
        let (recorder, sink) = start_recording(channels, rate, targets)?;
        self.send(EngineCommand::SetRecordSink(sink));
        self.audio_take = Some(AudioTake {
            recorder,
            tracks,
            started: false,
            waited: 0,
        });
        Ok(true)
    }

    pub(crate) fn poll_audio_take(&mut self) {
        let Some(take) = &mut self.audio_take else {
            return;
        };
        let Some((_, engine)) = &self.audio else {
            self.finish_audio_take();
            return;
        };
        if engine.is_recording() {
            take.started = true;
            return;
        }
        take.waited += 1;
        if take.started
            || take.waited > START_TIMEOUT_FRAMES && engine.state() == TransportState::Stopped
        {
            self.finish_audio_take();
        }
    }

    fn finish_audio_take(&mut self) {
        let Some(take) = self.audio_take.take() else {
            return;
        };
        let start = self.audio.as_ref().and_then(|(_, e)| e.audio_take_start());
        let overruns = self.audio.as_ref().map_or(0, |(_, e)| e.record_overruns());
        let latency = self.audio.as_ref().map_or(0, |(o, _)| o.latency_frames())
            + self.input.as_ref().map_or(0, |i| i.latency_frames());
        let files = match take.recorder.finish() {
            Ok(files) => files,
            Err(e) => {
                self.message = Some(format!("Recording failed: {e}"));
                return;
            }
        };
        let frames = files.first().map_or(0, |f| f.frames);
        let Some(start) = start.filter(|_| frames > 0) else {
            for file in &files {
                let _ = std::fs::remove_file(&file.path);
            }
            self.message = Some("No audio was recorded.".into());
            return;
        };
        let (start_sample, mut offset) = if start >= latency {
            (start - latency, 0)
        } else {
            (0, latency - start)
        };
        let timeline = self.timeline();
        let mut tick = timeline.sample_to_tick(start_sample);
        if timeline.tick_to_sample(tick) < start_sample {
            tick += 1;
        }
        offset += timeline.tick_to_sample(tick) - start_sample;
        if offset >= frames {
            self.message = Some("The recording was shorter than the latency.".into());
            return;
        }
        let mut created = Vec::new();
        self.edit(|p| {
            for ((track, rel), file) in take.tracks.iter().zip(&files) {
                let file_id = edit::add_audio_file(p, rel.clone(), file.channels, file.frames);
                if let Some(region) = edit::add_audio_region(p, *track, tick, file_id, offset) {
                    created.push(region);
                }
            }
        });
        self.load_audio_files();
        self.sync_plan();
        self.selection.regions = created;
        self.message = Some(if overruns > 0 {
            format!("Recorded audio, but {overruns} frames were lost. Try a larger buffer.")
        } else {
            format!(
                "Recorded {:.1} s of audio (latency shift {latency} frames).",
                frames as f64 / f64::from(self.sample_rate())
            )
        });
    }

    pub(crate) fn import_audio(&mut self) {
        let Some(dir) = self.project_dir.clone() else {
            self.message = Some("Save the project before importing audio.".into());
            return;
        };
        let Some(source) = rfd::FileDialog::new()
            .set_title("Import audio file")
            .add_filter("WAV audio", &["wav"])
            .pick_file()
        else {
            return;
        };
        let data = match load_wav(&source, self.project.sample_rate) {
            Ok(data) => data,
            Err(e) => {
                self.message = Some(format!("Cannot import {}: {e}", source.display()));
                return;
            }
        };
        let stem = source.file_stem().map_or_else(
            || "Audio".to_owned(),
            |s| file_stem_for(&s.to_string_lossy()),
        );
        let rel = unique_audio_path(&dir, &stem, &HashSet::new());
        let target = dir.join(&rel);
        let copied = target
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::copy(&source, &target).map(|_| ()));
        if let Err(e) = copied {
            self.message = Some(format!("Cannot copy the file into the project: {e}"));
            return;
        }
        let selected = self.selection.track.filter(|id| {
            self.project
                .track(*id)
                .is_some_and(|t| t.kind == TrackKind::Audio)
        });
        let tick = self.playhead_tick();
        let (channels, frames) = (data.channels, data.frames);
        let mut result = None;
        self.edit(|p| {
            let track = selected.unwrap_or_else(|| edit::add_track(p, TrackKind::Audio));
            let file_id = edit::add_audio_file(p, rel.clone(), channels, frames);
            result =
                edit::add_audio_region(p, track, tick, file_id, 0).map(|r| (track, r, file_id));
        });
        if let Some((track, region, file_id)) = result {
            self.pool.insert(file_id, Arc::new(data));
            self.sync_plan();
            self.selection.track = Some(track);
            self.selection.regions = vec![region];
            self.message = Some(format!("Imported {rel}."));
        }
    }

    pub(crate) fn copy_audio_files(&self, to: &Path) -> Result<(), String> {
        let Some(from) = &self.project_dir else {
            return Ok(());
        };
        if from == to {
            return Ok(());
        }
        for file in &self.project.audio_files {
            let (source, target) = (from.join(&file.path), to.join(&file.path));
            if target.exists() || !source.exists() {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&source, &target)
                .map_err(|e| format!("cannot copy {}: {e}", file.path))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_cleaned() {
        assert_eq!(file_stem_for("Audio 1"), "Audio 1");
        assert_eq!(file_stem_for("a/b:c*"), "a_b_c_");
        assert_eq!(file_stem_for("  "), "Audio");
        assert_eq!(file_stem_for(".."), "__");
    }

    #[test]
    fn audio_paths_skip_taken_names() {
        let dir = std::env::temp_dir().join(format!("fnoob-app-paths-{}", std::process::id()));
        let mut taken = HashSet::new();
        taken.insert("audio/Vox-01.wav".to_owned());
        assert_eq!(unique_audio_path(&dir, "Vox", &taken), "audio/Vox-02.wav");
    }
}
