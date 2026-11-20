use crate::metronome::Metronome;
use crate::plan::RenderPlan;
use fnoob_core::model::Id;
use fnoob_core::time::{beat_start_sample, first_beat_at_or_after};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

const COMMAND_CAPACITY: usize = 256;
const GARBAGE_CAPACITY: usize = 64;
const RECORDED_CAPACITY: usize = 4096;
pub const MIDI_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiEvent {
    pub pitch: u8,
    pub velocity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedEvent {
    pub sample: u64,
    pub pitch: u8,
    pub velocity: u8,
}

pub enum Command {
    Play,
    Stop,
    Record,
    Locate(u64),
    SetPlan(Box<RenderPlan>),
    SetMidiInput(Consumer<MidiEvent>),
    SetLiveTrack(Option<Id>),
    Preview { track: Id, pitch: u8, velocity: u8 },
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Play => write!(f, "Play"),
            Self::Stop => write!(f, "Stop"),
            Self::Record => write!(f, "Record"),
            Self::Locate(p) => write!(f, "Locate({p})"),
            Self::SetPlan(_) => write!(f, "SetPlan"),
            Self::SetMidiInput(_) => write!(f, "SetMidiInput"),
            Self::SetLiveTrack(t) => write!(f, "SetLiveTrack({t:?})"),
            Self::Preview {
                track,
                pitch,
                velocity,
            } => write!(f, "Preview({track}, {pitch}, {velocity})"),
        }
    }
}

pub enum Garbage {
    Plan(Box<RenderPlan>),
    Midi(Consumer<MidiEvent>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    CountIn,
    Playing,
}

impl TransportState {
    fn to_u8(self) -> u8 {
        match self {
            Self::Stopped => 0,
            Self::CountIn => 1,
            Self::Playing => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::CountIn,
            2 => Self::Playing,
            _ => Self::Stopped,
        }
    }
}

#[derive(Default)]
struct Shared {
    position: AtomicU64,
    state: AtomicU8,
    recording: AtomicBool,
}

pub struct EngineHandle {
    commands: Producer<Command>,
    garbage: Consumer<Garbage>,
    recorded: Consumer<RecordedEvent>,
    shared: Arc<Shared>,
}

impl EngineHandle {
    pub fn send(&mut self, command: Command) -> Result<(), Command> {
        self.commands.push(command).map_err(|PushError::Full(c)| c)
    }

    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }

    pub fn pop_recorded(&mut self) -> Option<RecordedEvent> {
        self.recorded.pop().ok()
    }

    pub fn position(&self) -> u64 {
        self.shared.position.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> TransportState {
        TransportState::from_u8(self.shared.state.load(Ordering::Relaxed))
    }

    pub fn is_running(&self) -> bool {
        self.state() != TransportState::Stopped
    }

    pub fn is_recording(&self) -> bool {
        self.shared.recording.load(Ordering::Acquire)
    }
}

pub fn midi_channel() -> (Producer<MidiEvent>, Consumer<MidiEvent>) {
    RingBuffer::new(MIDI_CAPACITY)
}

struct CountIn {
    elapsed: u64,
    length: u64,
    next_beat: u64,
    next_beat_sample: u64,
}

struct Recorder {
    out: Producer<RecordedEvent>,
    held: [bool; 128],
}

impl Recorder {
    fn push(&mut self, sample: u64, pitch: u8, velocity: u8) {
        let _ = self.out.push(RecordedEvent {
            sample,
            pitch,
            velocity,
        });
    }

    fn event(&mut self, sample: u64, event: MidiEvent) {
        let index = usize::from(event.pitch.min(127));
        if event.velocity > 0 {
            if self.held[index] {
                self.push(sample, event.pitch, 0);
            }
            self.held[index] = true;
            self.push(sample, event.pitch, event.velocity);
        } else if self.held[index] {
            self.held[index] = false;
            self.push(sample, event.pitch, 0);
        }
    }

