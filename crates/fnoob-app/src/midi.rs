use crate::app::App;
use fnoob_core::capture::{self, Played};
use fnoob_core::edit;
use fnoob_core::model::{Id, Note, TrackKind};
use fnoob_core::settings::MidiInputChoice;
use fnoob_core::time::{snap_floor, ticks_per_bar};
use fnoob_engine::midi_in::{MidiError, connect, list_midi_inputs};
use fnoob_engine::{Command as EngineCommand, TransportState};

const START_TIMEOUT_FRAMES: u32 = 120;
const MAX_PLAYED: usize = 8192;
const CAPTURE_GAP_SECONDS: u64 = 4;

pub struct Take {
    track: Id,
    loop_range: Option<(u64, u64)>,
    held: [Option<(u64, u8)>; 128],
    notes: Vec<Note>,
    started: bool,
    waited: u32,
}

impl Take {
    fn event(&mut self, tick: u64, pitch: u8, velocity: u8) {
        let slot = &mut self.held[usize::from(pitch.min(127))];
        if let Some((start, held_velocity)) = slot.take() {
            self.notes.push(Note {
                pitch,
                velocity: held_velocity.clamp(1, 127),
                start_tick: start,
                length_tick: tick.saturating_sub(start).max(1),
            });
        }
        if velocity > 0 {
            *slot = Some((tick, velocity));
        }
    }
}

impl App {
    pub(crate) fn refresh_midi_inputs(&mut self) {
        match list_midi_inputs() {
            Ok(inputs) => self.midi_inputs = inputs,
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    pub(crate) fn connect_midi(&mut self) {
        self.midi = None;
        let port = match &self.settings.midi_input {
            MidiInputChoice::Off => return,
            MidiInputChoice::FirstAvailable => None,
            MidiInputChoice::Port(id) => Some(id.clone()),
        };
        match connect(port.as_deref()) {
            Ok((connection, events)) => {
                self.send(EngineCommand::SetMidiInput(events));
                self.midi = Some(connection);
            }
            Err(MidiError::NoPort) if port.is_none() => {}
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    pub(crate) fn live_target(&self) -> Option<Id> {
        let project = &self.project;
        let instrument = |id: Id| {
            project
                .track(id)
                .filter(|t| t.kind == TrackKind::Instrument)
                .map(|t| t.id)
        };
        project
            .tracks
            .iter()
            .find(|t| t.record_enabled && t.kind == TrackKind::Instrument)
            .map(|t| t.id)
            .or_else(|| self.selection.track.and_then(instrument))
            .or_else(|| {
                self.editor
                    .region
                    .and_then(|r| project.find_region(r))
                    .map(|(t, _)| t.id)
            })
    }

    pub(crate) fn sync_live_track(&mut self) {
        let target = self.live_target();
        if self.live_sent != Some(target) {
            self.send(EngineCommand::SetLiveTrack(target));
            self.live_sent = Some(target);
        }
    }

    pub(crate) fn start_recording(&mut self) {
        if self.audio.is_none() {
            return;
        }
        let records_audio = match self.prepare_audio_take() {
            Ok(audio) => audio,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        let range = self.project.loop_range;
        let loop_range = range.enabled.then_some((range.start_tick, range.end_tick));
        if let Some((start, end)) = loop_range {
            let tick = self.playhead_tick();
            if tick < start || tick >= end {
                self.locate(start);
            }
        }
        match self.live_target() {
            Some(track) => {
                let name = self.project.track(track).map(|t| t.name.clone());
                self.take = Some(Take {
                    track,
                    loop_range,
                    held: [None; 128],
                    notes: Vec::new(),
                    started: false,
                    waited: 0,
                });
                self.message = Some(format!(
                    "Recording{} MIDI on {}.",
                    if records_audio { " audio and" } else { "" },
                    name.unwrap_or_default()
                ));
            }
            None if records_audio => self.message = Some("Recording audio.".into()),
            None => {
                self.message = Some("No track is armed for recording. Transport runs only.".into())
            }
        }
        self.send(EngineCommand::Record);
    }

    pub(crate) fn poll_played(&mut self) {
        let Some((_, engine)) = &mut self.audio else {
            return;
        };
        while let Some(event) = engine.pop_played() {
            self.played.push_back(event);
        }
        while self.played.len() > MAX_PLAYED {
            self.played.pop_front();
        }
    }

    pub(crate) fn capture_midi(&mut self) {
        let Some(track) = self.live_target() else {
            self.message = Some("Select or arm an instrument track to capture notes.".into());
            return;
        };
        let events: Vec<Played> = self.played.iter().copied().collect();
        let gap = u64::from(self.sample_rate()) * CAPTURE_GAP_SECONDS;
        let phrase = capture::last_phrase(&events, gap);
        let bar = ticks_per_bar(self.project.time_signature);
        let start = snap_floor(self.playhead_tick(), bar);
        let notes = capture::notes(phrase, self.timeline(), start);
        if notes.is_empty() {
            self.message = Some("Play some notes first, then capture them.".into());
            return;
        }
        let mut created = None;
        self.edit(|p| created = edit::add_recorded_take(p, track, &notes, None, bar));
        if let Some(id) = created {
            self.played.clear();
            self.selection.track = Some(track);
            self.selection.regions = vec![id];
            self.message = Some(format!("Captured {} notes.", notes.len()));
        }
    }

    pub(crate) fn poll_take(&mut self) {
        if self.take.is_none() {
            return;
        }
        let timeline = self.timeline();
        let (Some(take), Some((_, engine))) = (&mut self.take, &mut self.audio) else {
            self.take = None;
            return;
        };
        let recording = engine.is_recording();
        while let Some(event) = engine.pop_recorded() {
            take.event(
                timeline.sample_to_tick(event.sample),
                event.pitch,
                event.velocity,
            );
        }
        if recording {
            take.started = true;
            return;
        }
        take.waited += 1;
        if take.started
            || take.waited > START_TIMEOUT_FRAMES && engine.state() == TransportState::Stopped
        {
            self.finish_take();
        }
    }

    fn finish_take(&mut self) {
        let Some(mut take) = self.take.take() else {
            return;
        };
        let end = self.playhead_tick();
        for pitch in 0..128u8 {
            take.event(end, pitch, 0);
        }
        if take.notes.is_empty() {
            self.message = Some("No MIDI notes were recorded.".into());
            return;
        }
        let bar = ticks_per_bar(self.project.time_signature);
        let mut created = None;
        self.edit(|p| {
            created = edit::add_recorded_take(p, take.track, &take.notes, take.loop_range, bar);
        });
        if let Some(id) = created {
            self.selection.track = Some(take.track);
            self.selection.regions = vec![id];
            self.message = Some(format!("Recorded {} notes.", take.notes.len()));
        }
    }
}
