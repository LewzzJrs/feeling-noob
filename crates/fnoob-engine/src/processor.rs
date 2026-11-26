use crate::audio_data::AudioData;
use crate::effects::BuiltinProcessor;
use crate::metronome::Metronome;
use crate::plan::{
    AudioTrackPlan, Inserts, MASTER_METER, MAX_BLOCK, MAX_PLUGINS, METER_SLOTS, NO_OFF, Node,
    RenderPlan, StripPlan, TRACK_METERS, TrackPlan,
};
use crate::session::{Session, SessionStatus};
use fnoob_core::capture::Played;
use fnoob_core::effects::BuiltinEffect;
use fnoob_core::model::{Id, MAX_RETURNS};
use fnoob_core::time::beat_start_sample;
use fnoob_platform::plugin::{PluginProcessor, Transport};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

const COMMAND_CAPACITY: usize = 256;
const GARBAGE_CAPACITY: usize = 64;
const RECORDED_CAPACITY: usize = 4096;
const PLAYED_CAPACITY: usize = 4096;
pub const MIDI_CAPACITY: usize = 1024;
pub const MAX_INPUT_CHANNELS: usize = 32;
const INPUT_BACKLOG_FRAMES: usize = 8192;
const NO_TAKE: u64 = u64::MAX;

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
    Preview {
        track: Id,
        pitch: u8,
        velocity: u8,
    },
    SetAudioInput {
        samples: Consumer<f32>,
        channels: u16,
    },
    SetRecordSink(Producer<f32>),
    InstallPlugin {
        slot: usize,
        plugin: Box<PluginProcessor>,
    },
    RemovePlugin(usize),
    InstallBuiltin {
        slot: usize,
        effect: Box<BuiltinProcessor>,
    },
    SetBuiltinParams {
        slot: usize,
        params: BuiltinEffect,
    },
    LiveNote(MidiEvent),
    PreviewAudio(Option<Arc<AudioData>>),
    LaunchClip {
        track: Id,
        slot: usize,
    },
    ReleaseClip {
        track: Id,
        slot: usize,
    },
    StopClips(Option<Id>),
    LaunchScene(usize),
    BackToArrangement,
    SetOutputs(Outputs),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outputs {
    pub main: usize,
    pub cue: usize,
    pub cue_gain: f32,
}

impl Default for Outputs {
    fn default() -> Self {
        Self {
            main: 0,
            cue: 0,
            cue_gain: 0.7,
        }
    }
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
            Self::SetAudioInput { channels, .. } => write!(f, "SetAudioInput({channels})"),
            Self::SetRecordSink(_) => write!(f, "SetRecordSink"),
            Self::InstallPlugin { slot, .. } => write!(f, "InstallPlugin({slot})"),
            Self::RemovePlugin(slot) => write!(f, "RemovePlugin({slot})"),
            Self::InstallBuiltin { slot, .. } => write!(f, "InstallBuiltin({slot})"),
            Self::SetBuiltinParams { slot, .. } => write!(f, "SetBuiltinParams({slot})"),
            Self::LiveNote(event) => write!(f, "LiveNote({}, {})", event.pitch, event.velocity),
            Self::PreviewAudio(data) => write!(f, "PreviewAudio({})", data.is_some()),
            Self::LaunchClip { track, slot } => write!(f, "LaunchClip({track}, {slot})"),
            Self::ReleaseClip { track, slot } => write!(f, "ReleaseClip({track}, {slot})"),
            Self::StopClips(track) => write!(f, "StopClips({track:?})"),
            Self::LaunchScene(scene) => write!(f, "LaunchScene({scene})"),
            Self::BackToArrangement => write!(f, "BackToArrangement"),
            Self::SetOutputs(o) => write!(f, "SetOutputs({}, {})", o.main, o.cue),
        }
    }
}

pub enum Garbage {
    Plan(Box<RenderPlan>),
    Midi(Consumer<MidiEvent>),
    Input(Consumer<f32>),
    Sink(Producer<f32>),
    Plugin(Box<PluginProcessor>),
    Builtin(Box<BuiltinProcessor>),
    Audio(Arc<AudioData>),
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

struct Shared {
    position: AtomicU64,
    state: AtomicU8,
    recording: AtomicBool,
    take_start: AtomicU64,
    overruns: AtomicU64,
    cpu: AtomicU32,
    meters: Box<[AtomicU32]>,
    session: Box<[AtomicU64]>,
}

pub struct EngineHandle {
    commands: Producer<Command>,
    garbage: Consumer<Garbage>,
    recorded: Consumer<RecordedEvent>,
    played: Consumer<Played>,
    shared: Arc<Shared>,
}

impl EngineHandle {
    pub fn send(&mut self, command: Command) -> Result<(), Command> {
        self.commands.push(command).map_err(|PushError::Full(c)| c)
    }

    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }

    pub fn pop_played(&mut self) -> Option<Played> {
        self.played.pop().ok()
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

    pub fn audio_take_start(&self) -> Option<u64> {
        let start = self.shared.take_start.load(Ordering::Acquire);
        (start != NO_TAKE).then_some(start)
    }

    pub fn take_peak(&self, slot: usize) -> f32 {
        let (left, right) = self.take_peaks(slot);
        left.max(right)
    }

    pub fn take_peaks(&self, slot: usize) -> (f32, f32) {
        let take = |index: usize| {
            self.shared
                .meters
                .get(index)
                .map_or(0.0, |m| f32::from_bits(m.swap(0, Ordering::Relaxed)))
        };
        (take(slot * 2), take(slot * 2 + 1))
    }

    pub fn session_status(&self, track_index: usize) -> SessionStatus {
        self.shared
            .session
            .get(track_index)
            .map_or_else(SessionStatus::default, |cell| {
                SessionStatus::decode(cell.load(Ordering::Relaxed))
            })
    }

    pub fn cpu_load(&self) -> f32 {
        f32::from_bits(self.shared.cpu.load(Ordering::Relaxed))
    }

    pub fn record_overruns(&self) -> u64 {
        self.shared.overruns.load(Ordering::Relaxed)
    }
}

pub fn audio_channel(capacity_samples: usize) -> (Producer<f32>, Consumer<f32>) {
    RingBuffer::new(capacity_samples)
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
    input: Option<Consumer<f32>>,
    plugins: Vec<Option<EngineSlot>>,
    main_left: Vec<f32>,
    main_right: Vec<f32>,
    return_left: Vec<Vec<f32>>,
    return_right: Vec<Vec<f32>>,
    click: Vec<f32>,
    positions: Vec<u64>,
    preview: Option<Arc<AudioData>>,
    preview_at: u64,
    frame: usize,
    peaks: [f32; METER_SLOTS * 2],
    input_channels: usize,
    input_frame: [f32; MAX_INPUT_CHANNELS],
    sink: Option<Producer<f32>>,
    take_started: bool,
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
    next_downbeat: bool,
    next_beat_sample: u64,
    rng: u64,
    played: Producer<Played>,
    clock: u64,
    sample_rate: u32,
    outputs: Outputs,
}

pub fn create(sample_rate: u32, channels: u16, plan: Box<RenderPlan>) -> (Processor, EngineHandle) {
    let (command_tx, command_rx) = RingBuffer::new(COMMAND_CAPACITY);
    let (garbage_tx, garbage_rx) = RingBuffer::new(GARBAGE_CAPACITY);
    let (recorded_tx, recorded_rx) = RingBuffer::new(RECORDED_CAPACITY);
    let (played_tx, played_rx) = RingBuffer::new(PLAYED_CAPACITY);
    let shared = Arc::new(Shared {
        position: AtomicU64::new(0),
        state: AtomicU8::new(0),
        recording: AtomicBool::new(false),
        take_start: AtomicU64::new(NO_TAKE),
        overruns: AtomicU64::new(0),
        cpu: AtomicU32::new(0),
        meters: (0..METER_SLOTS * 2).map(|_| AtomicU32::new(0)).collect(),
        session: (0..TRACK_METERS).map(|_| AtomicU64::new(0)).collect(),
    });
    let mut processor = Processor {
        commands: command_rx,
        garbage: garbage_tx,
        midi: None,
        input: None,
        plugins: (0..MAX_PLUGINS).map(|_| None).collect(),
        main_left: vec![0.0; MAX_BLOCK],
        main_right: vec![0.0; MAX_BLOCK],
        return_left: (0..MAX_RETURNS).map(|_| vec![0.0; MAX_BLOCK]).collect(),
        return_right: (0..MAX_RETURNS).map(|_| vec![0.0; MAX_BLOCK]).collect(),
        click: vec![0.0; MAX_BLOCK],
        positions: vec![0; MAX_BLOCK],
        preview: None,
        preview_at: 0,
        frame: 0,
        peaks: [0.0; METER_SLOTS * 2],
        input_channels: 0,
        input_frame: [0.0; MAX_INPUT_CHANNELS],
        sink: None,
        take_started: false,
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
        next_downbeat: true,
        next_beat_sample: 0,
        rng: 0x9E37_79B9_7F4A_7C15,
        sample_rate,
        outputs: Outputs::default(),
        played: played_tx,
        clock: 0,
    };
    processor.resync();
    let handle = EngineHandle {
        commands: command_tx,
        garbage: garbage_rx,
        recorded: recorded_rx,
        played: played_rx,
        shared,
    };
    (processor, handle)
}

impl Processor {
    fn schedule_beat(&mut self, from_tick: u64, min_sample: u64) {
        let mut tick = from_tick;
        loop {
            let (beat, downbeat) = self.plan.meter.next_beat(tick);
            let sample = self.plan.timeline.tick_to_sample(beat);
            if sample >= min_sample {
                self.next_beat = beat;
                self.next_downbeat = downbeat;
                self.next_beat_sample = sample;
                return;
            }
            tick = beat + 1;
        }
    }

