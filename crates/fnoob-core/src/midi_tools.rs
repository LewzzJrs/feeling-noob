use crate::model::Note;
use crate::scale::Scale;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpStyle {
    Up,
    Down,
    UpDown,
}

impl ArpStyle {
    pub const ALL: [ArpStyle; 3] = [ArpStyle::Up, ArpStyle::Down, ArpStyle::UpDown];

    pub fn label(self) -> &'static str {
        match self {
            ArpStyle::Up => "Up",
            ArpStyle::Down => "Down",
            ArpStyle::UpDown => "Up and Down",
        }
    }
}

pub const CHORDS: [(&str, &[i8]); 6] = [
    ("Major", &[4, 7]),
    ("Minor", &[3, 7]),
    ("Sus4", &[5, 7]),
    ("Seventh", &[4, 7, 10]),
    ("Power", &[7]),
    ("Octave", &[12]),
];

fn targets(notes: &[Note], picked: &[usize]) -> Vec<usize> {
    if picked.is_empty() {
        (0..notes.len()).collect()
    } else {
        picked
            .iter()
            .copied()
            .filter(|&i| i < notes.len())
            .collect()
    }
}

fn spread(seed: u64, index: usize, salt: u64) -> f32 {
    let mut x = seed ^ salt ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    (x >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
}

fn shifted(pitch: u8, by: i32) -> Option<u8> {
    u8::try_from(i32::from(pitch) + by)
        .ok()
        .filter(|p| *p <= 127)
}

fn sort(notes: &mut [Note]) {
    notes.sort_by_key(|n| (n.start_tick, n.pitch));
}

pub fn transpose(notes: &mut [Note], picked: &[usize], semitones: i32) {
    for i in targets(notes, picked) {
        if let Some(pitch) = shifted(notes[i].pitch, semitones) {
            notes[i].pitch = pitch;
        }
    }
}

pub fn euclid_pattern(steps: u8, pulses: u8, rotate: u8) -> Vec<bool> {
    let steps = usize::from(steps.clamp(1, 64));
    let pulses = usize::from(pulses).min(steps);
    let pattern: Vec<bool> = (0..steps).map(|i| (i * pulses) % steps < pulses).collect();
    let shift = usize::from(rotate) % steps;
    (0..steps)
        .map(|i| pattern[(i + steps - shift) % steps])
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Euclid {
    pub steps: u8,
    pub pulses: u8,
    pub rotate: u8,
    pub step_ticks: u64,
    pub pitch: u8,
    pub velocity: u8,
}

pub fn euclid(notes: &mut Vec<Note>, clip_length: u64, settings: Euclid) {
    let pattern = euclid_pattern(settings.steps, settings.pulses, settings.rotate);
    let step = settings.step_ticks.max(1);
    let mut tick = 0;
    let mut index = 0;
    while tick < clip_length {
        if pattern[index % pattern.len()]
            && !notes
                .iter()
                .any(|n| n.pitch == settings.pitch && n.start_tick == tick)
        {
            notes.push(Note {
                pitch: settings.pitch,
                velocity: settings.velocity.clamp(1, 127),
                start_tick: tick,
                length_tick: (step / 2).max(1).min(clip_length - tick),
                chance: crate::model::FULL_CHANCE,
            });
        }
        tick += step;
        index += 1;
    }
    notes.sort_by_key(|n| (n.start_tick, n.pitch));
}

pub fn chop(notes: &mut Vec<Note>, picked: &[usize], parts: u8) {
    let parts = u64::from(parts.max(2));
    let chosen = targets(notes, picked);
    let mut added = Vec::new();
    for &i in &chosen {
        let note = notes[i];
        let piece = note.length_tick / parts;
        if piece == 0 {
            continue;
        }
        notes[i].length_tick = piece;
        for k in 1..parts {
            let last = k == parts - 1;
            added.push(Note {
                start_tick: note.start_tick + piece * k,
                length_tick: if last {
                    note.length_tick - piece * k
                } else {
                    piece
                },
                ..note
            });
        }
    }
    notes.extend(added);
    notes.sort_by_key(|n| (n.start_tick, n.pitch));
}

pub fn flam(notes: &mut Vec<Note>, picked: &[usize], gap: u64) {
    let gap = gap.max(1);
    let mut added = Vec::new();
    for i in targets(notes, picked) {
        let note = notes[i];
        if note.start_tick < gap {
            continue;
        }
        added.push(Note {
            start_tick: note.start_tick - gap,
            length_tick: gap,
            velocity: (note.velocity / 2).max(1),
            ..note
        });
    }
    notes.extend(added);
    notes.sort_by_key(|n| (n.start_tick, n.pitch));
}

pub fn fit_to_scale(notes: &mut [Note], picked: &[usize], scale: crate::scale::Scale) {
    for i in targets(notes, picked) {
        notes[i].pitch = scale.nearest(notes[i].pitch);
    }
}

pub fn transpose_in_scale(
    notes: &mut [Note],
    picked: &[usize],
    scale: crate::scale::Scale,
    steps: i32,
) {
    for i in targets(notes, picked) {
        if let Some(pitch) = scale.step(notes[i].pitch, steps) {
            notes[i].pitch = pitch;
        }
    }
}

pub fn humanize(notes: &mut [Note], picked: &[usize], timing: u64, velocity: u8, seed: u64) {
    for i in targets(notes, picked) {
        let note = &mut notes[i];
        let shift = (spread(seed, i, 1) * timing as f32).round() as i64;
        note.start_tick = (note.start_tick as i64 + shift).max(0) as u64;
        let change = (spread(seed, i, 2) * f32::from(velocity)).round() as i32;
        note.velocity = (i32::from(note.velocity) + change).clamp(1, 127) as u8;
    }
    sort(notes);
}

pub fn legato(notes: &mut [Note], picked: &[usize], clip_end: u64) {
    let chosen = targets(notes, picked);
    let mut starts: Vec<u64> = chosen.iter().map(|&i| notes[i].start_tick).collect();
    starts.sort_unstable();
    starts.dedup();
    for i in chosen {
        let start = notes[i].start_tick;
        let next = starts
            .iter()
            .copied()
            .find(|s| *s > start)
            .unwrap_or(clip_end);
        if next > start {
            notes[i].length_tick = next - start;
        }
    }
}

fn chords(notes: &[Note], chosen: &[usize]) -> Vec<Vec<usize>> {
    let mut groups: Vec<(u64, Vec<usize>)> = Vec::new();
    for &i in chosen {
        match groups.iter_mut().find(|(s, _)| *s == notes[i].start_tick) {
            Some((_, members)) => members.push(i),
            None => groups.push((notes[i].start_tick, vec![i])),
        }
    }
    groups
        .into_iter()
        .map(|(_, mut members)| {
            members.sort_by_key(|&i| notes[i].pitch);
            members
        })
        .collect()
}

pub fn strum(notes: &mut [Note], picked: &[usize], step: u64, down: bool) {
    let chosen = targets(notes, picked);
    for members in chords(notes, &chosen) {
        let order: Vec<usize> = if down {
            members.iter().rev().copied().collect()
        } else {
            members
        };
        for (k, i) in order.into_iter().enumerate() {
            let delay = step * k as u64;
            let note = &mut notes[i];
            note.start_tick += delay;
            note.length_tick = note.length_tick.saturating_sub(delay).max(1);
        }
    }
    sort(notes);
}

pub fn arpeggiate(notes: &mut Vec<Note>, picked: &[usize], rate: u64, style: ArpStyle) {
    let rate = rate.max(1);
    let chosen = targets(notes, picked);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for members in chords(notes, &chosen) {
        if members.len() < 2 {
            continue;
        }
        let start = notes[members[0]].start_tick;
        let end = members
            .iter()
            .map(|&i| notes[i].end_tick())
            .max()
            .unwrap_or(start + rate);
        let up: Vec<Note> = members.iter().map(|&i| notes[i]).collect();
        let pattern: Vec<Note> = match style {
            ArpStyle::Up => up.clone(),
            ArpStyle::Down => up.iter().rev().copied().collect(),
            ArpStyle::UpDown => {
                let mut cycle = up.clone();
                cycle.extend(up.iter().rev().skip(1).take(up.len().saturating_sub(2)));
                cycle
            }
        };
        let mut at = start;
        let mut k = 0;
        while at < end {
            let source = pattern[k % pattern.len()];
            added.push(Note {
                start_tick: at,
                length_tick: rate.min(end - at),
                ..source
            });
            at += rate;
            k += 1;
        }
        removed.extend(members);
    }
    removed.sort_unstable();
    for i in removed.into_iter().rev() {
        notes.remove(i);
    }
    notes.extend(added);
    sort(notes);
}

pub fn add_chord(notes: &mut Vec<Note>, picked: &[usize], intervals: &[i8]) {
    let mut added = Vec::new();
    for i in targets(notes, picked) {
        for interval in intervals {
            if let Some(pitch) = shifted(notes[i].pitch, i32::from(*interval)) {
                added.push(Note { pitch, ..notes[i] });
            }
        }
    }
    notes.extend(added);
    sort(notes);
    notes.dedup_by(|a, b| a.start_tick == b.start_tick && a.pitch == b.pitch);
}

pub fn add_key_chord(notes: &mut Vec<Note>, picked: &[usize], scale: Scale) {
    let mut added = Vec::new();
    for i in targets(notes, picked) {
        let mut found = Vec::new();
        let mut pitch = notes[i].pitch;
        while found.len() < 4 && pitch < 127 {
            pitch += 1;
            if scale.contains(pitch) {
                found.push(pitch);
            }
        }
        for step in [1, 3] {
            if let Some(&p) = found.get(step) {
                added.push(Note {
                    pitch: p,
                    ..notes[i]
                });
            }
        }
    }
    notes.extend(added);
    sort(notes);
    notes.dedup_by(|a, b| a.start_tick == b.start_tick && a.pitch == b.pitch);
}

pub fn set_chance(notes: &mut [Note], picked: &[usize], chance: u8) {
    for i in targets(notes, picked) {
        notes[i].chance = chance.min(100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;

    fn note(pitch: u8, start: u64, length: u64) -> Note {
        Note {
            pitch,
            velocity: 100,
            start_tick: start,
            length_tick: length,
            chance: 100,
        }
    }

    #[test]
    fn legato_reaches_the_next_note() {
        let mut notes = vec![note(60, 0, 10), note(64, 0, 10), note(67, 480, 10)];
        legato(&mut notes, &[], 960);
        let lengths: Vec<u64> = notes.iter().map(|n| n.length_tick).collect();
        assert_eq!(lengths, vec![480, 480, 480]);
    }

    #[test]
    fn strum_spreads_a_chord() {
        let mut notes = vec![note(60, 0, 480), note(64, 0, 480), note(67, 0, 480)];
        strum(&mut notes, &[], 20, false);
        let starts: Vec<(u8, u64)> = notes.iter().map(|n| (n.pitch, n.start_tick)).collect();
        assert_eq!(starts, vec![(60, 0), (64, 20), (67, 40)]);
        let mut notes = vec![note(60, 0, 480), note(64, 0, 480)];
        strum(&mut notes, &[], 20, true);
        assert_eq!(notes[0].pitch, 64);
    }

    #[test]
    fn arpeggio_turns_a_chord_into_steps() {
        let mut notes = vec![note(60, 0, 480), note(64, 0, 480), note(67, 0, 480)];
        arpeggiate(&mut notes, &[], 120, ArpStyle::UpDown);
        let pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![60, 64, 67, 64]);
        assert!(notes.iter().all(|n| n.length_tick == 120));
    }

    #[test]
    fn chords_add_notes_above() {
        let mut notes = vec![note(60, 0, 480)];
        add_chord(&mut notes, &[], &[4, 7]);
        let pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![60, 64, 67]);
        let mut notes = vec![note(62, 0, 480)];
        let c_major = Scale {
            enabled: true,
            root: 0,
            kind: ScaleKind::Major,
        };
        add_key_chord(&mut notes, &[], c_major);
        let pitches: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![62, 65, 69]);
    }

    #[test]
    fn humanize_stays_in_range() {
        let mut notes = vec![note(60, 0, 100), note(62, 480, 100)];
        humanize(&mut notes, &[], 20, 30, 9);
        assert!(notes.iter().all(|n| n.start_tick <= 500));
        assert!(notes.iter().all(|n| (70..=127).contains(&n.velocity)));
        transpose(&mut notes, &[0], 12);
        assert!(notes.iter().any(|n| n.pitch >= 72));
    }

    #[test]
    fn euclid_spreads_pulses_evenly() {
        let pattern = euclid_pattern(8, 3, 0);
        assert_eq!(pattern.iter().filter(|p| **p).count(), 3);
        assert_eq!(
            pattern,
            [true, false, false, true, false, false, true, false]
        );
        assert_eq!(euclid_pattern(4, 1, 1), [false, true, false, false]);
        let mut notes = Vec::new();
        let settings = Euclid {
            steps: 4,
            pulses: 2,
            rotate: 0,
            step_ticks: 240,
            pitch: 36,
            velocity: 100,
        };
        euclid(&mut notes, 1920, settings);
        assert_eq!(notes.len(), 4);
        euclid(&mut notes, 1920, settings);
        assert_eq!(notes.len(), 4);
    }

    #[test]
    fn chop_and_flam() {
        let mut notes = vec![note(60, 480, 400)];
        chop(&mut notes, &[], 3);
        let starts: Vec<(u64, u64)> = notes
            .iter()
            .map(|n| (n.start_tick, n.length_tick))
            .collect();
        assert_eq!(starts, [(480, 133), (613, 133), (746, 134)]);
        let mut notes = vec![note(38, 480, 120), note(38, 10, 120)];
        flam(&mut notes, &[], 30);
        assert_eq!(notes.len(), 3);
        assert_eq!((notes[1].start_tick, notes[1].velocity), (450, 50));
    }
}