    fn end_held(&mut self, sample: u64) {
        for pitch in 0..128u8 {
            let index = usize::from(pitch);
            if self.held[index] {
                self.held[index] = false;
                self.push(sample, pitch, 0);
            }
        }
    }
}

pub struct Processor {
    commands: Consumer<Command>,
    garbage: Producer<Garbage>,
    midi: Option<Consumer<MidiEvent>>,
    recorder: Recorder,
    shared: Arc<Shared>,
    channels: usize,
    plan: Box<RenderPlan>,
    metronome: Metronome,
    state: TransportState,
    recording: bool,
    live_track: Option<Id>,
    count_in: CountIn,
    position: u64,
    next_beat: u64,
    next_beat_sample: u64,
}

pub fn create(sample_rate: u32, channels: u16, plan: Box<RenderPlan>) -> (Processor, EngineHandle) {
    let (command_tx, command_rx) = RingBuffer::new(COMMAND_CAPACITY);
    let (garbage_tx, garbage_rx) = RingBuffer::new(GARBAGE_CAPACITY);
    let (recorded_tx, recorded_rx) = RingBuffer::new(RECORDED_CAPACITY);
    let shared = Arc::new(Shared::default());
    let mut processor = Processor {
        commands: command_rx,
        garbage: garbage_tx,
        midi: None,
        recorder: Recorder {
            out: recorded_tx,
            held: [false; 128],
        },
        shared: Arc::clone(&shared),
        channels: usize::from(channels),
        plan,
        metronome: Metronome::new(sample_rate),
        state: TransportState::Stopped,
        recording: false,
        live_track: None,
        count_in: CountIn {
            elapsed: 0,
            length: 0,
            next_beat: 0,
            next_beat_sample: 0,
        },
        position: 0,
        next_beat: 0,
        next_beat_sample: 0,
    };
    processor.resync();
    let handle = EngineHandle {
        commands: command_tx,
        garbage: garbage_rx,
        recorded: recorded_rx,
        shared,
    };
    (processor, handle)
}

impl Processor {
    fn resync(&mut self) {
        let spb = self.plan.samples_per_beat;
        self.next_beat = first_beat_at_or_after(self.position, spb);
        self.next_beat_sample = beat_start_sample(self.next_beat, spb);
        let position = self.position;
        for track in &mut self.plan.tracks {
            track.cursor = track.notes.partition_point(|n| n.start < position);
        }
    }

    fn release_scheduled(&mut self) {
        for track in &mut self.plan.tracks {
            track.synth.release_scheduled();
        }
    }

    fn start(&mut self, record: bool) {
        if self.state != TransportState::Stopped {
            return;
        }
        self.recording = record;
        let beats = if record { self.plan.count_in_beats } else { 0 };
        if beats == 0 {
            self.state = TransportState::Playing;
            return;
        }
        self.count_in = CountIn {
            elapsed: 0,
            length: beat_start_sample(beats, self.plan.samples_per_beat),
            next_beat: 0,
            next_beat_sample: 0,
        };
        self.state = TransportState::CountIn;
    }

    fn stop(&mut self) {
        if self.recording {
            self.recorder.end_held(self.position);
        }
        self.state = TransportState::Stopped;
        self.recording = false;
        self.metronome.stop();
        self.release_scheduled();
    }

    fn set_plan(&mut self, mut plan: Box<RenderPlan>) {
        if plan.samples_per_tick != self.plan.samples_per_tick {
            let tick = self.position as f64 / self.plan.samples_per_tick;
            self.position = (tick * plan.samples_per_tick).round() as u64;
        }
        for track in &mut plan.tracks {
            if track.audible
                && let Some(old) = self.plan.tracks.iter().find(|t| t.id == track.id)
            {
                track.synth.take_voices(&old.synth);
            }
        }
        let old = std::mem::replace(&mut self.plan, plan);
        let _ = self.garbage.push(Garbage::Plan(old));
        self.resync();
    }

    fn set_live_track(&mut self, track: Option<Id>) {
        if track == self.live_track {
            return;
        }
        for plan in &mut self.plan.tracks {
            plan.synth.release_live();
        }
        self.live_track = track;
    }

