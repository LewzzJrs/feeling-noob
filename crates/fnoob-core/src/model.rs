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
    Vst3(PluginSlot),
}

pub const MAX_INSERTS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSlot {
    pub id: Id,
    pub class_id: String,
    pub name: String,
    pub path_hint: String,
    pub bypass: bool,
    pub component_state: String,
    pub controller_state: Option<String>,
}

impl PluginSlot {
    pub fn is_valid(&self) -> bool {
        self.class_id.len() == 32 && self.class_id.chars().all(|c| c.is_ascii_hexdigit())
    }
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

pub const MIN_GAIN_DB: f32 = -60.0;
pub const MAX_GAIN_DB: f32 = 24.0;
pub const SILENT_DB: f32 = -144.0;
pub const MAX_FADER_DB: f32 = 6.0;

pub fn db_to_gain(db: f32) -> f32 {
    if db <= SILENT_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

pub fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Send {
    pub level_db: f32,
    pub pre_fader: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelStrip {
    pub volume_db: f32,
    pub pan: f32,
    pub send: Send,
    pub inserts: Vec<PluginSlot>,
}

impl Default for ChannelStrip {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            send: Send {
                level_db: SILENT_DB,
                pre_fader: false,
            },
            inserts: Vec::new(),
        }
    }
}

impl ChannelStrip {
    pub fn is_valid(&self) -> bool {
        let level = SILENT_DB..=MAX_FADER_DB;
        level.contains(&self.volume_db)
            && level.contains(&self.send.level_db)
            && (-1.0..=1.0).contains(&self.pan)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bus {
    pub name: String,
    pub volume_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub inserts: Vec<PluginSlot>,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            name: "Bus 1".into(),
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            inserts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Master {
    pub volume_db: f32,
}

impl Default for Master {
    fn default() -> Self {
        Self { volume_db: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegionContent {
    Midi {
        notes: Vec<Note>,
    },
    Audio {
        file_id: Id,
        offset_frames: u64,
        gain_db: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioFile {
    pub id: Id,
    pub path: String,
    pub channels: u16,
    pub frames: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInput {
    pub first_channel: u16,
    pub channels: u16,
}

impl Default for AudioInput {
    fn default() -> Self {
        Self {
            first_channel: 0,
            channels: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: Id,
    pub name: String,
    pub start_tick: u64,
    pub length_tick: u64,
    pub muted: bool,
    #[serde(default)]
    pub fade_in_tick: u64,
    #[serde(default)]
    pub fade_out_tick: u64,
    pub content: RegionContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub tick: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationParam {
    #[default]
    Volume,
    Pan,
}

impl AutomationParam {
    pub const ALL: [AutomationParam; 2] = [AutomationParam::Volume, AutomationParam::Pan];

    pub fn label(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Pan => "Pan",
        }
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            Self::Volume => (-60.0, MAX_FADER_DB),
            Self::Pan => (-1.0, 1.0),
        }
    }

    pub fn is_valid(self, value: f32) -> bool {
        match self {
            Self::Volume => (SILENT_DB..=MAX_FADER_DB).contains(&value),
            Self::Pan => (-1.0..=1.0).contains(&value),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Automation {
    pub volume: Vec<AutomationPoint>,
    pub pan: Vec<AutomationPoint>,
}

impl Automation {
    pub fn lane(&self, param: AutomationParam) -> &[AutomationPoint] {
        match param {
            AutomationParam::Volume => &self.volume,
            AutomationParam::Pan => &self.pan,
        }
    }

    pub fn lane_mut(&mut self, param: AutomationParam) -> &mut Vec<AutomationPoint> {
        match param {
            AutomationParam::Volume => &mut self.volume,
            AutomationParam::Pan => &mut self.pan,
        }
    }
}

pub fn value_at(points: &[AutomationPoint], tick: u64, default: f32) -> f32 {
    let next = points.partition_point(|p| p.tick <= tick);
    match (next.checked_sub(1).map(|i| points[i]), points.get(next)) {
        (None, None) => default,
        (None, Some(first)) => first.value,
        (Some(last), None) => last.value,
        (Some(a), Some(b)) => {
            let span = (b.tick - a.tick) as f32;
            let t = if span > 0.0 {
                (tick - a.tick) as f32 / span
            } else {
                1.0
            };
            a.value + (b.value - a.value) * t
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub id: Id,
    pub tick: u64,
    pub name: String,
}

impl Region {
    pub fn end_tick(&self) -> u64 {
        self.start_tick + self.length_tick
    }

    pub fn notes(&self) -> &[Note] {
        match &self.content {
            RegionContent::Midi { notes } => notes,
            RegionContent::Audio { .. } => &[],
        }
    }

    pub fn notes_mut(&mut self) -> Option<&mut Vec<Note>> {
        match &mut self.content {
            RegionContent::Midi { notes } => Some(notes),
            RegionContent::Audio { .. } => None,
        }
    }

    pub fn audio_file(&self) -> Option<Id> {
        match self.content {
            RegionContent::Audio { file_id, .. } => Some(file_id),
            RegionContent::Midi { .. } => None,
        }
    }

    pub fn kind(&self) -> TrackKind {
        match self.content {
            RegionContent::Midi { .. } => TrackKind::Instrument,
            RegionContent::Audio { .. } => TrackKind::Audio,
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
    pub input_monitoring: bool,
    #[serde(default)]
    pub audio_input: Option<AudioInput>,
    #[serde(default)]
    pub instrument: Option<Instrument>,
    #[serde(default)]
    pub strip: ChannelStrip,
    #[serde(default)]
    pub automation: Automation,
    pub regions: Vec<Region>,
}

impl Track {
    pub fn plugin_slots(&self) -> impl Iterator<Item = &PluginSlot> {
        let instrument = match &self.instrument {
            Some(Instrument::Vst3(slot)) => Some(slot),
            _ => None,
        };
        instrument.into_iter().chain(self.strip.inserts.iter())
    }

    pub fn synth(&self) -> Option<SynthParams> {
        match (&self.kind, &self.instrument) {
            (TrackKind::Instrument, Some(Instrument::BasicSynth(params))) => Some(*params),
            (TrackKind::Instrument, Some(Instrument::Vst3(_))) => None,
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
    #[serde(default)]
    pub audio_files: Vec<AudioFile>,
    #[serde(default)]
    pub bus: Bus,
    #[serde(default)]
    pub master: Master,
    #[serde(default)]
    pub markers: Vec<Marker>,
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
            audio_files: Vec::new(),
            bus: Bus::default(),
            master: Master::default(),
            markers: Vec::new(),
            tracks: Vec::new(),
        }
    }

    pub fn alloc_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn audio_file(&self, id: Id) -> Option<&AudioFile> {
        self.audio_files.iter().find(|f| f.id == id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_law_is_minus_3_db_in_the_middle() {
        let (l, r) = pan_gains(0.0);
        assert!((l - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((l - r).abs() < 1e-6);
        let (l, r) = pan_gains(-1.0);
        assert!((l - 1.0).abs() < 1e-6 && r.abs() < 1e-6);
        let (l, r) = pan_gains(0.4);
        assert!((l * l + r * r - 1.0).abs() < 1e-5);
    }

    #[test]
    fn silent_db_is_zero_gain() {
        assert_eq!(db_to_gain(SILENT_DB), 0.0);
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(6.0) - 1.995).abs() < 0.01);
    }

    #[test]
    fn old_projects_get_default_mixer() {
        let mut value = serde_json::to_value(Project::new("t", 48_000)).unwrap();
        value.as_object_mut().unwrap().remove("bus");
        value.as_object_mut().unwrap().remove("master");
        let project: Project = serde_json::from_value(value).unwrap();
        assert_eq!(project.bus, Bus::default());
        assert_eq!(project.master, Master::default());
    }
}
