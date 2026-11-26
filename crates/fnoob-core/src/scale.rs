use serde::{Deserialize, Serialize};

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScaleKind {
    #[default]
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    HarmonicMinor,
    MelodicMinor,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
}

impl ScaleKind {
    pub const ALL: [Self; 12] = [
        Self::Major,
        Self::Minor,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Locrian,
        Self::HarmonicMinor,
        Self::MelodicMinor,
        Self::MajorPentatonic,
        Self::MinorPentatonic,
        Self::Blues,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Dorian => "Dorian",
            Self::Phrygian => "Phrygian",
            Self::Lydian => "Lydian",
            Self::Mixolydian => "Mixolydian",
            Self::Locrian => "Locrian",
            Self::HarmonicMinor => "Harmonic Minor",
            Self::MelodicMinor => "Melodic Minor",
            Self::MajorPentatonic => "Major Pentatonic",
            Self::MinorPentatonic => "Minor Pentatonic",
            Self::Blues => "Blues",
        }
    }

    pub fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Self::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Self::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Self::MajorPentatonic => &[0, 2, 4, 7, 9],
            Self::MinorPentatonic => &[0, 3, 5, 7, 10],
            Self::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Scale {
    pub enabled: bool,
    pub root: u8,
    pub kind: ScaleKind,
}

impl Scale {
    pub fn is_valid(&self) -> bool {
        self.root < 12
    }

    fn degree(&self, pitch: u8) -> u8 {
        (pitch % 12 + 12 - self.root % 12) % 12
    }

    pub fn contains(&self, pitch: u8) -> bool {
        self.kind.intervals().contains(&self.degree(pitch))
    }

    pub fn nearest(&self, pitch: u8) -> u8 {
        (0..12i16)
            .flat_map(|d| [-d, d])
            .map(|d| i16::from(pitch) + d)
            .filter(|p| (0..=127).contains(p))
            .map(|p| p as u8)
            .find(|p| self.contains(*p))
            .unwrap_or(pitch)
    }

    pub fn step(&self, pitch: u8, steps: i32) -> Option<u8> {
        let mut current = i32::from(self.nearest(pitch));
        let direction = steps.signum();
        for _ in 0..steps.unsigned_abs() {
            loop {
                current += direction;
                if !(0..=127).contains(&current) {
                    return None;
                }
                if self.contains(current as u8) {
                    break;
                }
            }
        }
        Some(current as u8)
    }

    pub fn is_root(&self, pitch: u8) -> bool {
        self.degree(pitch) == 0
    }

    pub fn label(&self) -> String {
        format!(
            "{} {}",
            NOTE_NAMES[usize::from(self.root % 12)],
            self.kind.label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_know_their_notes() {
        let c_major = Scale {
            enabled: true,
            root: 0,
            kind: ScaleKind::Major,
        };
        assert!(c_major.contains(60) && c_major.contains(64) && !c_major.contains(61));
        assert!(c_major.is_root(72) && !c_major.is_root(67));
        let a_minor = Scale {
            enabled: true,
            root: 9,
            kind: ScaleKind::Minor,
        };
        assert!(a_minor.contains(60) && a_minor.contains(69) && !a_minor.contains(68));
        assert_eq!(a_minor.label(), "A Minor");
        assert!(ScaleKind::ALL.iter().all(|k| k.intervals()[0] == 0));
        assert!(
            !Scale {
                root: 12,
                ..c_major
            }
            .is_valid()
        );
    }

    #[test]
    fn nearest_and_steps_stay_in_key() {
        let c_major = Scale {
            root: 0,
            kind: ScaleKind::Major,
            enabled: true,
        };
        assert_eq!(c_major.nearest(61), 60);
        assert_eq!(c_major.nearest(64), 64);
        assert_eq!(c_major.step(60, 1), Some(62));
        assert_eq!(c_major.step(64, 1), Some(65));
        assert_eq!(c_major.step(60, -1), Some(59));
        assert_eq!(c_major.step(60, 7), Some(72));
        assert_eq!(c_major.step(127, 3), None);
    }
}