    fn play_live(&mut self, track: Id, event: MidiEvent) {
        if let Some(plan) = self.plan.track_mut(track) {
            if event.velocity > 0 {
                plan.synth
                    .note_on(event.pitch, event.velocity, u64::MAX, true);
            } else {
                plan.synth.note_off_live(event.pitch);
            }
        }
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Play => self.start(false),
            Command::Record => self.start(true),
            Command::Stop => self.stop(),
            Command::Locate(position) => {
                if self.recording {
                    self.recorder.end_held(self.position);
                }
                self.position = position;
                self.metronome.stop();
                self.release_scheduled();
                self.resync();
            }
            Command::SetPlan(plan) => self.set_plan(plan),
            Command::SetMidiInput(consumer) => {
                if let Some(old) = self.midi.replace(consumer) {
                    let _ = self.garbage.push(Garbage::Midi(old));
                }
            }
            Command::SetLiveTrack(track) => self.set_live_track(track),
            Command::Preview {
                track,
                pitch,
                velocity,
            } => self.play_live(track, MidiEvent { pitch, velocity }),
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.peek() {
            let returns_garbage = matches!(command, Command::SetPlan(_) | Command::SetMidiInput(_));
            if returns_garbage && self.garbage.is_full() {
                break;
            }
            match self.commands.pop() {
                Ok(command) => self.apply(command),
                Err(_) => break,
            }
        }
    }

    fn drain_midi(&mut self) {
        let recording = self.recording && self.state == TransportState::Playing;
        while let Some(event) = self.midi.as_mut().and_then(|m| m.pop().ok()) {
            if let Some(track) = self.live_track {
                self.play_live(track, event);
            }
            if recording {
                self.recorder.event(self.position, event);
            }
        }
    }

    fn tick_count_in(&mut self) {
        let c = &mut self.count_in;
        if c.elapsed == c.next_beat_sample {
            self.metronome
                .trigger(c.next_beat.is_multiple_of(self.plan.beats_per_bar));
            c.next_beat += 1;
            c.next_beat_sample = beat_start_sample(c.next_beat, self.plan.samples_per_beat);
        }
        c.elapsed += 1;
        if c.elapsed >= c.length {
            self.state = TransportState::Playing;
        }
    }

    fn tick_playing(&mut self) {
        if let Some((start, end)) = self.plan.loop_range
            && self.position == end
        {
            if self.recording {
                self.recorder.end_held(end);
            }
            self.position = start;
            self.release_scheduled();
            self.resync();
        }
        if self.position == self.next_beat_sample {
            if self.plan.metronome {
                self.metronome
                    .trigger(self.next_beat.is_multiple_of(self.plan.beats_per_bar));
            }
            self.next_beat += 1;
            self.next_beat_sample = beat_start_sample(self.next_beat, self.plan.samples_per_beat);
        }
        let position = self.position;
        for track in &mut self.plan.tracks {
            while let Some(note) = track.notes.get(track.cursor)
                && note.start <= position
            {
                if note.end > position {
                    track
                        .synth
                        .note_on(note.pitch, note.velocity, note.end, false);
                }
                track.cursor += 1;
            }
        }
    }