    fn resync(&mut self) {
        let tick = self.plan.timeline.sample_to_tick(self.position);
        self.schedule_beat(tick, self.position);
        let position = self.position;
        for track in &mut self.plan.tracks {
            track.cursor = track.notes.partition_point(|n| n.start < position);
        }
        for track in &mut self.plan.audio_tracks {
            track.cursor = 0;
        }
    }

    fn release_scheduled(&mut self) {
        let frame = self.frame;
        for track in &mut self.plan.tracks {
            release_track(track, &self.plugins, frame);
        }
    }

    fn session_mut(&mut self, id: Id) -> Option<&mut Session> {
        if let Some(track) = self.plan.tracks.iter_mut().find(|t| t.id == id) {
            return Some(&mut track.session);
        }
        self.plan
            .audio_tracks
            .iter_mut()
            .find(|t| t.id == id)
            .map(|t| &mut t.session)
    }

    fn sessions(&mut self) -> impl Iterator<Item = &mut Session> {
        self.plan
            .tracks
            .iter_mut()
            .map(|t| &mut t.session)
            .chain(self.plan.audio_tracks.iter_mut().map(|t| &mut t.session))
    }

    fn session_command(&mut self, command: Command) {
        let stopped = self.state == TransportState::Stopped;
        if stopped
            && !matches!(
                command,
                Command::ReleaseClip { .. } | Command::StopClips(_) | Command::BackToArrangement
            )
        {
            self.start(false);
        }
        let (position, quantum) = (self.position, self.plan.launch_quantum);
        match command {
            Command::LaunchClip { track, slot } => {
                if let Some(session) = self.session_mut(track) {
                    session.launch(slot, position, quantum, stopped, false);
                }
            }
            Command::ReleaseClip { track, slot } => {
                if let Some(session) = self.session_mut(track) {
                    session.release(slot, position, quantum);
                }
            }
            Command::StopClips(Some(track)) => {
                if let Some(session) = self.session_mut(track) {
                    session.stop(position, quantum, stopped);
                }
            }
            Command::StopClips(None) => {
                for session in self.sessions() {
                    session.stop(position, quantum, stopped);
                }
            }
            Command::LaunchScene(scene) => {
                for session in self.sessions() {
                    session.launch(scene, position, quantum, stopped, true);
                }
            }
            Command::BackToArrangement => {
                for session in self.sessions() {
                    session.halt();
                    session.play.overrides = false;
                }
                self.release_scheduled();
            }
            _ => {}
        }
    }

    fn fire_queued_now(&mut self) {
        let position = self.position;
        for session in self.sessions() {
            session.fire_queued_now(position);
        }
    }

    fn send_due_offs(&mut self) {
        let (frame, position) = (self.frame, self.position);
        for track in &mut self.plan.tracks {
            if position < track.next_off {
                continue;
            }
            let plugin = plugin_of(&self.plugins, track.instrument);
            let mut next = NO_OFF;
            for pitch in 0..128u8 {
                let off = &mut track.pending_offs[usize::from(pitch)];
                if *off <= position {
                    if let Some(plugin) = plugin {
                        plugin.note(frame, pitch, 0);
                    }
                    *off = NO_OFF;
                } else {
                    next = next.min(*off);
                }
            }
            track.next_off = next;
        }
    }

    fn start(&mut self, record: bool) {
        if self.state != TransportState::Stopped {
            return;
        }
        self.recording = record;
        if record {
            self.take_started = false;
            self.shared.take_start.store(NO_TAKE, Ordering::Release);
        }
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
        for session in self.sessions() {
            session.halt();
        }
        self.state = TransportState::Stopped;
        self.recording = false;
        self.metronome.stop();
        self.release_scheduled();
    }

