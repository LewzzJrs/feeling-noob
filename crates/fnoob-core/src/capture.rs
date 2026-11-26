use crate::model::Note;
use crate::time::{PPQ, Timeline};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    pub clock: u64,
    pub position: Option<u64>,
    pub pitch: u8,
    pub velocity: u8,
}

pub fn last_phrase(events: &[Played], gap: u64) -> &[Played] {
    let mut held = [false; 128];
    let mut start = 0;
    let mut previous: Option<u64> = None;
    for (index, event) in events.iter().enumerate() {
        let idle = held.iter().all(|h| !h);
        if idle && previous.is_some_and(|p| event.clock.saturating_sub(p) > gap) {
            start = index;
        }
        held[usize::from(event.pitch.min(127))] = event.velocity > 0;
        previous = Some(event.clock);
    }
    &events[start..]
}

pub fn notes(phrase: &[Played], timeline: Timeline, stopped_at_tick: u64) -> Vec<Note> {
    let Some(first) = phrase.first() else {
        return Vec::new();
    };
    let on_transport = phrase.iter().all(|e| e.position.is_some());
    let per_tick = timeline.samples_per_tick();
    let tick_of = |event: &Played| match (on_transport, event.position) {
        (true, Some(position)) => timeline.sample_to_tick(position),
        _ => stopped_at_tick + ((event.clock - first.clock) as f64 / per_tick).round() as u64,
    };
    let last_tick = phrase.iter().map(tick_of).max().unwrap_or(0);
    let mut open: [Option<(u64, u8)>; 128] = [None; 128];
    let mut result = Vec::new();
    let close = |pitch: u8, start: u64, velocity: u8, end: u64, result: &mut Vec<Note>| {
        if end > start {
            result.push(Note {
                pitch,
                velocity: velocity.clamp(1, 127),
                start_tick: start,
                length_tick: end - start,
                chance: 100,
            });
        }
    };
    for event in phrase {
        let tick = tick_of(event);
        let index = usize::from(event.pitch.min(127));
        if let Some((start, velocity)) = open[index].take() {
            close(event.pitch, start, velocity, tick, &mut result);
        }
        if event.velocity > 0 {
            open[index] = Some((tick, event.velocity));
        }
    }
    for (pitch, slot) in open.iter().enumerate() {
        if let Some((start, velocity)) = *slot {
            let end = last_tick.max(start + PPQ / 4);
            close(pitch as u8, start, velocity, end, &mut result);
        }
    }
    result.sort_by_key(|n| (n.start_tick, n.pitch));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const TL: Timeline = Timeline::new(48_000, 120.0);

    fn played(clock: u64, position: Option<u64>, pitch: u8, velocity: u8) -> Played {
        Played {
            clock,
            position,
            pitch,
            velocity,
        }
    }

    #[test]
    fn phrase_starts_after_the_last_long_silence() {
        let events = [
            played(0, None, 60, 100),
            played(100, None, 60, 0),
            played(500_000, None, 62, 100),
            played(510_000, None, 64, 100),
            played(520_000, None, 62, 0),
            played(800_000, None, 64, 0),
            played(820_000, None, 65, 100),
        ];
        assert_eq!(last_phrase(&events, 192_000).len(), 5);
        assert_eq!(last_phrase(&events, 1_000_000).len(), 7);
    }

    #[test]
    fn stopped_notes_start_at_the_given_tick() {
        let events = [
            played(1000, None, 60, 100),
            played(1000 + 24_000, None, 60, 0),
            played(1000 + 24_000, None, 67, 90),
        ];
        let notes = notes(&events, TL, 3840);
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].start_tick, notes[0].length_tick), (3840, 960));
        assert_eq!((notes[1].pitch, notes[1].start_tick), (67, 4800));
        assert_eq!(notes[1].length_tick, PPQ / 4);
    }

    #[test]
    fn playing_notes_keep_their_transport_positions() {
        let events = [
            played(5, Some(48_000), 60, 100),
            played(9, Some(60_000), 60, 0),
        ];
        let notes = notes(&events, TL, 0);
        assert_eq!(notes.len(), 1);
        assert_eq!((notes[0].start_tick, notes[0].length_tick), (1920, 480));
    }
}
