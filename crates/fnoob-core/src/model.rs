use serde::{Deserialize, Serialize};

pub type Id = u64;

pub const FORMAT_VERSION: u32 = 1;
pub const TRACK_COLORS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

impl TimeSignature {
    pub fn is_valid(self) -> bool {
        (1..=32).contains(&self.numerator) && matches!(self.denominator, 1 | 2 | 4 | 8 | 16 | 32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopRange {
    pub enabled: bool,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetronomeSettings {
    pub enabled: bool,
    pub count_in_enabled: bool,
    pub count_in_bars: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grid {
    Bar,
    Beat,
    Eighth,
    Sixteenth,
}

impl Grid {
    pub const ALL: [Grid; 4] = [Grid::Bar, Grid::Beat, Grid::Eighth, Grid::Sixteenth];

    pub fn label(self) -> &'static str {
        match self {
            Grid::Bar => "Bar",
            Grid::Beat => "Beat",
            Grid::Eighth => "1/8",
            Grid::Sixteenth => "1/16",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snap {
    pub enabled: bool,
    pub grid: Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Instrument,
}

impl TrackKind {
    pub fn default_name(self) -> &'static str {
        match self {
            TrackKind::Audio => "Audio",
            TrackKind::Instrument => "Inst",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Waveform {
    Sine,
    Saw,
    Square,
}

impl Waveform {
    pub const ALL: [Waveform; 3] = [Waveform::Sine, Waveform::Saw, Waveform::Square];

    pub fn label(self) -> &'static str {
        match self {
            Waveform::Sine => "Sine",
            Waveform::Saw => "Saw",
            Waveform::Square => "Square",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SynthParams {
    pub waveform: Waveform,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub level_db: f32,
}

pub const MAX_ENVELOPE_MS: f32 = 10_000.0;
pub const MIN_LEVEL_DB: f32 = -60.0;
pub const MAX_LEVEL_DB: f32 = 6.0;

impl SynthParams {
    pub fn is_valid(&self) -> bool {
        let time = 0.0..=MAX_ENVELOPE_MS;
        time.contains(&self.attack_ms)
            && time.contains(&self.decay_ms)
            && time.contains(&self.release_ms)
            && (0.0..=1.0).contains(&self.sustain)
            && (MIN_LEVEL_DB..=MAX_LEVEL_DB).contains(&self.level_db)
    }
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            attack_ms: 5.0,
            decay_ms: 100.0,
            sustain: 0.7,
            release_ms: 200.0,
            level_db: -12.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Instrument {
    BasicSynth(SynthParams),
}

impl Default for Instrument {
    fn default() -> Self {
        Self::BasicSynth(SynthParams::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub pitch: u8,
    pub velocity: u8,
    pub start_tick: u64,
    pub length_tick: u64,
}

impl Note {
    pub fn end_tick(&self) -> u64 {
        self.start_tick + self.length_tick
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionContent {
    Midi { notes: Vec<Note> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub id: Id,
    pub name: String,
    pub start_tick: u64,
    pub length_tick: u64,
    pub muted: bool,
    pub content: RegionContent,
}

impl Region {
    pub fn end_tick(&self) -> u64 {
        self.start_tick + self.length_tick
    }

    pub fn notes(&self) -> &[Note] {
        match &self.content {
            RegionContent::Midi { notes } => notes,
        }
    }

    pub fn notes_mut(&mut self) -> &mut Vec<Note> {
        match &mut self.content {
            RegionContent::Midi { notes } => notes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: Id,
    pub name: String,
    pub kind: TrackKind,
    pub color: u8,
    pub mute: bool,
    pub solo: bool,
    pub record_enabled: bool,
    #[serde(default)]
    pub instrument: Option<Instrument>,
    pub regions: Vec<Region>,
}

impl Track {
    pub fn synth(&self) -> Option<SynthParams> {
        match (&self.kind, &self.instrument) {
            (TrackKind::Instrument, Some(Instrument::BasicSynth(params))) => Some(*params),
            (TrackKind::Instrument, None) => Some(SynthParams::default()),
            (TrackKind::Audio, _) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub app_version: String,
    pub name: String,
    pub sample_rate: u32,
    pub tempo_bpm: f64,
    pub time_signature: TimeSignature,
    #[serde(rename = "loop")]
    pub loop_range: LoopRange,
    pub metronome: MetronomeSettings,
    pub snap: Snap,
    pub next_id: Id,
    pub tracks: Vec<Track>,
}

impl Project {
    pub fn new(name: impl Into<String>, sample_rate: u32) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            name: name.into(),
            sample_rate,
            tempo_bpm: 120.0,
            time_signature: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
            loop_range: LoopRange {
                enabled: false,
                start_tick: 0,
                end_tick: 4 * 3840,
            },
            metronome: MetronomeSettings {
                enabled: true,
                count_in_enabled: false,
                count_in_bars: 1,
            },
            snap: Snap {
                enabled: true,
                grid: Grid::Bar,
            },
            next_id: 1,
            tracks: Vec::new(),
        }
    }

    pub fn alloc_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn track(&self, id: Id) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: Id) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn find_region(&self, id: Id) -> Option<(&Track, &Region)> {
        self.tracks
            .iter()
            .find_map(|t| t.regions.iter().find(|r| r.id == id).map(|r| (t, r)))
    }

    pub fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    pub fn is_track_audible(&self, track: &Track) -> bool {
        !track.mute && (track.solo || !self.any_solo())
    }

    pub fn end_tick(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| t.regions.iter().map(Region::end_tick))
            .max()
            .unwrap_or(0)
    }
}