    fn set_plan(&mut self, mut plan: Box<RenderPlan>) {
        if plan.timeline != self.plan.timeline {
            let tick = self.plan.timeline.sample_to_tick_f64(self.position as f64);
            self.position = plan.timeline.tick_to_sample_f64(tick).round() as u64;
        }
        for track in &mut plan.tracks {
            if track.audible
                && let Some(old) = self.plan.tracks.iter().find(|t| t.id == track.id)
            {
                track.synth.take_voices(&old.synth);
                if old.instrument == track.instrument {
                    track.pending_offs = old.pending_offs;
                    track.next_off = old.next_off;
                }
            }
            if let Some(old) = self.plan.tracks.iter().find(|t| t.id == track.id) {
                track.session.take_state(&old.session);
            }
        }
        for track in &mut plan.audio_tracks {
            if let Some(old) = self.plan.audio_tracks.iter().find(|t| t.id == track.id) {
                track.session.take_state(&old.session);
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
            if let Some(plugin) = plugin_of(&self.plugins, plan.instrument) {
                for pitch in 0..128u8 {
                    plugin.note(0, pitch, 0);
                }
            }
        }
        self.live_track = track;
    }

    fn play_live(&mut self, track: Id, event: MidiEvent) {
        let playing = self.state == TransportState::Playing;
        if let Some(plan) = self.plan.tracks.iter_mut().find(|t| t.id == track) {
            let mut out = [0u8; fnoob_core::midi_fx::MAX_OUT];
            let count = fnoob_core::midi_fx::pitches(&plan.midi_fx, event.pitch, &mut out);
            for &pitch in &out[..count] {
                if let (Some(arp), true) = (&mut plan.arp, playing) {
                    if event.velocity > 0 {
                        arp.hold(pitch, event.velocity, NO_OFF);
                    } else {
                        arp.release(pitch);
                    }
                } else if let Some(plugin) = plugin_of(&self.plugins, plan.instrument) {
                    plugin.note(self.frame, pitch, event.velocity);
                } else if event.velocity > 0 {
                    plan.synth.note_on(pitch, event.velocity, u64::MAX, true);
                } else {
                    plan.synth.note_off_live(pitch);
                }
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
                self.fire_queued_now();
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
            Command::SetAudioInput { samples, channels } => {
                self.input_channels = usize::from(channels).min(MAX_INPUT_CHANNELS);
                if let Some(old) = self.input.replace(samples) {
                    let _ = self.garbage.push(Garbage::Input(old));
                }
            }
            Command::InstallPlugin { slot, plugin } => {
                self.install(slot, EngineSlot::Vst3(plugin));
            }
            Command::InstallBuiltin { slot, effect } => {
                self.install(slot, EngineSlot::Builtin(effect));
            }
            Command::SetBuiltinParams { slot, params } => {
                if let Some(Some(EngineSlot::Builtin(effect))) = self.plugins.get_mut(slot) {
                    effect.set_params(params);
                }
            }
            Command::LiveNote(event) => self.live_event(event),
            Command::SetOutputs(outputs) => self.outputs = outputs,
            Command::LaunchClip { .. }
            | Command::ReleaseClip { .. }
            | Command::StopClips(_)
            | Command::LaunchScene(_)
            | Command::BackToArrangement => self.session_command(command),
            Command::PreviewAudio(data) => {
                self.preview_at = 0;
                let old = std::mem::replace(&mut self.preview, data);
                if let Some(old) = old {
                    let _ = self.garbage.push(Garbage::Audio(old));
                }
            }
            Command::RemovePlugin(slot) => {
                if let Some(old) = self.plugins.get_mut(slot).and_then(Option::take) {
                    let _ = self.garbage.push(old.into_garbage());
                }
            }
            Command::SetRecordSink(sink) => {
                if let Some(old) = self.sink.replace(sink) {
                    let _ = self.garbage.push(Garbage::Sink(old));
                }
            }
        }
    }

    fn publish_meters(&mut self) {
        for (meter, peak) in self.shared.meters.iter().zip(self.peaks.iter_mut()) {
            if *peak > 0.0 {
                meter.fetch_max(peak.to_bits(), Ordering::Relaxed);
                *peak = 0.0;
            }
        }
    }

    fn trim_input_backlog(&mut self) {
        let width = self.input_channels;
        if let Some(input) = &mut self.input {
            let limit = INPUT_BACKLOG_FRAMES * width.max(1);
            while input.slots() > limit {
                let _ = input.pop();
            }
        }
    }

    fn read_input_frame(&mut self) {
        let width = self.input_channels;
        let frame = &mut self.input_frame[..width];
        match &mut self.input {
            Some(input) if input.slots() >= width => {
                for sample in frame.iter_mut() {
                    *sample = input.pop().unwrap_or(0.0);
                }
            }
            _ => frame.fill(0.0),
        }
    }

    fn input_pair(&self, track: &AudioTrackPlan) -> (f32, f32) {
        let first = usize::from(track.input.first_channel);
        let get = |i: usize| {
            if i < self.input_channels {
                self.input_frame[i]
            } else {
                0.0
            }
        };
        let left = get(first);
        let right = if track.input.channels > 1 {
            get(first + 1)
        } else {
            left
        };
        (left, right)
    }

    fn record_input_frame(&mut self) {
        let width = self.input_channels;
        let Some(sink) = &mut self.sink else {
            return;
        };
        if width == 0 {
            return;
        }
        if !self.take_started {
            self.take_started = true;
            self.shared
                .take_start
                .store(self.position, Ordering::Release);
        }
        if sink.slots() < width {
            self.shared.overruns.fetch_add(1, Ordering::Relaxed);
            return;
        }
        for &sample in &self.input_frame[..width] {
            let _ = sink.push(sample);
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.peek() {
            let returns_garbage = matches!(
                command,
                Command::SetPlan(_)
                    | Command::SetMidiInput(_)
                    | Command::SetAudioInput { .. }
                    | Command::SetRecordSink(_)
                    | Command::InstallPlugin { .. }
                    | Command::InstallBuiltin { .. }
                    | Command::RemovePlugin(_)
                    | Command::PreviewAudio(_)
            );
            if returns_garbage && self.garbage.is_full() {
                break;
            }
            match self.commands.pop() {
                Ok(command) => self.apply(command),
                Err(_) => break,
            }
        }
    }

    fn in_record_window(&self) -> bool {
        let (start, end) = self.plan.record_window;
        (start..end).contains(&self.position)
    }

    fn live_event(&mut self, event: MidiEvent) {
        if let Some(track) = self.live_track {
            self.play_live(track, event);
        }
        let playing = self.state == TransportState::Playing;
        let _ = self.played.push(Played {
            clock: self.clock + self.frame as u64,
            position: playing.then_some(self.position),
            pitch: event.pitch,
            velocity: event.velocity,
        });
        if self.recording && playing && self.in_record_window() {
            self.recorder.event(self.position, event);
        }
    }

    fn drain_midi(&mut self) {
        while let Some(event) = self.midi.as_mut().and_then(|m| m.pop().ok()) {
            self.live_event(event);
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
            self.fire_queued_now();
        }
        if self.position == self.next_beat_sample {
            if self.plan.metronome {
                self.metronome.trigger(self.next_downbeat);
            }
            self.schedule_beat(self.next_beat + 1, self.position + 1);
        }
        self.send_due_offs();
        let (position, frame) = (self.position, self.frame);
        let quantum = self.plan.launch_quantum;
        for track in &mut self.plan.tracks {
            let overrides = track.session.play.overrides;
            while let Some(&note) = track.notes.get(track.cursor)
                && note.start <= position
            {
                if note.end > position && !overrides && passes(note.chance, &mut self.rng) {
                    trigger(
                        track,
                        &self.plugins,
                        frame,
                        note.pitch,
                        note.velocity,
                        note.end,
                    );
                }
                track.cursor += 1;
            }
            if track
                .session
                .step(position, quantum, &mut self.rng)
                .is_some()
            {
                release_track(track, &self.plugins, frame);
            }
            if track.audible && track.session.play.playing.is_some() {
                loop {
                    let elapsed = track.session.play.elapsed;
                    let Some(&note) = track
                        .session
                        .clip()
                        .and_then(|c| c.notes.get(track.session.play.cursor))
                        .filter(|n| n.start <= elapsed)
                    else {
                        break;
                    };
                    track.session.play.cursor += 1;
                    if note.start == elapsed && passes(note.chance, &mut self.rng) {
                        let end = position + (note.end - note.start);
                        trigger(track, &self.plugins, frame, note.pitch, note.velocity, end);
                    }
                }
            }
            track.session.advance();
            if let Some((pitch, velocity, end)) =
                track.arp.as_mut().and_then(|arp| arp.tick(position))
            {
                sound(track, &self.plugins, frame, pitch, velocity, end);
            }
        }
        for track in &mut self.plan.audio_tracks {
            track.session.step(position, quantum, &mut self.rng);
            if !track.audible {
                track.session.advance();
            }
        }
    }

    fn publish_session(&self) {
        for (index, cell) in self.shared.session.iter().enumerate() {
            let status = self
                .plan
                .tracks
                .iter()
                .find(|t| t.strip.meter == index)
                .map(|t| t.session.status())
                .or_else(|| {
                    self.plan
                        .audio_tracks
                        .iter()
                        .find(|t| t.strip.meter == index)
                        .map(|t| t.session.status())
                })
                .unwrap_or(0);
            cell.store(status, Ordering::Relaxed);
        }
    }

    pub fn process(&mut self, out: &mut [f32]) {
        let begin = std::time::Instant::now();
        self.drain_commands();
        self.frame = 0;
        self.drain_midi();
        self.trim_input_backlog();
        if self.channels == 0 {
            out.fill(0.0);
            return;
        }
        let width = MAX_BLOCK * self.channels;
        for chunk in out.chunks_mut(width) {
            self.render(chunk);
        }
        self.publish_meters();
        self.publish_session();
        self.shared.position.store(self.position, Ordering::Relaxed);
        self.shared
            .state
            .store(self.state.to_u8(), Ordering::Relaxed);
        self.shared
            .recording
            .store(self.recording, Ordering::Release);
        let frames = out.len() / self.channels.max(1);
        let budget = frames as f64 / f64::from(self.sample_rate.max(1));
        if budget > 0.0 {
            let load = (begin.elapsed().as_secs_f64() / budget) as f32;
            self.shared.cpu.store(load.to_bits(), Ordering::Relaxed);
        }
    }
}

impl Processor {
    fn transport(&self, playing: bool, position: u64) -> Transport {
        Transport {
            playing,
            position,
            tempo: self
                .plan
                .timeline
                .bpm_at_tick(self.plan.timeline.sample_to_tick(position)),
            numerator: self.plan.numerator,
            denominator: self.plan.denominator,
            quarter_notes: self.plan.timeline.sample_to_tick_f64(position as f64) / 960.0,
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        let frames = out.len() / self.channels;
        let start = self.position;
        let started_playing = self.state == TransportState::Playing;
        for f in 0..frames {
            self.frame = f;
            self.read_input_frame();
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
            self.positions[f] = position;
            self.click[f] = self.metronome.next_sample();
            for track in &mut self.plan.tracks {
                if track.audible {
                    let (left, right) = if track.instrument.is_some() {
                        (0.0, 0.0)
                    } else {
                        track.synth.next_frame(position)
                    };
                    track.left[f] = left;
                    track.right[f] = right;
                }
            }
            for index in 0..self.plan.audio_tracks.len() {
                let track = &self.plan.audio_tracks[index];
                if !track.audible {
                    continue;
                }
                let (mut left, mut right) = if track.monitor {
                    self.input_pair(track)
                } else {
                    (0.0, 0.0)
                };
                if playing {
                    let track = &mut self.plan.audio_tracks[index];
                    if !track.session.play.overrides {
                        let (l, r) = clip_frame(track, position);
                        left += l;
                        right += r;
                    }
                    let (l, r) = track.session.audio_frame();
                    left += l;
                    right += r;
                    track.session.advance();
                }
                let track = &mut self.plan.audio_tracks[index];
                track.left[f] = left;
                track.right[f] = right;
            }
            if playing && self.recording && self.position == self.plan.record_window.1 {
                self.recorder.end_held(self.position);
            }
            if playing && self.recording && self.plan.records_audio && self.in_record_window() {
                self.record_input_frame();
            }
            if playing {
                self.position += 1;
            }
        }
        self.clock += frames as u64;
        let transport = self.transport(started_playing, start);
        self.main_left[..frames].fill(0.0);
        self.main_right[..frames].fill(0.0);
        for (left, right) in self
            .return_left
            .iter_mut()
            .zip(self.return_right.iter_mut())
        {
            left[..frames].fill(0.0);
            right[..frames].fill(0.0);
        }
        for group in &mut self.plan.groups {
            group.left[..frames].fill(0.0);
            group.right[..frames].fill(0.0);
        }
        for step in 0..self.plan.order.len() {
            match self.plan.order[step] {
                Node::Track(index) => {
                    if !self.plan.tracks[index].audible {
                        continue;
                    }
                    feed_keys(
                        &mut self.plugins,
                        &self.plan.tracks[index].keys,
                        &self.plan.tracks,
                        &self.plan.audio_tracks,
                        frames,
                    );
                    let track = &mut self.plan.tracks[index];
                    if let Some(plugin) = plugin_of_mut(&mut self.plugins, track.instrument) {
                        run_plugin(
                            plugin,
                            &mut track.left[..frames],
                            &mut track.right[..frames],
                            &transport,
                        );
                    }
                    apply_device_automation(&mut self.plugins, &track.device_automation, start);
                    run_inserts(
                        &mut self.plugins,
                        &track.strip.inserts,
                        &mut track.left[..frames],
                        &mut track.right[..frames],
                        &transport,
                    );
                    for f in 0..frames {
                        let track = &self.plan.tracks[index];
                        let strip = track.strip.at(&track.envelopes, self.positions[f]);
                        let (left, right) = (track.left[f], track.right[f]);
                        self.mix_frame(&strip, left, right, f);
                    }
                }
                Node::Audio(index) => {
                    if !self.plan.audio_tracks[index].audible {
                        continue;
                    }
                    feed_keys(
                        &mut self.plugins,
                        &self.plan.audio_tracks[index].keys,
                        &self.plan.tracks,
                        &self.plan.audio_tracks,
                        frames,
                    );
                    let track = &mut self.plan.audio_tracks[index];
                    apply_device_automation(&mut self.plugins, &track.device_automation, start);
                    run_inserts(
                        &mut self.plugins,
                        &track.strip.inserts,
                        &mut track.left[..frames],
                        &mut track.right[..frames],
                        &transport,
                    );
                    for f in 0..frames {
                        let track = &self.plan.audio_tracks[index];
                        let strip = track.strip.at(&track.envelopes, self.positions[f]);
                        let (left, right) = (track.left[f], track.right[f]);
                        self.mix_frame(&strip, left, right, f);
                    }
                }
            }
        }
        for index in 0..self.plan.groups.len() {
            if !self.plan.groups[index].audible {
                continue;
            }
            feed_keys(
                &mut self.plugins,
                &self.plan.groups[index].keys,
                &self.plan.tracks,
                &self.plan.audio_tracks,
                frames,
            );
            {
                let group = &mut self.plan.groups[index];
                apply_device_automation(&mut self.plugins, &group.device_automation, start);
                run_inserts(
                    &mut self.plugins,
                    &group.strip.inserts,
                    &mut group.left[..frames],
                    &mut group.right[..frames],
                    &transport,
                );
            }
            for f in 0..frames {
                let group = &self.plan.groups[index];
                let mut strip = group.strip.at(&group.envelopes, self.positions[f]);
                strip.route = None;
                strip.group = strip.group.filter(|p| *p != index);
                let (left, right) = (group.left[f], group.right[f]);
                self.mix_frame(&strip, left, right, f);
            }
        }
        let count = self.plan.returns.len().min(MAX_RETURNS);
        for index in 0..count {
            run_inserts(
                &mut self.plugins,
                &self.plan.returns[index].inserts,
                &mut self.return_left[index][..frames],
                &mut self.return_right[index][..frames],
                &transport,
            );
        }
        for (f, frame) in out.chunks_exact_mut(self.channels).enumerate() {
            let (mut sum_l, mut sum_r) = (self.main_left[f], self.main_right[f]);
            for index in 0..count {
                let plan = self.plan.returns[index];
                let l = self.return_left[index][f] * plan.gain.0;
                let r = self.return_right[index][f] * plan.gain.1;
                peak(&mut self.peaks, plan.meter, l, r);
                sum_l += l;
                sum_r += r;
            }
            let master = self.plan.master;
            let left = sum_l * master;
            let right = sum_r * master;
            peak(&mut self.peaks, MASTER_METER, left, right);
            let click = self.click[f];
            let (pl, pr) = match &self.preview {
                Some(data) if self.preview_at < data.frames => {
                    let sample = data.frame(self.preview_at);
                    self.preview_at += 1;
                    sample
                }
                _ => (0.0, 0.0),
            };
            let gain = self.outputs.cue_gain;
            write_outputs(
                frame,
                (left + click, right + click),
                (pl * gain, pr * gain),
                self.outputs,
            );
        }
        self.frame = 0;
    }
}

pub enum EngineSlot {
    Vst3(Box<PluginProcessor>),
    Builtin(Box<BuiltinProcessor>),
}

impl EngineSlot {
    fn into_garbage(self) -> Garbage {
        match self {
            Self::Vst3(plugin) => Garbage::Plugin(plugin),
            Self::Builtin(effect) => Garbage::Builtin(effect),
        }
    }
}

impl Processor {
    fn install(&mut self, slot: usize, device: EngineSlot) {
        match self.plugins.get_mut(slot) {
            Some(target) => {
                if let Some(old) = target.replace(device) {
                    let _ = self.garbage.push(old.into_garbage());
                }
            }
            None => {
                let _ = self.garbage.push(device.into_garbage());
            }
        }
    }
}

fn plugin_of(plugins: &[Option<EngineSlot>], slot: Option<usize>) -> Option<&PluginProcessor> {
    match plugins.get(slot?)? {
        Some(EngineSlot::Vst3(plugin)) => Some(plugin),
        _ => None,
    }
}

fn plugin_of_mut(
    plugins: &mut [Option<EngineSlot>],
    slot: Option<usize>,
) -> Option<&mut PluginProcessor> {
    match plugins.get_mut(slot?)? {
        Some(EngineSlot::Vst3(plugin)) => Some(plugin),
        _ => None,
    }
}

fn run_plugin(
    plugin: &mut PluginProcessor,
    left: &mut [f32],
    right: &mut [f32],
    transport: &Transport,
) {
    assert_no_alloc::permit_alloc(|| plugin.process(left, right, transport));
    for sample in left.iter_mut().chain(right.iter_mut()) {
        if !sample.is_finite() {
            *sample = 0.0;
        }
    }
}

fn apply_device_automation(
    plugins: &mut [Option<EngineSlot>],
    automation: &[crate::plan::DeviceAutomation],
    position: u64,
) {
    for device in automation {
        match (plugins.get_mut(device.slot), device.base) {
            (Some(Some(EngineSlot::Builtin(effect))), Some(base)) => {
                let mut params = base;
                for (param, lane) in &device.lanes {
                    let Ok(param) = u8::try_from(*param) else {
                        continue;
                    };
                    let current = params.normal(param).unwrap_or(0.0);
                    params.set_normal(param, crate::plan::envelope_at(lane, position, current));
                }
                effect.set_params(params);
            }
            (Some(Some(EngineSlot::Vst3(plugin))), None) => {
                for (param, lane) in &device.lanes {
                    let value = crate::plan::envelope_at(lane, position, 0.0);
                    plugin.set_param(*param, f64::from(value));
                }
            }
            _ => {}
        }
    }
}

fn run_inserts(
    plugins: &mut [Option<EngineSlot>],
    inserts: &Inserts,
    left: &mut [f32],
    right: &mut [f32],
    transport: &Transport,
) {
    for (slot, bypass) in inserts.iter().flatten() {
        if *bypass {
            continue;
        }
        match plugins.get_mut(*slot) {
            Some(Some(EngineSlot::Vst3(plugin))) => run_plugin(plugin, left, right, transport),
            Some(Some(EngineSlot::Builtin(effect))) => {
                effect.process(left, right, transport.tempo);
            }
            _ => {}
        }
    }
}

impl Processor {
    fn mix_frame(&mut self, strip: &StripPlan, left: f32, right: f32, f: usize) {
        let post = (left * strip.left, right * strip.right);
        if let Some(target) = strip.route {
            if let Some(track) = self.plan.audio_tracks.get_mut(target)
                && track.audible
            {
                track.left[f] += post.0;
                track.right[f] += post.1;
            }
        } else if let Some(group) = strip.group.and_then(|g| self.plan.groups.get_mut(g)) {
            group.left[f] += post.0;
            group.right[f] += post.1;
        } else if strip.to_main {
            self.main_left[f] += post.0;
            self.main_right[f] += post.1;
        }
        for index in 0..MAX_RETURNS {
            let send = strip.sends[index];
            if send > 0.0 {
                let source = if strip.pre_fader[index] {
                    (left, right)
                } else {
                    post
                };
                self.return_left[index][f] += source.0 * send;
                self.return_right[index][f] += source.1 * send;
            }
        }
        peak(&mut self.peaks, strip.meter, post.0, post.1);
    }
}

fn feed_keys(
    plugins: &mut [Option<EngineSlot>],
    keys: &[(usize, Node)],
    tracks: &[TrackPlan],
    audio: &[AudioTrackPlan],
    frames: usize,
) {
    for (slot, source) in keys {
        let buffers = match *source {
            Node::Track(i) => tracks
                .get(i)
                .filter(|t| t.audible)
                .map(|t| (&t.left[..frames], &t.right[..frames])),
            Node::Audio(i) => audio
                .get(i)
                .filter(|t| t.audible)
                .map(|t| (&t.left[..frames], &t.right[..frames])),
        };
        if let Some(Some(EngineSlot::Builtin(effect))) = plugins.get_mut(*slot) {
            effect.set_key(buffers, frames);
        }
    }
}

fn passes(chance: u8, rng: &mut u64) -> bool {
    chance >= fnoob_core::model::FULL_CHANCE
        || crate::session::random(rng) * 100.0 < f32::from(chance)
}

fn peak(peaks: &mut [f32], slot: usize, left: f32, right: f32) {
    for (index, level) in [(slot * 2, left.abs()), (slot * 2 + 1, right.abs())] {
        if let Some(p) = peaks.get_mut(index)
            && level.is_finite()
            && level > *p
        {
            *p = level;
        }
    }
}

fn release_track(track: &mut TrackPlan, plugins: &[Option<EngineSlot>], frame: usize) {
    track.synth.release_scheduled();
    if let Some(arp) = &mut track.arp {
        arp.clear();
    }
    if let Some(plugin) = plugin_of(plugins, track.instrument) {
        for pitch in 0..128u8 {
            if track.pending_offs[usize::from(pitch)] != NO_OFF {
                plugin.note(frame, pitch, 0);
            }
        }
    }
    track.pending_offs = [NO_OFF; 128];
    track.next_off = NO_OFF;
}

fn trigger(
    track: &mut TrackPlan,
    plugins: &[Option<EngineSlot>],
    frame: usize,
    pitch: u8,
    velocity: u8,
    end: u64,
) {
    if let Some(arp) = &mut track.arp {
        arp.hold(pitch, velocity, end);
        return;
    }
    sound(track, plugins, frame, pitch, velocity, end);
}

fn sound(
    track: &mut TrackPlan,
    plugins: &[Option<EngineSlot>],
    frame: usize,
    pitch: u8,
    velocity: u8,
    end: u64,
) {
    match plugin_of(plugins, track.instrument) {
        Some(plugin) => {
            let slot = &mut track.pending_offs[usize::from(pitch.min(127))];
            if *slot != NO_OFF {
                plugin.note(frame, pitch, 0);
            }
            plugin.note(frame, pitch, velocity);
            *slot = end;
            track.next_off = track.next_off.min(end);
        }
        None => track.synth.note_on(pitch, velocity, end, false),
    }
}

pub fn render_clips(track: &mut AudioTrackPlan, from: u64, frames: u64) -> Vec<f32> {
    track.cursor = 0;
    let mut out = Vec::with_capacity(frames as usize * 2);
    for position in from..from + frames {
        let (l, r) = clip_frame(track, position);
        out.push(l);
        out.push(r);
    }
    out
}

const GRAIN: u64 = 2048;
const HOP: u64 = GRAIN / 2;

fn frame_at(data: &AudioData, position: f64) -> (f32, f32) {
    if position < 0.0 {
        return (0.0, 0.0);
    }
    let index = position.floor();
    let frac = (position - index) as f32;
    let (a, b) = (data.frame(index as u64), data.frame(index as u64 + 1));
    (a.0 + (b.0 - a.0) * frac, a.1 + (b.1 - a.1) * frac)
}

fn warped_frame(
    clip: &crate::plan::Clip,
    warp: &crate::plan::ClipWarp,
    position: u64,
) -> (f32, f32) {
    let offset = clip.offset as f64;
    if warp.repitch {
        return frame_at(&clip.data, offset + warp.source_at(position));
    }
    let into = position - clip.start;
    let grain = into / HOP;
    let (mut left, mut right) = (0.0, 0.0);
    for index in [grain.checked_sub(1), Some(grain)].into_iter().flatten() {
        let begin = index * HOP;
        let inside = into - begin;
        let weight = if index == 0 && inside < HOP {
            1.0
        } else {
            let phase = std::f32::consts::PI * inside as f32 / GRAIN as f32;
            phase.sin() * phase.sin()
        };
        let source = offset + warp.source_at(clip.start + begin) + inside as f64;
        let (l, r) = frame_at(&clip.data, source);
        left += l * weight;
        right += r * weight;
    }
    (left, right)
}

fn clip_frame(track: &mut AudioTrackPlan, position: u64) -> (f32, f32) {
    while let Some(clip) = track.clips.get(track.cursor)
        && clip.end <= position
    {
        track.cursor += 1;
    }
    let (mut left, mut right) = (0.0, 0.0);
    for clip in &track.clips[track.cursor..] {
        if clip.start > position {
            break;
        }
        if clip.end > position {
            let into = position - clip.start;
            let left_over = clip.end - position;
            let mut gain = clip.gain;
            if into < clip.fade_in {
                gain *=
                    fnoob_core::model::shape(into as f32 / clip.fade_in as f32, clip.fade_in_curve);
            }
            if left_over < clip.fade_out {
                gain *= fnoob_core::model::shape(
                    left_over as f32 / clip.fade_out as f32,
                    clip.fade_out_curve,
                );
            }
            if into < clip.xfade_in {
                gain *= (into as f32 / clip.xfade_in as f32 * std::f32::consts::FRAC_PI_2).sin();
            }
            if clip.xfade_out > 0 && position >= clip.xfade_from {
                let t = (position - clip.xfade_from) as f32 / clip.xfade_out as f32;
                gain *= if t >= 1.0 {
                    0.0
                } else {
                    ((1.0 - t) * std::f32::consts::FRAC_PI_2).sin()
                };
            }
            let (l, r) = match &clip.warp {
                Some(warp) => warped_frame(clip, warp, position),
                None => {
                    let source = if clip.loop_len > 0 {
                        into % clip.loop_len
                    } else {
                        into
                    };
                    clip.data.frame(clip.offset + source)
                }
            };
            left += l * gain;
            right += r * gain;
        }
    }
    (left, right)
}

fn safe(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn write_outputs(frame: &mut [f32], main: (f32, f32), cue: (f32, f32), outputs: Outputs) {
    if let [only] = frame {
        *only = safe((main.0 + main.1 + cue.0 + cue.1) * 0.5);
        return;
    }
    frame.fill(0.0);
    let pairs = frame.len() / 2;
    if pairs == 0 {
        return;
    }
    let main_pair = if outputs.main < pairs {
        outputs.main
    } else {
        0
    };
    let cue_pair = if outputs.cue < pairs {
        outputs.cue
    } else {
        main_pair
    };
    for (pair, (left, right)) in [(main_pair, main), (cue_pair, cue)] {
        frame[pair * 2] += left;
        frame[pair * 2 + 1] += right;
    }
    for sample in frame.iter_mut() {
        *sample = safe(*sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_data::{AudioData, AudioPool};
    use crate::plan::return_meter;
    use fnoob_core::edit::{
        add_audio_file, add_audio_region, add_midi_region, add_note, add_track,
    };
    use fnoob_core::model::{
        AudioInput, Instrument, Note, Project, SILENT_DB, SynthParams, TrackKind, Waveform,
    };

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
            chance: 100,
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
    fn punch_records_only_inside_the_loop_brace() {
        let (mut project, track, _) = synth_project();
        project.loop_range.start_tick = 3840;
        project.loop_range.end_tick = 7680;
        project.punch.punch_in = true;
        project.punch.punch_out = true;
        let (mut p, mut h) = create(SR, 1, plan(&project));
        let (mut tx, rx) = midi_channel();
        h.send(Command::SetMidiInput(rx)).unwrap();
        h.send(Command::SetLiveTrack(Some(track))).unwrap();
        h.send(Command::Record).unwrap();
        render(&mut p, 1000);
        for (pitch, frames) in [(60, 99_000), (62, 100_000), (64, 100)] {
            tx.push(MidiEvent {
                pitch,
                velocity: 100,
            })
            .unwrap();
            render(&mut p, frames);
        }
        let events: Vec<(u64, u8, u8)> = recorded(&mut h)
            .iter()
            .map(|e| (e.sample, e.pitch, e.velocity))
            .collect();
        assert_eq!(events, vec![(100_000, 62, 100), (192_000, 62, 0)]);
    }

    #[test]
    fn played_notes_are_reported_while_stopped() {
        let (project, track, _) = synth_project();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        let (mut tx, rx) = midi_channel();
        h.send(Command::SetMidiInput(rx)).unwrap();
        h.send(Command::SetLiveTrack(Some(track))).unwrap();
        render(&mut p, 512);
        tx.push(MidiEvent {
            pitch: 60,
            velocity: 100,
        })
        .unwrap();
        render(&mut p, 512);
        tx.push(MidiEvent {
            pitch: 60,
            velocity: 0,
        })
        .unwrap();
        render(&mut p, 512);
        let played: Vec<Played> = std::iter::from_fn(|| h.pop_played()).collect();
        assert_eq!(played.len(), 2);
        assert_eq!((played[0].clock, played[0].position), (512, None));
        assert_eq!((played[1].clock, played[1].velocity), (1024, 0));
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
    fn launched_clip_waits_for_the_bar_and_mutes_the_timeline() {
        let (mut project, track, region) = synth_project();
        add_note(&mut project, region, note(0, 480));
        add_note(&mut project, region, note(8640, 480));
        let clip = fnoob_core::session::add_midi_clip(&mut project, track, 0, 1920).unwrap();
        add_note(&mut project, clip, note(0, 480));
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        let mut out = render(&mut p, 100);
        h.send(Command::LaunchClip { track, slot: 0 }).unwrap();
        out.extend(render(&mut p, 11 * BEAT - 100));
        assert_eq!(
            onsets(&out),
            vec![0, 4 * BEAT, 6 * BEAT, 8 * BEAT, 10 * BEAT]
        );
        let status = h.session_status(0);
        assert_eq!(status.playing, Some(0));
        assert!(status.overrides);
        h.send(Command::BackToArrangement).unwrap();
        render(&mut p, 10);
        assert!(!h.session_status(0).overrides);
        h.send(Command::Stop).unwrap();
        render(&mut p, 10);
        assert_eq!(h.session_status(0).playing, None);
    }

    #[test]
    fn scene_launch_starts_every_track_in_the_row() {
        let mut project = metronome_only();
        let mut tracks = Vec::new();
        for _ in 0..3 {
            let track = add_track(&mut project, TrackKind::Instrument);
            let clip = fnoob_core::session::add_midi_clip(&mut project, track, 0, 3840).unwrap();
            add_note(&mut project, clip, note(0, 480));
            tracks.push(track);
        }
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::LaunchScene(0)).unwrap();
        render(&mut p, 1000);
        for index in 0..3 {
            assert_eq!(h.session_status(index).playing, Some(0), "track {index}");
        }
    }

    #[test]
    fn main_and_cue_go_to_their_output_pairs() {
        let mut frame = [9.0f32; 4];
        let outputs = Outputs {
            main: 1,
            cue: 0,
            cue_gain: 1.0,
        };
        write_outputs(&mut frame, (0.25, 0.5), (0.1, 0.2), outputs);
        assert_eq!(frame, [0.1, 0.2, 0.25, 0.5]);
        write_outputs(&mut frame, (0.25, 0.5), (0.1, 0.2), Outputs::default());
        assert_eq!(frame, [0.35, 0.7, 0.0, 0.0]);
        let mut stereo = [0.0f32; 2];
        let far = Outputs {
            main: 3,
            cue: 5,
            cue_gain: 1.0,
        };
        write_outputs(&mut stereo, (0.25, 0.5), (0.0, 0.0), far);
        assert_eq!(stereo, [0.25, 0.5]);
        let mut mono = [0.0f32; 1];
        write_outputs(&mut mono, (0.5, 0.5), (0.0, 0.0), far);
        assert_eq!(mono, [0.5]);
    }

    #[test]
    fn clicks_follow_tempo_changes() {
        let mut project = metronome_only();
        fnoob_core::edit::set_tempo_change(&mut project, 1920, 60.0);
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        let out = render(&mut p, 6 * BEAT + 100);
        assert_eq!(onsets(&out), vec![0, BEAT, 2 * BEAT, 4 * BEAT, 6 * BEAT]);
        let tick = p.plan.timeline.sample_to_tick(p.position);
        assert_eq!(p.plan.timeline.bpm_at_tick(tick), 60.0);
    }

    #[test]
    fn clicks_follow_meter_changes() {
        let mut project = metronome_only();
        fnoob_core::edit::set_meter_change(
            &mut project,
            2,
            fnoob_core::model::TimeSignature {
                numerator: 6,
                denominator: 8,
            },
        );
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        let out = render(&mut p, 5 * BEAT + 100);
        assert_eq!(
            onsets(&out),
            vec![
                0,
                BEAT,
                2 * BEAT,
                3 * BEAT,
                4 * BEAT,
                9 * BEAT / 2,
                5 * BEAT
            ]
        );
    }

    #[test]
    fn launching_an_audio_clip_starts_the_transport() {
        let mut project = metronome_only();
        project.metronome.enabled = false;
        let track = add_track(&mut project, TrackKind::Audio);
        let file = add_audio_file(&mut project, "audio/a.wav".into(), 1, 1000);
        fnoob_core::session::add_audio_clip(&mut project, track, 1, file).unwrap();
        fnoob_core::session::clip_mut(&mut project, track, 1)
            .unwrap()
            .looping = false;
        let mut pool = AudioPool::new();
        pool.insert(file, Arc::new(AudioData::from_samples(1, vec![0.5; 1000])));
        let (mut p, mut h) = create(SR, 1, Box::new(RenderPlan::with_audio(&project, SR, &pool)));
        h.send(Command::LaunchClip { track, slot: 1 }).unwrap();
        let out = render(&mut p, 2000);
        assert!(out[0] != 0.0 && out[999] != 0.0);
        assert!(out[1500..].iter().all(|&s| s == 0.0));
        assert!(h.is_running());
        assert_eq!(h.session_status(0).playing, None);
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

    fn assert_close(got: &[f32], want: &[f32]) {
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-6, "{got:?} vs {want:?}");
        }
    }

    fn mixer_project() -> (Project, Id, Id) {
        let data = AudioData::from_samples(1, vec![0.5; 4000]);
        let (project, track, _) = audio_project(&data, 0);
        let file = project.audio_files[0].id;
        (project, track, file)
    }

    fn mixer_pool(project: &Project) -> AudioPool {
        let mut pool = AudioPool::new();
        pool.insert(
            project.audio_files[0].id,
            Arc::new(AudioData::from_samples(1, vec![0.5; 4000])),
        );
        pool
    }

    fn first_frame(project: &Project) -> ([f32; 2], EngineHandle) {
        let pool = mixer_pool(project);
        let (mut p, mut h) = create(SR, 2, audio_plan(project, &pool));
        h.send(Command::Play).unwrap();
        let mut out = vec![0.0; 2];
        p.process(&mut out);
        ([out[0], out[1]], h)
    }

    #[test]
    fn fader_and_pan_shape_the_track() {
        let (mut project, track, _) = mixer_project();
        let strip = &mut project.track_mut(track).unwrap().strip;
        strip.volume_db = -6.0;
        strip.pan = -1.0;
        let (frame, _) = first_frame(&project);
        assert!((frame[0] - 0.5 * 0.501_187).abs() < 1e-5);
        assert!(frame[1].abs() < 1e-6);
        project.track_mut(track).unwrap().strip.volume_db = SILENT_DB;
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
    }

    #[test]
    fn nested_groups_pass_sound_upward() {
        let (mut project, track, _) = mixer_project();
        let inner = fnoob_core::edit::group_tracks(&mut project, &[track]).unwrap();
        let outer = fnoob_core::edit::group_tracks(&mut project, &[inner]).unwrap();
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let (frame, _) = first_frame(&project);
        assert_close(&frame, &[0.5 * c * c * c, 0.5 * c * c * c]);
        project.track_mut(outer).unwrap().mute = true;
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
    }

    #[test]
    fn note_chance_decides_how_often_a_note_plays() {
        let mut rng = 42;
        assert!((0..100).all(|_| passes(100, &mut rng)));
        assert!((0..100).all(|_| !passes(0, &mut rng)));
        let hits = (0..2000).filter(|_| passes(25, &mut rng)).count();
        assert!((350..650).contains(&hits), "{hits}");
    }

    #[test]
    fn group_tracks_carry_their_members() {
        let (mut project, track, _) = mixer_project();
        let group = fnoob_core::edit::group_tracks(&mut project, &[track]).unwrap();
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let (frame, _) = first_frame(&project);
        assert_close(&frame, &[0.5 * c * c, 0.5 * c * c]);
        project.track_mut(group).unwrap().strip.pan = -1.0;
        let (frame, _) = first_frame(&project);
        assert_close(&frame, &[0.5 * c, 0.0]);
        project.track_mut(group).unwrap().mute = true;
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
    }

    #[test]
    fn sends_feed_returns_pre_or_post_fader() {
        let (mut project, track, _) = mixer_project();
        fnoob_core::edit::add_return(&mut project, "One", None);
        fnoob_core::edit::add_return(&mut project, "Two", None);
        let c = std::f32::consts::FRAC_1_SQRT_2;
        {
            let strip = &mut project.track_mut(track).unwrap().strip;
            strip.volume_db = SILENT_DB;
            strip.send_mut(1).level_db = 0.0;
        }
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
        project
            .track_mut(track)
            .unwrap()
            .strip
            .send_mut(1)
            .pre_fader = true;
        let (frame, h) = first_frame(&project);
        assert_close(&frame, &[0.5 * c, 0.5 * c]);
        assert!((h.take_peak(return_meter(1)) - 0.5 * c).abs() < 1e-6);
        assert_eq!(h.take_peak(return_meter(0)), 0.0);
        project.returns[1].mute = true;
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
    }

    #[test]
    fn solo_never_silences_returns_and_no_output_skips_main() {
        let (mut project, track, _) = mixer_project();
        fnoob_core::edit::add_return(&mut project, "One", None);
        let other = add_track(&mut project, TrackKind::Audio);
        project.track_mut(other).unwrap().solo = true;
        project.track_mut(track).unwrap().strip.send_mut(0).level_db = 0.0;
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
        project.track_mut(track).unwrap().solo = true;
        project.track_mut(track).unwrap().strip.volume_db = SILENT_DB;
        project
            .track_mut(track)
            .unwrap()
            .strip
            .send_mut(0)
            .pre_fader = true;
        let (frame, _) = first_frame(&project);
        assert!(frame[0] > 0.0);
        let strip = &mut project.track_mut(track).unwrap().strip;
        strip.volume_db = 0.0;
        strip.send_mut(0).level_db = SILENT_DB;
        let (frame, _) = first_frame(&project);
        assert!(frame[0] > 0.0);
        project.track_mut(track).unwrap().output = fnoob_core::model::Output::None;
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
    }

    #[test]
    fn master_scales_everything_and_meters_report_peaks() {
        let (mut project, track, _) = mixer_project();
        project.master.volume_db = SILENT_DB;
        let (frame, h) = first_frame(&project);
        assert_eq!(frame, [0.0, 0.0]);
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let slot = project.tracks.iter().position(|t| t.id == track).unwrap();
        assert!((h.take_peak(slot) - 0.5 * c).abs() < 1e-6);
        assert_eq!(h.take_peak(MASTER_METER), 0.0);
    }

    fn audio_project(data: &AudioData, start_tick: u64) -> (Project, Id, AudioPool) {
        let mut project = metronome_only();
        project.metronome.enabled = false;
        let track = add_track(&mut project, TrackKind::Audio);
        let file = add_audio_file(
            &mut project,
            "audio/a.wav".into(),
            data.channels,
            data.frames,
        );
        add_audio_region(&mut project, track, start_tick, file, 0).unwrap();
        let mut pool = AudioPool::new();
        pool.insert(file, Arc::new(data.clone()));
        (project, track, pool)
    }

    #[test]
    fn looped_audio_clips_repeat_their_start() {
        let ramp: Vec<f32> = (0..4000).map(|i| i as f32 / 4000.0).collect();
        let data = AudioData::from_samples(1, ramp);
        let (project, _, pool) = audio_project(&data, 0);
        let mut plan = audio_plan(&project, &pool);
        let track = &mut plan.audio_tracks[0];
        track.clips[0].loop_len = 100;
        let first = clip_frame(track, 30);
        track.cursor = 0;
        let again = clip_frame(track, 130);
        assert!((first.0 - again.0).abs() < 1e-6);
        assert!(first.0 > 0.0);
    }

    fn audio_plan(project: &Project, pool: &AudioPool) -> Box<RenderPlan> {
        Box::new(RenderPlan::with_audio(project, SR, pool))
    }

    fn input_channel(h: &mut EngineHandle, channels: u16) -> Producer<f32> {
        let (tx, rx) = audio_channel(1 << 16);
        h.send(Command::SetAudioInput {
            samples: rx,
            channels,
        })
        .unwrap();
        tx
    }

    #[test]
    fn audio_clip_starts_on_its_sample_in_stereo() {
        let samples: Vec<f32> = (0..200)
            .flat_map(|i| [0.5, -0.25 - i as f32 * 0.0])
            .collect();
        let data = AudioData::from_samples(2, samples);
        let (project, _, pool) = audio_project(&data, 960);
        let (mut p, mut h) = create(SR, 2, audio_plan(&project, &pool));
        h.send(Command::Play).unwrap();
        let mut out = vec![1.0; 2 * (BEAT + 300)];
        p.process(&mut out);
        let frames = out.as_chunks::<2>().0;
        let c = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(frames[BEAT - 1], [0.0, 0.0]);
        assert_close(&frames[BEAT], &[0.5 * c, -0.25 * c]);
        assert_close(&frames[BEAT + 199], &[0.5 * c, -0.25 * c]);
        assert_eq!(frames[BEAT + 200], [0.0, 0.0]);
    }

    #[test]
    fn mono_output_mixes_left_and_right() {
        let data = AudioData::from_samples(2, vec![0.6, 0.2]);
        let (project, _, pool) = audio_project(&data, 0);
        let (mut p, mut h) = create(SR, 1, audio_plan(&project, &pool));
        h.send(Command::Play).unwrap();
        let c = std::f32::consts::FRAC_1_SQRT_2;
        assert_close(&render(&mut p, 2), &[0.4 * c, 0.0]);
    }

    #[test]
    fn missing_audio_is_silent() {
        let data = AudioData::from_samples(1, vec![0.5; 100]);
        let (project, _, _) = audio_project(&data, 0);
        let (mut p, mut h) = create(SR, 1, audio_plan(&project, &AudioPool::new()));
        h.send(Command::Play).unwrap();
        assert!(render(&mut p, 100).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn monitoring_passes_selected_input_channel() {
        let data = AudioData::from_samples(1, vec![0.0]);
        let (mut project, track, pool) = audio_project(&data, 0);
        project.track_mut(track).unwrap().monitor = fnoob_core::model::Monitor::In;
        project.track_mut(track).unwrap().audio_input = Some(AudioInput {
            first_channel: 1,
            channels: 1,
        });
        let (mut p, mut h) = create(SR, 2, audio_plan(&project, &pool));
        let mut tx = input_channel(&mut h, 2);
        for i in 0..4 {
            tx.push(0.9).unwrap();
            tx.push(i as f32 * 0.1).unwrap();
        }
        let mut out = vec![0.0; 8];
        p.process(&mut out);
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let want: Vec<f32> = [0.0, 0.0, 0.1, 0.1, 0.2, 0.2, 0.3, 0.3]
            .iter()
            .map(|v| v * c)
            .collect();
        assert_close(&out, &want);
    }

    #[test]
    fn recording_streams_input_and_marks_take_start() {
        let data = AudioData::from_samples(1, vec![0.0]);
        let (mut project, track, pool) = audio_project(&data, 100_000);
        project.track_mut(track).unwrap().record_enabled = true;
        project.loop_range.enabled = true;
        project.loop_range.end_tick = 10;
        let (mut p, mut h) = create(SR, 1, audio_plan(&project, &pool));
        let mut tx = input_channel(&mut h, 1);
        let (sink, mut recorded) = audio_channel(1 << 16);
        h.send(Command::SetRecordSink(sink)).unwrap();
        h.send(Command::Locate(500)).unwrap();
        h.send(Command::Record).unwrap();
        for i in 0..1000 {
            tx.push(i as f32).unwrap();
        }
        render(&mut p, 1000);
        assert_eq!(h.audio_take_start(), Some(500));
        assert_eq!(h.position(), 1500);
        let got: Vec<f32> = std::iter::from_fn(|| recorded.pop().ok()).collect();
        assert_eq!(got.len(), 1000);
        assert_eq!(got[10], 10.0);
        h.send(Command::Stop).unwrap();
        render(&mut p, 100);
        assert!(recorded.pop().is_err());
        assert_eq!(h.record_overruns(), 0);
    }

    #[test]
    fn audio_recording_keeps_looping_and_keeps_every_frame() {
        let data = AudioData::from_samples(1, vec![0.0]);
        let (mut project, track, pool) = audio_project(&data, 100_000);
        project.track_mut(track).unwrap().record_enabled = true;
        project.loop_range.enabled = true;
        project.loop_range.start_tick = 0;
        project.loop_range.end_tick = 40;
        let (mut p, mut h) = create(SR, 1, audio_plan(&project, &pool));
        let mut tx = input_channel(&mut h, 1);
        let (sink, mut recorded) = audio_channel(1 << 16);
        h.send(Command::SetRecordSink(sink)).unwrap();
        h.send(Command::Locate(500)).unwrap();
        h.send(Command::Record).unwrap();
        for i in 0..1600 {
            tx.push(i as f32).unwrap();
        }
        render(&mut p, 1600);
        assert_eq!(h.audio_take_start(), Some(500));
        assert_eq!(h.position(), 100);
        let got: Vec<f32> = std::iter::from_fn(|| recorded.pop().ok()).collect();
        assert_eq!(got.len(), 1600);
        assert_eq!(got[1599], 1599.0);
    }

    #[test]
    fn input_backlog_is_trimmed() {
        let (mut p, mut h) = create(SR, 1, plan(&metronome_only()));
        let mut tx = input_channel(&mut h, 1);
        for _ in 0..INPUT_BACKLOG_FRAMES * 3 {
            tx.push(0.0).unwrap();
        }
        render(&mut p, 16);
        assert_eq!(p.input.as_ref().unwrap().slots(), INPUT_BACKLOG_FRAMES - 16);
    }

    #[test]
    fn volume_automation_shapes_the_track() {
        let (mut project, track, _) = mixer_project();
        let a = &mut project.track_mut(track).unwrap().automation;
        a.volume = vec![
            fnoob_core::model::AutomationPoint {
                tick: 0,
                value: -60.0,
                curve: 0.0,
            },
            fnoob_core::model::AutomationPoint {
                tick: 96,
                value: 0.0,
                curve: 0.0,
            },
        ];
        let pool = mixer_pool(&project);
        let (mut p, mut h) = create(SR, 2, audio_plan(&project, &pool));
        h.send(Command::Play).unwrap();
        let mut out = vec![0.0; 2 * 3000];
        p.process(&mut out);
        let c = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(out[0], 0.0);
        assert!(out[2 * 1200] > 0.0 && out[2 * 1200] < 0.5 * c);
        assert!((out[2 * 2900] - 0.5 * c).abs() < 1e-5);
    }

    #[test]
    fn pan_automation_moves_the_sound() {
        let (mut project, track, _) = mixer_project();
        project.track_mut(track).unwrap().automation.pan =
            vec![fnoob_core::model::AutomationPoint {
                tick: 0,
                value: 1.0,
                curve: 0.0,
            }];
        let (frame, _) = first_frame(&project);
        assert!(frame[0].abs() < 1e-6);
        assert!((frame[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn audio_fades_ramp_in_and_out() {
        let (mut project, _, _) = mixer_project();
        let region = project.tracks[0].regions[0].id;
        fnoob_core::edit::set_region_fades(&mut project, region, 40, 40);
        let length = project.tracks[0].regions[0].length_tick;
        let pool = mixer_pool(&project);
        let (mut p, mut h) = create(SR, 2, audio_plan(&project, &pool));
        h.send(Command::Play).unwrap();
        let frames = (length as usize) * 25;
        let mut out = vec![0.0; 2 * frames];
        p.process(&mut out);
        let c = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(out[0], 0.0);
        assert!((out[2 * 500] - 0.25 * c).abs() < 1e-3);
        assert!((out[2 * 1500] - 0.5 * c).abs() < 1e-5);
    }

    #[test]
    fn live_notes_from_the_ui_are_played_and_recorded() {
        let (project, track, _) = synth_project();
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::SetLiveTrack(Some(track))).unwrap();
        h.send(Command::Record).unwrap();
        render(&mut p, 100);
        h.send(Command::LiveNote(MidiEvent {
            pitch: 67,
            velocity: 90,
        }))
        .unwrap();
        assert!(render(&mut p, 200).iter().any(|&s| s != 0.0));
        let events = recorded(&mut h);
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].sample, events[0].pitch), (100, 67));
    }

    #[test]
    fn audio_preview_plays_without_transport_and_stops() {
        let (mut p, mut h) = create(SR, 1, plan(&metronome_only()));
        let data = Arc::new(AudioData::from_samples(1, vec![0.5; 100]));
        h.send(Command::PreviewAudio(Some(Arc::clone(&data))))
            .unwrap();
        let out = render(&mut p, 200);
        assert!((out[10] - 0.35).abs() < 1e-6);
        assert_eq!(out[150], 0.0);
        h.send(Command::PreviewAudio(None)).unwrap();
        render(&mut p, 1);
        h.collect_garbage();
        assert_eq!(Arc::strong_count(&data), 1);
    }

    #[test]
    fn builtin_insert_runs_and_follows_new_params() {
        let (mut project, track, _) = mixer_project();
        let slot_id = project.alloc_id();
        let mut effect = fnoob_core::effects::BuiltinEffect::all()[5];
        project
            .track_mut(track)
            .unwrap()
            .strip
            .inserts
            .push(fnoob_core::model::PluginSlot::builtin(slot_id, effect));
        let pool = mixer_pool(&project);
        let mut map = crate::plan::PluginMap::new();
        map.insert(slot_id, 3);
        let plan = Box::new(RenderPlan::build(&project, SR, &pool, &map));
        let (mut p, mut h) = create(SR, 2, plan);
        h.send(Command::InstallBuiltin {
            slot: 3,
            effect: Box::new(BuiltinProcessor::new(effect, SR)),
        })
        .unwrap();
        if let fnoob_core::effects::BuiltinEffect::Utility(u) = &mut effect {
            u.gain_db = -60.0;
        }
        h.send(Command::SetBuiltinParams {
            slot: 3,
            params: effect,
        })
        .unwrap();
        h.send(Command::Play).unwrap();
        let mut out = vec![0.0; 2 * 64];
        p.process(&mut out);
        assert!(out.iter().all(|s| s.abs() < 0.001));
        h.send(Command::RemovePlugin(3)).unwrap();
        p.process(&mut out);
        h.collect_garbage();
    }

    #[test]
    fn device_automation_drives_builtin_params() {
        let (mut project, track, _) = mixer_project();
        let slot_id = project.alloc_id();
        let effect = fnoob_core::effects::BuiltinEffect::all()[5];
        project
            .track_mut(track)
            .unwrap()
            .strip
            .inserts
            .push(fnoob_core::model::PluginSlot::builtin(slot_id, effect));
        let render = |project: &Project| {
            let pool = mixer_pool(project);
            let mut map = crate::plan::PluginMap::new();
            map.insert(slot_id, 3);
            let plan = Box::new(RenderPlan::build(project, SR, &pool, &map));
            let (mut p, mut h) = create(SR, 2, plan);
            h.send(Command::InstallBuiltin {
                slot: 3,
                effect: Box::new(BuiltinProcessor::new(effect, SR)),
            })
            .unwrap();
            h.send(Command::Play).unwrap();
            let mut out = vec![0.0; 2 * 64];
            p.process(&mut out);
            h.send(Command::RemovePlugin(3)).unwrap();
            p.process(&mut vec![0.0; 2 * 64]);
            h.collect_garbage();
            out.iter().fold(0.0f32, |m, s| m.max(s.abs()))
        };
        assert!(render(&project) > 0.1);
        let param = fnoob_core::model::AutomationParam::Device(slot_id, 0);
        project
            .track_mut(track)
            .unwrap()
            .automation
            .lane_mut(param)
            .push(fnoob_core::model::AutomationPoint {
                tick: 0,
                value: 0.0,
                curve: 0.0,
            });
        assert!(render(&project) < 0.001);
        project.track_mut(track).unwrap().automation_off = true;
        assert!(render(&project) > 0.1);
    }

    #[test]
    fn tracks_route_into_audio_tracks() {
        let (mut project, track, _) = mixer_project();
        let bus = fnoob_core::edit::add_track(&mut project, TrackKind::Audio);
        project.track_mut(track).unwrap().output = fnoob_core::model::Output::Track(bus);
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let (frame, _) = first_frame(&project);
        assert_close(&frame, &[0.5 * c * c, 0.5 * c * c]);
        project.track_mut(bus).unwrap().strip.pan = -1.0;
        let (frame, _) = first_frame(&project);
        assert_close(&frame, &[0.5 * c, 0.0]);
        project.track_mut(bus).unwrap().mute = true;
        assert_eq!(first_frame(&project).0, [0.0, 0.0]);
        project.track_mut(bus).unwrap().mute = false;
        project.track_mut(bus).unwrap().output = fnoob_core::model::Output::Track(track);
        assert!(first_frame(&project).0.iter().all(|s| s.is_finite()));
        fnoob_core::edit::delete_track(&mut project, bus);
        assert_eq!(
            project.track(track).unwrap().output,
            fnoob_core::model::Output::Main
        );
    }

    #[test]
    fn sidechain_key_drives_the_gate() {
        let (mut project, track, _) = mixer_project();
        let key = fnoob_core::edit::add_track(&mut project, TrackKind::Audio);
        let slot_id = project.alloc_id();
        let effect = fnoob_core::effects::BuiltinEffect::all()[9];
        assert!(matches!(
            effect,
            fnoob_core::effects::BuiltinEffect::Gate(_)
        ));
        let mut slot = fnoob_core::model::PluginSlot::builtin(slot_id, effect);
        slot.sidechain = Some(key);
        project.track_mut(track).unwrap().strip.inserts.push(slot);
        let render = |project: &Project| {
            let pool = mixer_pool(project);
            let mut map = crate::plan::PluginMap::new();
            map.insert(slot_id, 3);
            let plan = Box::new(RenderPlan::build(project, SR, &pool, &map));
            let (mut p, mut h) = create(SR, 2, plan);
            h.send(Command::InstallBuiltin {
                slot: 3,
                effect: Box::new(BuiltinProcessor::new(effect, SR)),
            })
            .unwrap();
            h.send(Command::Play).unwrap();
            let mut out = vec![0.0; 2 * 1024];
            p.process(&mut out);
            h.send(Command::RemovePlugin(3)).unwrap();
            p.process(&mut vec![0.0; 2 * 64]);
            h.collect_garbage();
            out[out.len() - 1].abs()
        };
        assert!(render(&project) < 0.01);
        project.track_mut(track).unwrap().strip.inserts[0].sidechain = None;
        assert!(render(&project) > 0.1);
    }

    fn warped_project(data: &AudioData, mode: fnoob_core::model::WarpMode) -> Box<RenderPlan> {
        let (mut project, track, pool) = audio_project(data, 0);
        project.tempo_bpm = 120.0;
        let region = project.track(track).unwrap().regions[0].id;
        assert!(fnoob_core::edit::set_warp(
            &mut project,
            region,
            Some(fnoob_core::model::Warp { bpm: 60.0, mode })
        ));
        audio_plan(&project, &pool)
    }

    #[test]
    fn repitched_clips_follow_the_tempo() {
        let ramp: Vec<f32> = (0..48_000).map(|i| i as f32 / 48_000.0).collect();
        let data = AudioData::from_samples(1, ramp);
        let mut plan = warped_project(&data, fnoob_core::model::WarpMode::Repitch);
        let out = render_clips(&mut plan.audio_tracks[0], 0, 4000);
        for position in [100usize, 1000, 3000] {
            let expected = (2 * position) as f32 / 48_000.0;
            assert!((out[position * 2] - expected).abs() < 1e-3, "{position}");
        }
    }

    #[test]
    fn stretched_clips_keep_their_pitch() {
        let rate = SR as f32;
        let sine: Vec<f32> = (0..96_000)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate).sin() * 0.5)
            .collect();
        let data = AudioData::from_samples(1, sine);
        let mut plan = warped_project(&data, fnoob_core::model::WarpMode::Stretch);
        let out = render_clips(&mut plan.audio_tracks[0], 0, 24_000);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let crossings = left[4000..20_000]
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        let hz = crossings as f32 * rate / 16_000.0;
        assert!((hz - 440.0).abs() < 25.0, "{hz}");
        let peak = left[4000..20_000]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.3 && peak < 0.75, "{peak}");
    }

    #[test]
    fn overlapping_audio_clips_crossfade() {
        let data = AudioData::from_samples(1, vec![0.5; 4000]);
        let (mut project, track, pool) = audio_project(&data, 0);
        let file = project.audio_files[0].id;
        add_audio_region(&mut project, track, 40, file, 0).unwrap();
        let mut plan = audio_plan(&project, &pool);
        let out = render_clips(&mut plan.audio_tracks[0], 0, 6000);
        assert!((out[500 * 2] - 0.5).abs() < 1e-4);
        let middle = out[2500 * 2];
        assert!(
            (middle - 0.5 * std::f32::consts::SQRT_2).abs() < 1e-2,
            "{middle}"
        );
        assert!((out[4500 * 2] - 0.5).abs() < 1e-4);
        project.crossfade_overlaps = false;
        let mut plan = audio_plan(&project, &pool);
        let out = render_clips(&mut plan.audio_tracks[0], 0, 6000);
        assert!((out[2500 * 2] - 1.0).abs() < 1e-4);
        assert_eq!(
            fnoob_core::model::crossfades(project.track(track).unwrap()),
            vec![(40, 160)]
        );
        project.crossfade_overlaps = true;
        let second = project.track(track).unwrap().regions[1].id;
        fnoob_core::edit::set_region_fades(&mut project, second, 40, 0);
        assert_eq!(
            fnoob_core::model::crossfades(project.track(track).unwrap()),
            vec![(40, 80)]
        );
        let mut plan = audio_plan(&project, &pool);
        let out = render_clips(&mut plan.audio_tracks[0], 0, 6000);
        assert!((out[1500 * 2] - 0.5 * std::f32::consts::SQRT_2).abs() < 1e-2);
        assert!((out[3000 * 2] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn fade_curves_shape_the_gain() {
        let data = AudioData::from_samples(1, vec![1.0; 48_000]);
        let (mut project, track, pool) = audio_project(&data, 0);
        let region = project.track(track).unwrap().regions[0].id;
        fnoob_core::edit::set_region_fades(&mut project, region, 400, 0);
        let mut plan = audio_plan(&project, &pool);
        let linear = render_clips(&mut plan.audio_tracks[0], 0, 10_000)[5000 * 2];
        assert!((linear - 0.5).abs() < 1e-2, "{linear}");
        fnoob_core::edit::set_fade_curve(&mut project, region, true, 0.6);
        let mut plan = audio_plan(&project, &pool);
        let slow = render_clips(&mut plan.audio_tracks[0], 0, 10_000)[5000 * 2];
        assert!(slow < 0.3, "{slow}");
    }

    #[test]
    fn plugin_parameter_lanes_reach_the_plan() {
        let (mut project, track, _) = mixer_project();
        let slot_id = project.alloc_id();
        project
            .track_mut(track)
            .unwrap()
            .strip
            .inserts
            .push(fnoob_core::model::PluginSlot {
                id: slot_id,
                class_id: "0123456789ABCDEF0123456789ABCDEF".into(),
                name: "Verb".into(),
                path_hint: String::new(),
                bypass: false,
                component_state: String::new(),
                controller_state: None,
                builtin: None,
                sidechain: None,
            });
        project
            .track_mut(track)
            .unwrap()
            .automation
            .lane_mut(fnoob_core::model::AutomationParam::Device(slot_id, 7001))
            .push(fnoob_core::model::AutomationPoint {
                tick: 0,
                value: 0.25,
                curve: 0.0,
            });
        let pool = mixer_pool(&project);
        let mut map = crate::plan::PluginMap::new();
        map.insert(slot_id, 2);
        let plan = RenderPlan::build(&project, SR, &pool, &map);
        let device = &plan.audio_tracks[0].device_automation[0];
        assert_eq!((device.slot, device.base), (2, None));
        assert_eq!(device.lanes[0].0, 7001);
    }
}