    pub fn process(&mut self, out: &mut [f32]) {
        self.drain_commands();
        self.drain_midi();
        if self.channels == 0 {
            out.fill(0.0);
            return;
        }
        for frame in out.chunks_exact_mut(self.channels) {
            let playing = match self.state {
                TransportState::Stopped => false,
                TransportState::CountIn => {
                    self.tick_count_in();
                    false
                }
                TransportState::Playing => {
                    self.tick_playing();
                    true
                }
            };
            let position = self.position;
            let mut sample = self.metronome.next_sample();
            for track in &mut self.plan.tracks {
                if track.audible {
                    sample += track.synth.next_sample(position);
                }
            }
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            frame.fill(sample);
            if playing {
                self.position += 1;
            }
        }
        self.shared.position.store(self.position, Ordering::Relaxed);
        self.shared
            .state
            .store(self.state.to_u8(), Ordering::Relaxed);
        self.shared
            .recording
            .store(self.recording, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnoob_core::edit::{add_midi_region, add_note, add_track};
    use fnoob_core::model::{Instrument, Note, Project, SynthParams, TrackKind, Waveform};

    const SR: u32 = 48_000;
    const BEAT: usize = 24_000;

    fn plan(project: &Project) -> Box<RenderPlan> {
        Box::new(RenderPlan::from_project(project, SR))
    }

    fn render(processor: &mut Processor, frames: usize) -> Vec<f32> {
        let mut out = vec![1.0; frames];
        processor.process(&mut out);
        out
    }

    fn onsets(mono: &[f32]) -> Vec<usize> {
        let mut result = Vec::new();
        let mut silent_run = usize::MAX;
        for (i, s) in mono.iter().enumerate() {
            if *s == 0.0 {
                silent_run = silent_run.saturating_add(1);
            } else {
                if silent_run > 16 {
                    result.push(i - 1);
                }
                silent_run = 0;
            }
        }
        result
    }

    fn metronome_only() -> Project {
        Project::new("t", SR)
    }

    fn short_synth() -> Instrument {
        Instrument::BasicSynth(SynthParams {
            waveform: Waveform::Square,
            attack_ms: 0.0,
            decay_ms: 0.0,
            sustain: 1.0,
            release_ms: 0.0,
            level_db: -12.0,
        })
    }

    fn synth_project() -> (Project, Id, Id) {
        let mut project = metronome_only();
        project.metronome.enabled = false;
        let track = add_track(&mut project, TrackKind::Instrument);
        project.tracks[0].instrument = Some(short_synth());
        let region = add_midi_region(&mut project, track, 0, 4 * 3840).unwrap();
        (project, track, region)
    }

    fn note(start_tick: u64, length_tick: u64) -> Note {
        Note {
            pitch: 60,
            velocity: 100,
            start_tick,
            length_tick,
        }
    }

    fn recorded(h: &mut EngineHandle) -> Vec<RecordedEvent> {
        std::iter::from_fn(|| h.pop_recorded()).collect()
    }

    #[test]
    fn stopped_engine_outputs_silence() {
        let (mut p, _h) = create(SR, 2, plan(&metronome_only()));
        let mut out = vec![1.0; 1024];
        p.process(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn clicks_land_on_beats() {
        let (mut p, mut h) = create(SR, 1, plan(&metronome_only()));
        h.send(Command::Play).unwrap();
        let out = render(&mut p, BEAT * 4 + 100);
        assert_eq!(onsets(&out), [0, BEAT, 2 * BEAT, 3 * BEAT, 4 * BEAT]);
        assert_eq!(h.position(), (BEAT * 4 + 100) as u64);
        assert_eq!(h.state(), TransportState::Playing);
    }

    #[test]
    fn accent_on_first_beat_of_bar() {
        let (mut p, mut h) = create(SR, 1, plan(&metronome_only()));
        h.send(Command::Play).unwrap();
        let out = render(&mut p, BEAT * 5);
        assert_ne!(out[1..200], out[BEAT + 1..BEAT + 200]);
        assert_eq!(out[1..200], out[4 * BEAT + 1..4 * BEAT + 200]);
    }

    #[test]
    fn metronome_off_but_transport_moves() {
        let mut project = metronome_only();
        project.metronome.enabled = false;
        let (mut p, mut h) = create(SR, 2, plan(&project));
        h.send(Command::Play).unwrap();
        let mut out = vec![1.0; 100_000];
        p.process(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(h.position(), 50_000);
    }

    #[test]
    fn notes_start_and_stop_on_exact_samples() {
        let (mut project, _, region) = synth_project();
        add_note(&mut project, region, note(960, 480));
        add_note(&mut project, region, note(3840, 240));
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        let out = render(&mut p, BEAT * 5);
        assert_eq!(onsets(&out), [BEAT, 4 * BEAT]);
        assert_ne!(out[BEAT + 1], 0.0);
        assert_eq!(out[BEAT - 1], 0.0);
        assert!(out[BEAT + BEAT / 2 + 2..4 * BEAT].iter().all(|&s| s == 0.0));
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn loop_wraps_and_replays_notes() {
        let (mut project, _, region) = synth_project();
        project.loop_range.enabled = true;
        project.loop_range.start_tick = 960;
        project.loop_range.end_tick = 1920;
        add_note(&mut project, region, note(960, 480));
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Locate(BEAT as u64)).unwrap();
        h.send(Command::Play).unwrap();
        let out = render(&mut p, BEAT * 3);
        assert_eq!(onsets(&out), [0, BEAT, 2 * BEAT]);
        assert_eq!(h.position(), 2 * BEAT as u64);
    }

    #[test]
    fn note_held_over_loop_end_is_cut() {
        let (mut project, _, region) = synth_project();
        project.loop_range.enabled = true;
        project.loop_range.start_tick = 0;
        project.loop_range.end_tick = 960;
        add_note(&mut project, region, note(480, 3000));
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        let out = render(&mut p, BEAT + BEAT / 4);
        assert!(out[BEAT + 2..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn record_with_count_in_waits_then_plays() {
        let mut project = metronome_only();
        project.metronome.enabled = false;
        project.metronome.count_in_enabled = true;
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Record).unwrap();
        let out = render(&mut p, BEAT * 4 - 1);
        assert_eq!(onsets(&out), [0, BEAT, 2 * BEAT, 3 * BEAT]);
        assert_eq!(h.position(), 0);
        assert_eq!(h.state(), TransportState::CountIn);
        assert!(h.is_recording());
        render(&mut p, 11);
        assert_eq!(h.state(), TransportState::Playing);
        assert_eq!(h.position(), 10);
        h.send(Command::Stop).unwrap();
        render(&mut p, 1);
        assert!(!h.is_recording());
    }

    #[test]
    fn play_skips_count_in() {
        let mut project = metronome_only();
        project.metronome.count_in_enabled = true;
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        render(&mut p, 100);
        assert_eq!(h.position(), 100);
    }

    #[test]
    fn live_midi_plays_on_target_track_only() {
        let (project, track, _) = synth_project();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        let (mut tx, rx) = midi_channel();
        h.send(Command::SetMidiInput(rx)).unwrap();
        tx.push(MidiEvent {
            pitch: 60,
            velocity: 100,
        })
        .unwrap();
        assert!(render(&mut p, 500).iter().all(|&s| s == 0.0));
        h.send(Command::SetLiveTrack(Some(track))).unwrap();
        tx.push(MidiEvent {
            pitch: 60,
            velocity: 100,
        })
        .unwrap();
        assert!(render(&mut p, 500).iter().any(|&s| s != 0.0));
        tx.push(MidiEvent {
            pitch: 60,
            velocity: 0,
        })
        .unwrap();
        render(&mut p, 10);
        assert!(render(&mut p, 500).iter().all(|&s| s == 0.0));
        assert_eq!(h.position(), 0);
    }

    #[test]
    fn recording_captures_midi_with_positions() {
        let (project, track, _) = synth_project();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        let (mut tx, rx) = midi_channel();
        h.send(Command::SetMidiInput(rx)).unwrap();
        h.send(Command::SetLiveTrack(Some(track))).unwrap();
        tx.push(MidiEvent {
            pitch: 50,
            velocity: 90,
        })
        .unwrap();
        render(&mut p, 1000);
        h.send(Command::Record).unwrap();
        render(&mut p, 1000);
        tx.push(MidiEvent {
            pitch: 60,
            velocity: 80,
        })
        .unwrap();
        render(&mut p, 1000);
        tx.push(MidiEvent {
            pitch: 60,
            velocity: 0,
        })
        .unwrap();
        tx.push(MidiEvent {
            pitch: 62,
            velocity: 70,
        })
        .unwrap();
        render(&mut p, 500);
        h.send(Command::Stop).unwrap();
        render(&mut p, 1);
        assert!(!h.is_recording());
        let ev = |sample, pitch, velocity| RecordedEvent {
            sample,
            pitch,
            velocity,
        };
        assert_eq!(
            recorded(&mut h),
            [
                ev(1000, 60, 80),
                ev(2000, 60, 0),
                ev(2000, 62, 70),
                ev(2500, 62, 0)
            ]
        );
    }

    #[test]
    fn recording_ends_held_notes_at_loop_end() {
        let (mut project, track, _) = synth_project();
        project.loop_range.enabled = true;
        project.loop_range.start_tick = 0;
        project.loop_range.end_tick = 960;
        let (mut p, mut h) = create(SR, 1, plan(&project));
        let (mut tx, rx) = midi_channel();
        h.send(Command::SetMidiInput(rx)).unwrap();
        h.send(Command::SetLiveTrack(Some(track))).unwrap();
        h.send(Command::Record).unwrap();
        render(&mut p, 100);
        tx.push(MidiEvent {
            pitch: 64,
            velocity: 100,
        })
        .unwrap();
        render(&mut p, BEAT + 100);
        tx.push(MidiEvent {
            pitch: 64,
            velocity: 0,
        })
        .unwrap();
        render(&mut p, 10);
        let events = recorded(&mut h);
        assert_eq!(events.len(), 2);
        assert_eq!((events[0].sample, events[0].velocity), (100, 100));
        assert_eq!((events[1].sample, events[1].velocity), (BEAT as u64, 0));
    }

    #[test]
    fn edits_during_playback_keep_sounding_voices() {
        let (mut project, _, region) = synth_project();
        add_note(&mut project, region, note(0, 3840));
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        render(&mut p, 1000);
        project.tracks[0].name = "renamed".into();
        h.send(Command::SetPlan(plan(&project))).unwrap();
        let out = render(&mut p, 1000);
        assert!(out[900..].iter().any(|&s| s != 0.0));
    }

    #[test]
    fn preview_plays_without_transport() {
        let (project, track, _) = synth_project();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Preview {
            track,
            pitch: 72,
            velocity: 100,
        })
        .unwrap();
        assert!(render(&mut p, 200).iter().any(|&s| s != 0.0));
        h.send(Command::Preview {
            track,
            pitch: 72,
            velocity: 0,
        })
        .unwrap();
        render(&mut p, 10);
        assert!(render(&mut p, 200).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn tempo_change_keeps_musical_position() {
        let mut project = metronome_only();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Locate(48_000)).unwrap();
        project.tempo_bpm = 60.0;
        h.send(Command::SetPlan(plan(&project))).unwrap();
        render(&mut p, 1);
        assert_eq!(h.position(), 96_000);
    }

    #[test]
    fn old_plans_come_back_for_dropping() {
        let project = metronome_only();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        for _ in 0..3 {
            h.send(Command::SetPlan(plan(&project))).unwrap();
        }
        render(&mut p, 1);
        assert_eq!(h.garbage.slots(), 3);
        h.collect_garbage();
        assert_eq!(h.garbage.slots(), 0);
    }

    #[test]
    fn full_garbage_defers_plan_swap() {
        let mut project = metronome_only();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        for _ in 0..GARBAGE_CAPACITY {
            h.send(Command::SetPlan(plan(&project))).unwrap();
        }
        render(&mut p, 1);
        project.tempo_bpm = 60.0;
        h.send(Command::SetPlan(plan(&project))).unwrap();
        h.send(Command::Play).unwrap();
        render(&mut p, 1);
        assert_eq!(h.state(), TransportState::Stopped);
        h.collect_garbage();
        render(&mut p, 1);
        assert_eq!(h.state(), TransportState::Playing);
        assert_eq!(p.plan.samples_per_tick, 48_000.0 * 60.0 / (60.0 * 960.0));
    }

    #[test]
    fn multichannel_frames_are_identical() {
        let (mut p, mut h) = create(SR, 2, plan(&metronome_only()));
        h.send(Command::Play).unwrap();
        let mut out = vec![0.0; 4000];
        p.process(&mut out);
        assert!(out.as_chunks::<2>().0.iter().all(|f| f[0] == f[1]));
        assert!(out.iter().any(|&s| s != 0.0));
    }
}
