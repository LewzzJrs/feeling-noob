use crate::metronome::{Metronome, OneShot, beep};
use crate::plan::RenderPlan;
use fnoob_core::time::{beat_start_sample, first_beat_at_or_after};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

const COMMAND_CAPACITY: usize = 256;
const GARBAGE_CAPACITY: usize = 64;

#[derive(Debug)]
pub enum Command {
    Play,
    Stop,
    Record,
    Locate(u64),
    SetPlan(Box<RenderPlan>),
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
    garbage: Consumer<Box<RenderPlan>>,
    shared: Arc<Shared>,
}

impl EngineHandle {
    pub fn send(&mut self, command: Command) -> Result<(), Command> {
        self.commands.push(command).map_err(|PushError::Full(c)| c)
    }

    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
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
        self.shared.recording.load(Ordering::Relaxed)
    }
}

struct CountIn {
    elapsed: u64,
    length: u64,
    next_beat: u64,
    next_beat_sample: u64,
}

pub struct Processor {
    commands: Consumer<Command>,
    garbage: Producer<Box<RenderPlan>>,
    shared: Arc<Shared>,
    channels: usize,
    plan: Box<RenderPlan>,
    metronome: Metronome,
    beeper: OneShot,
    state: TransportState,
    recording: bool,
    count_in: CountIn,
    position: u64,
    next_beat: u64,
    next_beat_sample: u64,
    beep_cursor: usize,
}

pub fn create(sample_rate: u32, channels: u16, plan: Box<RenderPlan>) -> (Processor, EngineHandle) {
    let (command_tx, command_rx) = RingBuffer::new(COMMAND_CAPACITY);
    let (garbage_tx, garbage_rx) = RingBuffer::new(GARBAGE_CAPACITY);
    let shared = Arc::new(Shared::default());
    let mut processor = Processor {
        commands: command_rx,
        garbage: garbage_tx,
        shared: Arc::clone(&shared),
        channels: usize::from(channels),
        plan,
        metronome: Metronome::new(sample_rate),
        beeper: beep(sample_rate),
        state: TransportState::Stopped,
        recording: false,
        count_in: CountIn {
            elapsed: 0,
            length: 0,
            next_beat: 0,
            next_beat_sample: 0,
        },
        position: 0,
        next_beat: 0,
        next_beat_sample: 0,
        beep_cursor: 0,
    };
    processor.resync();
    let handle = EngineHandle {
        commands: command_tx,
        garbage: garbage_rx,
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
        self.beep_cursor = self.plan.beeps.partition_point(|&b| b < position);
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

    fn set_plan(&mut self, plan: Box<RenderPlan>) {
        if plan.samples_per_tick != self.plan.samples_per_tick {
            let tick = self.position as f64 / self.plan.samples_per_tick;
            self.position = (tick * plan.samples_per_tick).round() as u64;
        }
        let old = std::mem::replace(&mut self.plan, plan);
        let _ = self.garbage.push(old);
        self.resync();
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Play => self.start(false),
            Command::Record => self.start(true),
            Command::Stop => {
                self.state = TransportState::Stopped;
                self.recording = false;
                self.metronome.stop();
                self.beeper.stop();
            }
            Command::Locate(position) => {
                self.position = position;
                self.metronome.stop();
                self.beeper.stop();
                self.resync();
            }
            Command::SetPlan(plan) => self.set_plan(plan),
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.peek() {
            if matches!(command, Command::SetPlan(_)) && self.garbage.is_full() {
                break;
            }
            match self.commands.pop() {
                Ok(command) => self.apply(command),
                Err(_) => break,
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
            self.position = start;
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
        if self.plan.beeps.get(self.beep_cursor) == Some(&self.position) {
            self.beeper.trigger();
            self.beep_cursor += 1;
        }
        self.position += 1;
    }

    pub fn process(&mut self, out: &mut [f32]) {
        self.drain_commands();
        if self.channels == 0 {
            out.fill(0.0);
            return;
        }
        for frame in out.chunks_exact_mut(self.channels) {
            match self.state {
                TransportState::Stopped => {}
                TransportState::CountIn => self.tick_count_in(),
                TransportState::Playing => self.tick_playing(),
            }
            let sample = self.metronome.next_sample() + self.beeper.next_sample();
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            frame.fill(sample);
        }
        self.shared.position.store(self.position, Ordering::Relaxed);
        self.shared
            .state
            .store(self.state.to_u8(), Ordering::Relaxed);
        self.shared
            .recording
            .store(self.recording, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnoob_core::edit::{add_midi_region, add_track};
    use fnoob_core::model::{Project, TrackKind};

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
    fn beeps_at_region_starts() {
        let mut project = metronome_only();
        project.metronome.enabled = false;
        let track = add_track(&mut project, TrackKind::Instrument);
        add_midi_region(&mut project, track, 960, 960);
        add_midi_region(&mut project, track, 3840, 960);
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Play).unwrap();
        let out = render(&mut p, BEAT * 5);
        assert_eq!(onsets(&out), [BEAT, 4 * BEAT]);
    }

    #[test]
    fn loop_wraps_to_start() {
        let mut project = metronome_only();
        project.metronome.enabled = false;
        project.loop_range.enabled = true;
        project.loop_range.start_tick = 960;
        project.loop_range.end_tick = 1920;
        let track = add_track(&mut project, TrackKind::Instrument);
        add_midi_region(&mut project, track, 960, 960);
        let (mut p, mut h) = create(SR, 1, plan(&project));
        h.send(Command::Locate(BEAT as u64)).unwrap();
        h.send(Command::Play).unwrap();
        let out = render(&mut p, BEAT * 3);
        assert_eq!(onsets(&out), [0, BEAT, 2 * BEAT]);
        assert_eq!(h.position(), 2 * BEAT as u64);
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
