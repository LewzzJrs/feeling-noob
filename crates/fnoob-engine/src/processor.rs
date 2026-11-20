use crate::metronome::Metronome;
use fnoob_core::time::{beat_start_sample, first_beat_at_or_after, samples_per_beat};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const COMMAND_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Stop,
    Locate(u64),
    SetTempo(f64),
    SetMetronome(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportSettings {
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub metronome: bool,
}

#[derive(Default)]
struct Shared {
    position: AtomicU64,
    playing: AtomicBool,
}

pub struct EngineHandle {
    commands: Producer<Command>,
    shared: Arc<Shared>,
}

impl EngineHandle {
    pub fn send(&mut self, command: Command) -> Result<(), Command> {
        self.commands.push(command).map_err(|PushError::Full(c)| c)
    }

    pub fn position(&self) -> u64 {
        self.shared.position.load(Ordering::Relaxed)
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }
}

pub struct Processor {
    commands: Consumer<Command>,
    shared: Arc<Shared>,
    channels: usize,
    sample_rate: u32,
    samples_per_beat: f64,
    beats_per_bar: u64,
    metronome_enabled: bool,
    metronome: Metronome,
    playing: bool,
    position: u64,
    next_beat: u64,
    next_beat_sample: u64,
}

pub fn create(
    sample_rate: u32,
    channels: u16,
    settings: TransportSettings,
) -> (Processor, EngineHandle) {
    let (producer, consumer) = RingBuffer::new(COMMAND_CAPACITY);
    let shared = Arc::new(Shared::default());
    let mut processor = Processor {
        commands: consumer,
        shared: Arc::clone(&shared),
        channels: usize::from(channels),
        sample_rate,
        samples_per_beat: samples_per_beat(sample_rate, settings.bpm),
        beats_per_bar: u64::from(settings.beats_per_bar.max(1)),
        metronome_enabled: settings.metronome,
        metronome: Metronome::new(sample_rate),
        playing: false,
        position: 0,
        next_beat: 0,
        next_beat_sample: 0,
    };
    processor.locate(0);
    let handle = EngineHandle {
        commands: producer,
        shared,
    };
    (processor, handle)
}

impl Processor {
    fn locate(&mut self, position: u64) {
        self.position = position;
        self.next_beat = first_beat_at_or_after(position, self.samples_per_beat);
        self.next_beat_sample = beat_start_sample(self.next_beat, self.samples_per_beat);
    }

    fn set_tempo(&mut self, bpm: f64) {
        let beats = self.position as f64 / self.samples_per_beat;
        self.samples_per_beat = samples_per_beat(self.sample_rate, bpm);
        let position = (beats * self.samples_per_beat).round() as u64;
        self.locate(position);
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Play => self.playing = true,
            Command::Stop => {
                self.playing = false;
                self.metronome.stop();
            }
            Command::Locate(position) => {
                self.metronome.stop();
                self.locate(position);
            }
            Command::SetTempo(bpm) => self.set_tempo(bpm),
            Command::SetMetronome(on) => {
                self.metronome_enabled = on;
                if !on {
                    self.metronome.stop();
                }
            }
        }
    }

    pub fn process(&mut self, out: &mut [f32]) {
        while let Ok(command) = self.commands.pop() {
            self.apply(command);
        }
        if self.channels == 0 {
            out.fill(0.0);
            return;
        }
        for frame in out.chunks_exact_mut(self.channels) {
            if self.playing {
                if self.position == self.next_beat_sample {
                    if self.metronome_enabled {
                        self.metronome
                            .trigger(self.next_beat.is_multiple_of(self.beats_per_bar));
                    }
                    self.next_beat += 1;
                    self.next_beat_sample =
                        beat_start_sample(self.next_beat, self.samples_per_beat);
                }
                self.position += 1;
            }
            let sample = self.metronome.next_sample();
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            frame.fill(sample);
        }
        self.shared.position.store(self.position, Ordering::Relaxed);
        self.shared.playing.store(self.playing, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn settings() -> TransportSettings {
        TransportSettings {
            bpm: 120.0,
            beats_per_bar: 4,
            metronome: true,
        }
    }

    fn render(processor: &mut Processor, frames: usize, channels: usize) -> Vec<f32> {
        let mut out = vec![1.0; frames * channels];
        processor.process(&mut out);
        out
    }

    fn onsets(mono: &[f32]) -> Vec<usize> {
        let mut result = Vec::new();
        let mut silent = true;
        for (i, s) in mono.iter().enumerate() {
            if silent && *s != 0.0 {
                result.push(i);
                silent = false;
            } else if *s == 0.0 && i + 1 < mono.len() && mono[i + 1] == 0.0 {
                silent = true;
            }
        }
        result
    }

    #[test]
    fn stopped_engine_outputs_silence() {
        let (mut p, _h) = create(SR, 2, settings());
        assert!(render(&mut p, 512, 2).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn clicks_land_on_beats() {
        let (mut p, mut h) = create(SR, 1, settings());
        h.send(Command::Play).unwrap();
        let out = render(&mut p, 24_000 * 4 + 100, 1);
        let first_onset_sample =
            |start: usize| start + out[start..].iter().position(|s| *s != 0.0).unwrap();
        assert_eq!(first_onset_sample(0), 1);
        assert_eq!(out[0], 0.0);
        let found = onsets(&out);
        assert_eq!(found.len(), 5);
        for (beat, &onset) in found.iter().enumerate() {
            assert!(
                (onset as i64 - (beat as i64 * 24_000)).abs() <= 1,
                "beat {beat} at {onset}"
            );
        }
        assert_eq!(h.position(), 24_000 * 4 + 100);
        assert!(h.is_playing());
    }

    #[test]
    fn accent_differs_from_normal_click() {
        let (mut p, mut h) = create(SR, 1, settings());
        h.send(Command::Play).unwrap();
        let out = render(&mut p, 24_000 * 2, 1);
        assert_ne!(out[1..200], out[24_001..24_200]);
    }

    #[test]
    fn metronome_off_is_silent_but_transport_moves() {
        let (mut p, mut h) = create(SR, 2, settings());
        h.send(Command::SetMetronome(false)).unwrap();
        h.send(Command::Play).unwrap();
        assert!(render(&mut p, 50_000, 2).iter().all(|&s| s == 0.0));
        assert_eq!(h.position(), 50_000);
    }

    #[test]
    fn stop_and_locate() {
        let (mut p, mut h) = create(SR, 2, settings());
        h.send(Command::Play).unwrap();
        render(&mut p, 1000, 2);
        h.send(Command::Stop).unwrap();
        render(&mut p, 1000, 2);
        assert_eq!(h.position(), 1000);
        assert!(!h.is_playing());
        h.send(Command::Locate(0)).unwrap();
        render(&mut p, 10, 2);
        assert_eq!(h.position(), 0);
    }

    #[test]
    fn tempo_change_keeps_musical_position() {
        let (mut p, mut h) = create(SR, 1, settings());
        h.send(Command::Locate(48_000)).unwrap();
        h.send(Command::SetTempo(60.0)).unwrap();
        render(&mut p, 1, 1);
        assert_eq!(h.position(), 96_000);
    }

    #[test]
    fn all_channels_get_same_signal() {
        let (mut p, mut h) = create(SR, 2, settings());
        h.send(Command::Play).unwrap();
        let out = render(&mut p, 2000, 2);
        assert!(out.as_chunks::<2>().0.iter().all(|f| f[0] == f[1]));
        assert!(out.iter().any(|&s| s != 0.0));
    }

    #[test]
    fn full_command_queue_is_reported() {
        let (_p, mut h) = create(SR, 2, settings());
        for _ in 0..COMMAND_CAPACITY {
            h.send(Command::Play).unwrap();
        }
        assert_eq!(h.send(Command::Stop), Err(Command::Stop));
    }
}
