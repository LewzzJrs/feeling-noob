use crate::model::Grid;
use crate::scale::ScaleKind;
use serde::{Deserialize, Serialize};

pub const MAX_MIDI_EFFECTS: usize = 4;
pub const MAX_CHORD_SHIFTS: usize = 5;
pub const MAX_OUT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArpStyle {
    Up,
    Down,
    UpDown,
    Random,
}

impl ArpStyle {
    pub const ALL: [ArpStyle; 4] = [
        ArpStyle::Up,
        ArpStyle::Down,
        ArpStyle::UpDown,
        ArpStyle::Random,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ArpStyle::Up => "Up",
            ArpStyle::Down => "Down",
            ArpStyle::UpDown => "Up/Down",
            ArpStyle::Random => "Random",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MidiEffect {
    Chord {
        shifts: [i8; MAX_CHORD_SHIFTS],
    },
    Scale {
        root: u8,
        kind: ScaleKind,
    },
    Arpeggiator {
        style: ArpStyle,
        rate: Grid,
        octaves: u8,
        gate: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MidiDevice {
    pub effect: MidiEffect,
    #[serde(default)]
    pub bypass: bool,
}

impl MidiEffect {
    pub fn all() -> [MidiEffect; 3] {
        [
            MidiEffect::Chord {
                shifts: [4, 7, 0, 0, 0],
            },
            MidiEffect::Scale {
                root: 0,
                kind: ScaleKind::Major,
            },
            MidiEffect::Arpeggiator {
                style: ArpStyle::Up,
                rate: Grid::Sixteenth,
                octaves: 1,
                gate: 0.6,
            },
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            MidiEffect::Chord { .. } => "Chord",
            MidiEffect::Scale { .. } => "Scale",
            MidiEffect::Arpeggiator { .. } => "Arpeggiator",
        }
    }

    pub fn is_valid(&self) -> bool {
        match self {
            MidiEffect::Chord { shifts } => shifts.iter().all(|s| (-36..=36).contains(s)),
            MidiEffect::Scale { root, .. } => *root < 12,
            MidiEffect::Arpeggiator { octaves, gate, .. } => {
                (1..=4).contains(octaves) && (0.05..=1.0).contains(gate)
            }
        }
    }
}

fn snap_to_scale(pitch: u8, root: u8, kind: ScaleKind) -> u8 {
    let intervals = kind.intervals();
    for down in 0..12u8 {
        let Some(candidate) = pitch.checked_sub(down) else {
            break;
        };
        let degree = (candidate % 12 + 12 - root % 12) % 12;
        if intervals.contains(&degree) {
            return candidate;
        }
    }
    pitch
}

pub fn pitches(effects: &[MidiDevice], pitch: u8, out: &mut [u8; MAX_OUT]) -> usize {
    out[0] = pitch;
    let mut count = 1;
    for device in effects.iter().filter(|d| !d.bypass) {
        match device.effect {
            MidiEffect::Scale { root, kind } => {
                for p in out.iter_mut().take(count) {
                    *p = snap_to_scale(*p, root, kind);
                }
            }
            MidiEffect::Chord { shifts } => {
                let base = count;
                for i in 0..base {
                    for shift in shifts.iter().filter(|s| **s != 0) {
                        let shifted = i16::from(out[i]) + i16::from(*shift);
                        if count < MAX_OUT && (0..=127).contains(&shifted) {
                            let shifted = shifted as u8;
                            if !out[..count].contains(&shifted) {
                                out[count] = shifted;
                                count += 1;
                            }
                        }
                    }
                }
            }
            MidiEffect::Arpeggiator { .. } => {}
        }
    }
    count
}

pub fn arpeggiator(effects: &[MidiDevice]) -> Option<(ArpStyle, Grid, u8, f32)> {
    effects
        .iter()
        .filter(|d| !d.bypass)
        .find_map(|d| match d.effect {
            MidiEffect::Arpeggiator {
                style,
                rate,
                octaves,
                gate,
            } => Some((style, rate, octaves, gate)),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(effect: MidiEffect) -> MidiDevice {
        MidiDevice {
            effect,
            bypass: false,
        }
    }

    #[test]
    fn chord_adds_shifted_notes() {
        let mut out = [0; MAX_OUT];
        let fx = [device(MidiEffect::all()[0])];
        let n = pitches(&fx, 60, &mut out);
        assert_eq!(&out[..n], &[60, 64, 67]);
    }

    #[test]
    fn scale_snaps_down_then_chord_builds_on_it() {
        let mut out = [0; MAX_OUT];
        let fx = [device(MidiEffect::all()[1]), device(MidiEffect::all()[0])];
        let n = pitches(&fx, 61, &mut out);
        assert_eq!(&out[..n], &[60, 64, 67]);
        let bypassed = [MidiDevice {
            effect: MidiEffect::all()[1],
            bypass: true,
        }];
        assert_eq!(pitches(&bypassed, 61, &mut out), 1);
        assert_eq!(out[0], 61);
    }

    #[test]
    fn arpeggiator_is_found_and_defaults_are_valid() {
        let fx: Vec<MidiDevice> = MidiEffect::all().into_iter().map(device).collect();
        assert!(MidiEffect::all().iter().all(MidiEffect::is_valid));
        assert_eq!(arpeggiator(&fx).map(|a| a.0), Some(ArpStyle::Up));
        assert!(arpeggiator(&fx[..2]).is_none());
    }
}
