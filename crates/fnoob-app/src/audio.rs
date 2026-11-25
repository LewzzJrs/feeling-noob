use crate::app::App;
use fnoob_core::edit;
use fnoob_core::io::AUDIO_DIR;
use fnoob_core::model::{Id, TrackKind};
use fnoob_core::settings::InputChoice;
use fnoob_engine::audio_data::{AudioData, load_wav};
use fnoob_engine::decode;
use fnoob_engine::device::{DeviceError, list_input_devices, open_input};
use fnoob_engine::record::{RecordTarget, WavRecorder, start_recording};
use fnoob_engine::{Command as EngineCommand, TransportState};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
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
        if self.project_dir.is_none() && !self.save_as() {
            return Err(
                "Audio is recorded into the project folder, so save the project first.".into(),
            );
        }
        let Some(dir) = self.project_dir.clone() else {
            return Err("The project has no folder.".into());
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
        let (recorder, sink) =
            start_recording(channels, rate, self.settings.record_format, targets)?;
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
        let timeline = self.timeline();
        let range = self.project.loop_range;
        let loop_samples = range.enabled.then(|| {
            (
                timeline.tick_to_sample(range.start_tick),
                timeline.tick_to_sample(range.end_tick),
            )
        });
        let placed: Vec<(u64, u64, u64)> =
            fnoob_core::takes::take_passes(start, latency, frames, loop_samples)
                .into_iter()
                .filter_map(|pass| {
                    let mut tick = timeline.sample_to_tick(pass.at);
                    if timeline.tick_to_sample(tick) < pass.at {
                        tick += 1;
                    }
                    let shift = timeline.tick_to_sample(tick) - pass.at;
                    (pass.frames > shift).then(|| (tick, pass.offset + shift, pass.frames - shift))
                })
                .collect();
        if placed.is_empty() {
            self.message = Some("The recording was shorter than the latency.".into());
            return;
        }
        let mut created = Vec::new();
        self.edit(|p| {
            for ((track, rel), file) in take.tracks.iter().zip(&files) {
                let file_id = edit::add_audio_file(p, rel.clone(), file.channels, file.frames);
                let mut lanes = Vec::new();
                for &(tick, offset, length) in &placed {
                    let Some(region) = edit::add_audio_region(p, *track, tick, file_id, offset)
                    else {
                        continue;
                    };
                    let ticks = edit::frames_to_ticks(p, length);
                    if let Some(r) = edit::region_mut(p, region) {
                        r.length_tick = r.length_tick.min(ticks);
                    }
                    if placed.len() == 1 {
                        created.push(region);
                    } else if let Some(t) = p.track_mut(*track)
                        && let Some(index) = t.regions.iter().position(|r| r.id == region)
                    {
                        let moved = t.regions.remove(index);
                        if let Some(lane) =
                            fnoob_core::takes::add_take_lane(p, *track, vec![moved.clone()])
                        {
                            lanes.push((lane, moved.start_tick, moved.end_tick()));
                        }
                    }
                }
                if let Some(&(lane, from, to)) = lanes.last() {
                    created.extend(fnoob_core::takes::comp(p, *track, lane, from, to));
                }
            }
        });
        if placed.len() > 1 {
            for (track, _) in &take.tracks {
                self.takes_open.insert(*track);
            }
        }
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
        let Some(sources) = rfd::FileDialog::new()
            .set_title("Import audio files")
            .add_filter("Audio files", &decode::AUDIO_EXTENSIONS)
            .pick_files()
        else {
            return;
        };
        let target = self.selection.track;
        let tick = self.playhead_tick();
        self.import_files(&sources, target, tick);
    }

    fn copy_one(
        &self,
        dir: &Path,
        source: &Path,
        taken: &mut HashSet<String>,
    ) -> Result<(String, AudioData), String> {
        let rate = self.project.sample_rate;
        let convert = decode::needs_conversion(source, rate);
        let data =
            decode::load_any(source, rate).map_err(|e| format!("{}: {e}", source.display()))?;
        let stem = source.file_stem().map_or_else(
            || "Audio".to_owned(),
            |s| file_stem_for(&s.to_string_lossy()),
        );
        let rel = unique_audio_path(dir, &stem, taken);
        taken.insert(rel.clone());
        let destination = dir.join(&rel);
        destination
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(|e| format!("{}: {e}", source.display()))?;
        if convert {
            decode::write_wav(&destination, &data, rate)
                .map_err(|e| format!("{}: {e}", source.display()))?;
        } else {
            std::fs::copy(source, &destination)
                .map_err(|e| format!("{}: {e}", source.display()))?;
        }
        Ok((rel, data))
    }

    pub(crate) fn reversed_audio(
        &mut self,
        region: Id,
    ) -> Result<(Id, String, u16, u64, AudioData), String> {
        let found = self
            .project
            .tracks
            .iter()
            .flat_map(|t| t.regions.iter())
            .find(|r| r.id == region)
            .cloned();
        let Some(found) = found else {
            return Err("The clip is gone.".into());
        };
        let fnoob_core::model::RegionContent::Audio {
            file_id,
            offset_frames,
            ..
        } = found.content
        else {
            return Err("Only audio clips need a reversed file.".into());
        };
        let Some(source) = self.pool.get(&file_id).cloned() else {
            return Err("The audio file of this clip is not loaded.".into());
        };
        if self.project_dir.is_none() && !self.save_as() {
            return Err(
                "Reversed audio is saved in the project folder, so save the project first.".into(),
            );
        }
        let Some(dir) = self.project_dir.clone() else {
            return Err("Save the project first.".into());
        };
        let wanted = edit::ticks_to_frames(&self.project, found.start_tick, found.length_tick);
        let start = offset_frames.min(source.frames);
        let end = (start + wanted).min(source.frames);
        let channels = usize::from(source.channels.max(1));
        let mut samples = Vec::with_capacity((end - start) as usize * channels);
        for frame in (start..end).rev() {
            let at = frame as usize * channels;
            samples.extend_from_slice(&source.samples[at..at + channels]);
        }
        let data = AudioData::from_samples(source.channels, samples);
        let stem = format!("{} Reverse", file_stem_for(&found.name));
        let rel = unique_audio_path(&dir, &stem, &HashSet::new());
        let destination = dir.join(&rel);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        decode::write_wav(&destination, &data, self.project.sample_rate)
            .map_err(|e| format!("Cannot write the reversed audio. {e}"))?;
        let frames = data.frames;
        Ok((region, rel, source.channels, frames, data))
    }

    pub(crate) fn copy_into_project(&mut self, source: &Path) -> Option<(String, AudioData)> {
        if self.project_dir.is_none() && !self.save_as() {
            self.message = Some(
                "Samples are copied into the project folder, so save the project first.".into(),
            );
            return None;
        }
        let dir = self.project_dir.clone()?;
        match self.copy_one(&dir, source, &mut HashSet::new()) {
            Ok(item) => Some(item),
            Err(e) => {
                self.message = Some(format!("Cannot load sample. {e}"));
                None
            }
        }
    }

    pub(crate) fn import_files(&mut self, sources: &[PathBuf], target: Option<Id>, tick: u64) {
        if sources.is_empty() {
            return;
        }
        if self.project_dir.is_none() && !self.save_as() {
            self.message =
                Some("Audio is copied into the project folder, so save the project first.".into());
            return;
        }
        let Some(dir) = self.project_dir.clone() else {
            return;
        };
        let mut problems = Vec::new();
        let mut ready = Vec::new();
        let mut taken = HashSet::new();
        for source in sources {
            match self.copy_one(&dir, source, &mut taken) {
                Ok(item) => ready.push(item),
                Err(e) => problems.push(e),
            }
        }
        let same_track = target.filter(|id| {
            self.project
                .track(*id)
                .is_some_and(|t| t.kind == TrackKind::Audio)
        });
        let mut created = Vec::new();
        let specs: Vec<(String, u16, u64)> = ready
            .iter()
            .map(|(rel, data)| (rel.clone(), data.channels, data.frames))
            .collect();
        self.edit(|p| {
            let mut at = tick;
            for (rel, channels, frames) in &specs {
                let track = same_track.unwrap_or_else(|| edit::add_track(p, TrackKind::Audio));
                let file_id = edit::add_audio_file(p, rel.clone(), *channels, *frames);
                if let Some(region) = edit::add_audio_region(p, track, at, file_id, 0) {
                    if same_track.is_some() {
                        at += edit::frames_to_ticks(p, *frames);
                    }
                    created.push((track, region, file_id));
                }
            }
        });
        for ((_, data), (_, _, file_id)) in ready.into_iter().zip(&created) {
            self.pool.insert(*file_id, Arc::new(data));
        }
        if !created.is_empty() {
            self.sync_plan();
            self.selection.track = created.first().map(|(t, _, _)| *t);
            self.selection.regions = created.iter().map(|(_, r, _)| *r).collect();
        }
        self.message = Some(if problems.is_empty() {
            format!("Imported {} audio files.", created.len())
        } else {
            format!(
                "Imported {} audio files. Skipped: {}",
                created.len(),
                problems.join("; ")
            )
        });
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
