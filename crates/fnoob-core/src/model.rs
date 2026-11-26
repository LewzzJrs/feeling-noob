use serde::{Deserialize, Serialize};

pub type Id = u64;

pub const FORMAT_VERSION: u32 = 1;
pub const TRACK_COLORS: u8 = 8;
pub const PALETTE_COLORS: u8 = 24;

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
pub struct TempoChange {
    pub tick: u64,
    pub bpm: f64,
    #[serde(default)]
    pub ramp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeterChange {
    pub bar: u64,
    pub signature: TimeSignature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Punch {
    pub punch_in: bool,
    pub punch_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetronomeSettings {
    pub enabled: bool,
    pub count_in_enabled: bool,
    pub count_in_bars: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grid {
    EightBars,
    FourBars,
    TwoBars,
    Bar,
    Half,
    Beat,
    Eighth,
    Sixteenth,
    ThirtySecond,
    HalfTriplet,
    BeatTriplet,
    EighthTriplet,
    SixteenthTriplet,
    ThirtySecondTriplet,
}

impl Grid {
    pub const ALL: [Grid; 9] = [
        Grid::EightBars,
        Grid::FourBars,
        Grid::TwoBars,
        Grid::Bar,
        Grid::Half,
        Grid::Beat,
        Grid::Eighth,
        Grid::Sixteenth,
        Grid::ThirtySecond,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Grid::EightBars => "8 Bars",
            Grid::FourBars => "4 Bars",
            Grid::TwoBars => "2 Bars",
            Grid::Bar => "Bar",
            Grid::Half => "1/2",
            Grid::Beat => "Beat",
            Grid::Eighth => "1/8",
            Grid::Sixteenth => "1/16",
            Grid::ThirtySecond => "1/32",
            Grid::HalfTriplet => "1/2T",
            Grid::BeatTriplet => "1/4T",
            Grid::EighthTriplet => "1/8T",
            Grid::SixteenthTriplet => "1/16T",
            Grid::ThirtySecondTriplet => "1/32T",
        }
    }

    pub const TRIPLETS: [Grid; 5] = [
        Grid::HalfTriplet,
        Grid::BeatTriplet,
        Grid::EighthTriplet,
        Grid::SixteenthTriplet,
        Grid::ThirtySecondTriplet,
    ];

    pub fn is_triplet(self) -> bool {
        Self::TRIPLETS.contains(&self)
    }

    pub fn toggled_triplet(self) -> Grid {
        match self {
            Grid::Half => Grid::HalfTriplet,
            Grid::Beat => Grid::BeatTriplet,
            Grid::Eighth => Grid::EighthTriplet,
            Grid::Sixteenth => Grid::SixteenthTriplet,
            Grid::ThirtySecond => Grid::ThirtySecondTriplet,
            Grid::HalfTriplet => Grid::Half,
            Grid::BeatTriplet => Grid::Beat,
            Grid::EighthTriplet => Grid::Eighth,
            Grid::SixteenthTriplet => Grid::Sixteenth,
            Grid::ThirtySecondTriplet => Grid::ThirtySecond,
            other => other,
        }
    }

    pub fn family(self) -> &'static [Grid] {
        if self.is_triplet() {
            &Self::TRIPLETS
        } else {
            &Self::ALL
        }
    }

    pub fn bars(self) -> Option<u64> {
        match self {
            Grid::EightBars => Some(8),
            Grid::FourBars => Some(4),
            Grid::TwoBars => Some(2),
            _ => None,
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
    Group,
}

impl TrackKind {
    pub fn default_name(self) -> &'static str {
        match self {
            TrackKind::Audio => "Audio",
            TrackKind::Instrument => "MIDI",
            TrackKind::Group => "Group",
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FmParams {
    pub ratio: f32,
    pub amount: f32,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub level_db: f32,
}

impl Default for FmParams {
    fn default() -> Self {
        Self {
            ratio: 2.0,
            amount: 2.5,
            attack_ms: 5.0,
            decay_ms: 300.0,
            sustain: 0.4,
            release_ms: 300.0,
            level_db: -12.0,
        }
    }
}

impl FmParams {
    pub fn is_valid(&self) -> bool {
        let time = 0.0..=MAX_ENVELOPE_MS;
        (0.25..=16.0).contains(&self.ratio)
            && (0.0..=10.0).contains(&self.amount)
            && time.contains(&self.attack_ms)
            && time.contains(&self.decay_ms)
            && time.contains(&self.release_ms)
            && (0.0..=1.0).contains(&self.sustain)
            && (MIN_LEVEL_DB..=MAX_LEVEL_DB).contains(&self.level_db)
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SamplerParams {
    pub file: Option<Id>,
    pub root_pitch: u8,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub level_db: f32,
}

impl SamplerParams {
    pub fn is_valid(&self) -> bool {
        let time = 0.0..=MAX_ENVELOPE_MS;
        self.root_pitch <= 127
            && time.contains(&self.attack_ms)
            && time.contains(&self.decay_ms)
            && time.contains(&self.release_ms)
            && (0.0..=1.0).contains(&self.sustain)
            && (MIN_LEVEL_DB..=MAX_LEVEL_DB).contains(&self.level_db)
    }
}

impl Default for SamplerParams {
    fn default() -> Self {
        Self {
            file: None,
            root_pitch: 60,
            attack_ms: 1.0,
            decay_ms: 200.0,
            sustain: 1.0,
            release_ms: 200.0,
            level_db: -6.0,
        }
    }
}

pub const DRUM_PADS: usize = 16;
pub const DRUM_BASE_PITCH: u8 = 36;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DrumPad {
    pub file: Option<Id>,
    pub gain_db: f32,
    pub pan: f32,
}

impl Default for DrumPad {
    fn default() -> Self {
        Self {
            file: None,
            gain_db: 0.0,
            pan: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DrumParams {
    pub pads: [DrumPad; DRUM_PADS],
    pub level_db: f32,
}

impl DrumParams {
    pub fn is_valid(&self) -> bool {
        (MIN_LEVEL_DB..=MAX_LEVEL_DB).contains(&self.level_db)
            && self.pads.iter().all(|p| {
                (MIN_LEVEL_DB..=MAX_LEVEL_DB).contains(&p.gain_db) && (-1.0..=1.0).contains(&p.pan)
            })
    }

    pub fn pad_for(pitch: u8) -> Option<usize> {
        let index = usize::from(pitch.checked_sub(DRUM_BASE_PITCH)?);
        (index < DRUM_PADS).then_some(index)
    }
}

impl Default for DrumParams {
    fn default() -> Self {
        Self {
            pads: [DrumPad::default(); DRUM_PADS],
            level_db: -6.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Instrument {
    BasicSynth(SynthParams),
    Vst3(PluginSlot),
    Sampler(SamplerParams),
    DrumSampler(Box<DrumParams>),
    FmSynth(FmParams),
}

impl Instrument {
    pub fn label(&self) -> &str {
        match self {
            Self::BasicSynth(_) => "Basic Synth",
            Self::Vst3(slot) => &slot.name,
            Self::Sampler(_) => "Sampler",
            Self::DrumSampler(_) => "Drum Sampler",
            Self::FmSynth(_) => "FM Synth",
        }
    }

    pub fn sample_files(&self) -> Vec<Id> {
        match self {
            Self::Sampler(p) => p.file.into_iter().collect(),
            Self::DrumSampler(p) => p.pads.iter().filter_map(|pad| pad.file).collect(),
            Self::BasicSynth(_) | Self::Vst3(_) | Self::FmSynth(_) => Vec::new(),
        }
    }
}

pub const MAX_INSERTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginSlot {
    pub id: Id,
    pub class_id: String,
    pub name: String,
    pub path_hint: String,
    pub bypass: bool,
    pub component_state: String,
    pub controller_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<crate::effects::BuiltinEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<Id>,
}

pub const BUILTIN_CLASS: &str = "builtin";

impl PluginSlot {
    pub fn builtin(id: Id, effect: crate::effects::BuiltinEffect) -> Self {
        Self {
            id,
            class_id: BUILTIN_CLASS.to_owned(),
            name: effect.label().to_owned(),
            path_hint: String::new(),
            bypass: false,
            component_state: String::new(),
            controller_state: None,
            builtin: Some(effect),
            sidechain: None,
        }
    }

    pub fn takes_sidechain(&self) -> bool {
        matches!(
            self.builtin,
            Some(
                crate::effects::BuiltinEffect::Compressor(_)
                    | crate::effects::BuiltinEffect::Gate(_)
            )
        )
    }

    pub fn is_valid(&self) -> bool {
        match &self.builtin {
            Some(effect) => self.class_id == BUILTIN_CLASS && effect.is_valid(),
            None => {
                self.class_id.len() == 32 && self.class_id.chars().all(|c| c.is_ascii_hexdigit())
            }
        }
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
    #[serde(default = "full_chance", skip_serializing_if = "is_full_chance")]
    pub chance: u8,
}

pub const FULL_CHANCE: u8 = 100;

fn full_chance() -> u8 {
    FULL_CHANCE
}

fn is_full_chance(chance: &u8) -> bool {
    *chance >= FULL_CHANCE
}

impl Note {
    pub fn end_tick(&self) -> u64 {
        self.start_tick + self.length_tick
    }
}

pub const MIN_GAIN_DB: f32 = -60.0;
pub const MAX_GAIN_DB: f32 = 24.0;
pub const SILENT_DB: f32 = -144.0;
pub const DEFAULT_LANE_HEIGHT: f32 = 60.0;
pub const MIN_LANE_HEIGHT: f32 = 24.0;
pub const MAX_LANE_HEIGHT: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuantizeSettings {
    pub strength: u8,
    pub swing: u8,
    pub ends: bool,
}

impl Default for QuantizeSettings {
    fn default() -> Self {
        Self {
            strength: 100,
            swing: 0,
            ends: false,
        }
    }
}

impl QuantizeSettings {
    pub fn target(self, tick: u64, grid: u64) -> u64 {
        if grid == 0 {
            return tick;
        }
        let snapped = crate::time::snap_round(tick, grid);
        let swung = if (snapped / grid) % 2 == 1 {
            snapped + grid * u64::from(self.swing.min(100)) / 200
        } else {
            snapped
        };
        let strength = i64::from(self.strength.min(100));
        let delta = swung as i64 - tick as i64;
        (tick as i64 + delta * strength / 100).max(0) as u64
    }
}

fn crossfade_default() -> bool {
    true
}

pub fn crossfades(track: &Track) -> Vec<(u64, u64)> {
    let mut audio: Vec<&Region> = track
        .regions
        .iter()
        .filter(|r| !r.muted && matches!(r.content, RegionContent::Audio { .. }))
        .collect();
    audio.sort_by_key(|r| r.start_tick);
    audio
        .windows(2)
        .filter(|w| w[1].start_tick < w[0].end_tick() && w[1].end_tick() > w[0].end_tick())
        .map(|w| {
            let overlap = w[0].end_tick() - w[1].start_tick;
            let length = match w[1].fade_in_tick {
                0 => overlap,
                fade => fade.min(overlap),
            };
            (w[1].start_tick, w[1].start_tick + length)
        })
        .collect()
}

fn default_lane_height() -> f32 {
    DEFAULT_LANE_HEIGHT
}

fn is_default_lane_height(height: &f32) -> bool {
    *height == DEFAULT_LANE_HEIGHT
}
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

pub const SILENT_SEND: Send = Send {
    level_db: SILENT_DB,
    pre_fader: false,
};

pub const MAX_RETURNS: usize = 8;

pub fn return_letter(index: usize) -> char {
    char::from(b'A' + (index.min(25)) as u8)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelStrip {
    pub volume_db: f32,
    pub pan: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send: Option<Send>,
    pub sends: Vec<Send>,
    pub inserts: Vec<PluginSlot>,
}

impl Default for ChannelStrip {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            send: None,
            sends: Vec::new(),
            inserts: Vec::new(),
        }
    }
}

impl ChannelStrip {
    pub fn is_valid(&self) -> bool {
        let level = SILENT_DB..=MAX_FADER_DB;
        level.contains(&self.volume_db)
            && self.sends.len() <= MAX_RETURNS
            && self.sends.iter().all(|s| level.contains(&s.level_db))
            && (-1.0..=1.0).contains(&self.pan)
    }

    pub fn send_at(&self, index: usize) -> Send {
        self.sends.get(index).copied().unwrap_or(SILENT_SEND)
    }

    pub fn send_mut(&mut self, index: usize) -> &mut Send {
        if self.sends.len() <= index {
            self.sends.resize(index + 1, SILENT_SEND);
        }
        &mut self.sends[index]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Monitor {
    In,
    #[default]
    Auto,
    Off,
}

impl Monitor {
    pub const ALL: [Self; 3] = [Self::In, Self::Auto, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::In => "In",
            Self::Auto => "Auto",
            Self::Off => "Off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Output {
    #[default]
    Main,
    None,
    Track(Id),
}

impl Output {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "Main",
            Self::None => "No Output",
            Self::Track(_) => "Track",
        }
    }
}

pub const RETURN_NAME: &str = "A Return";
pub const OLD_RETURN_NAME: &str = "Bus 1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReturnTrack {
    pub name: String,
    pub volume_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub inserts: Vec<PluginSlot>,
}

impl Default for ReturnTrack {
    fn default() -> Self {
        Self {
            name: RETURN_NAME.into(),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_ticks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warp: Option<Warp>,
    #[serde(default, skip_serializing_if = "is_linear")]
    pub fade_in_curve: f32,
    #[serde(default, skip_serializing_if = "is_linear")]
    pub fade_out_curve: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warp_markers: Vec<WarpMarker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WarpMarker {
    pub frame: u64,
    pub tick: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WarpMap {
    points: Vec<(f64, f64)>,
    frames_per_tick: f64,
}

impl WarpMap {
    pub fn new(warp: Warp, markers: &[WarpMarker], sample_rate: u32) -> Self {
        let mut points = vec![(0.0, 0.0)];
        let mut sorted = markers.to_vec();
        sorted.sort_by_key(|m| m.frame);
        for marker in sorted {
            let last = points[points.len() - 1];
            if marker.frame as f64 > last.1 && marker.tick > last.0 {
                points.push((marker.tick, marker.frame as f64));
            }
        }
        Self {
            points,
            frames_per_tick: warp.frames_per_tick(sample_rate),
        }
    }

    pub fn frame(&self, tick: f64) -> f64 {
        let next = self.points.partition_point(|p| p.0 <= tick);
        match (
            next.checked_sub(1).map(|i| self.points[i]),
            self.points.get(next),
        ) {
            (Some(a), Some(b)) => a.1 + (tick - a.0) / (b.0 - a.0) * (b.1 - a.1),
            (Some(a), None) => a.1 + (tick - a.0) * self.frames_per_tick,
            (None, _) => tick * self.frames_per_tick,
        }
    }

    pub fn tick(&self, frame: f64) -> f64 {
        let next = self.points.partition_point(|p| p.1 <= frame);
        match (
            next.checked_sub(1).map(|i| self.points[i]),
            self.points.get(next),
        ) {
            (Some(a), Some(b)) => a.0 + (frame - a.1) / (b.1 - a.1) * (b.0 - a.0),
            (Some(a), None) => a.0 + (frame - a.1) / self.frames_per_tick,
            (None, _) => frame / self.frames_per_tick,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WarpMode {
    #[default]
    Stretch,
    Beats,
    Repitch,
}

impl WarpMode {
    pub const ALL: [WarpMode; 3] = [WarpMode::Stretch, WarpMode::Beats, WarpMode::Repitch];

    pub fn label(self) -> &'static str {
        match self {
            Self::Stretch => "Stretch",
            Self::Beats => "Beats",
            Self::Repitch => "Repitch",
        }
    }
}

pub const MIN_WARP_BPM: f64 = 20.0;
pub const MAX_WARP_BPM: f64 = 999.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Warp {
    pub bpm: f64,
    #[serde(default)]
    pub mode: WarpMode,
}

impl Warp {
    pub fn frames_per_tick(self, sample_rate: u32) -> f64 {
        f64::from(sample_rate) * 60.0
            / self.bpm.clamp(MIN_WARP_BPM, MAX_WARP_BPM)
            / crate::time::PPQ as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub tick: u64,
    pub value: f32,
    #[serde(default, skip_serializing_if = "is_linear")]
    pub curve: f32,
}

fn is_linear(curve: &f32) -> bool {
    *curve == 0.0
}

pub fn shape(t: f32, curve: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if curve >= 0.0 {
        t.powf(1.0 + 4.0 * curve)
    } else {
        1.0 - (1.0 - t).powf(1.0 - 4.0 * curve)
    }
}

const SEND_LABELS: [&str; 8] = [
    "Send A", "Send B", "Send C", "Send D", "Send E", "Send F", "Send G", "Send H",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutomationParam {
    #[default]
    Volume,
    Pan,
    Send(u8),
    Device(Id, u32),
}

impl AutomationParam {
    pub const ALL: [AutomationParam; 2] = [AutomationParam::Volume, AutomationParam::Pan];

    pub fn with_sends(returns: usize) -> Vec<AutomationParam> {
        let mut all = Self::ALL.to_vec();
        all.extend((0..returns.min(MAX_RETURNS) as u8).map(Self::Send));
        all
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Pan => "Pan",
            Self::Send(i) => SEND_LABELS.get(usize::from(i)).copied().unwrap_or("Send"),
            Self::Device(..) => "Device",
        }
    }

    pub fn range(self) -> (f32, f32) {
        match self {
            Self::Volume | Self::Send(_) => (-60.0, MAX_FADER_DB),
            Self::Pan => (-1.0, 1.0),
            Self::Device(..) => (0.0, 1.0),
        }
    }

    pub fn is_valid(self, value: f32) -> bool {
        match self {
            Self::Volume | Self::Send(_) => (SILENT_DB..=MAX_FADER_DB).contains(&value),
            Self::Pan => (-1.0..=1.0).contains(&value),
            Self::Device(..) => (0.0..=1.0).contains(&value),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Automation {
    pub volume: Vec<AutomationPoint>,
    pub pan: Vec<AutomationPoint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<Vec<AutomationPoint>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<DeviceLane>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceLane {
    pub slot: Id,
    pub param: u32,
    pub points: Vec<AutomationPoint>,
}

impl Automation {
    pub fn lane(&self, param: AutomationParam) -> &[AutomationPoint] {
        match param {
            AutomationParam::Volume => &self.volume,
            AutomationParam::Pan => &self.pan,
            AutomationParam::Send(i) => self.sends.get(usize::from(i)).map_or(&[], Vec::as_slice),
            AutomationParam::Device(slot, param) => self
                .devices
                .iter()
                .find(|l| l.slot == slot && l.param == param)
                .map_or(&[], |l| l.points.as_slice()),
        }
    }

    pub fn lane_mut(&mut self, param: AutomationParam) -> &mut Vec<AutomationPoint> {
        match param {
            AutomationParam::Volume => &mut self.volume,
            AutomationParam::Pan => &mut self.pan,
            AutomationParam::Send(i) => {
                let i = usize::from(i);
                if self.sends.len() <= i {
                    self.sends.resize(i + 1, Vec::new());
                }
                &mut self.sends[i]
            }
            AutomationParam::Device(slot, param) => {
                let index = match self
                    .devices
                    .iter()
                    .position(|l| l.slot == slot && l.param == param)
                {
                    Some(index) => index,
                    None => {
                        self.devices.push(DeviceLane {
                            slot,
                            param,
                            points: Vec::new(),
                        });
                        self.devices.len() - 1
                    }
                };
                &mut self.devices[index].points
            }
        }
    }

    pub fn lanes(&self) -> impl Iterator<Item = (AutomationParam, &[AutomationPoint])> {
        [
            (AutomationParam::Volume, self.volume.as_slice()),
            (AutomationParam::Pan, self.pan.as_slice()),
        ]
        .into_iter()
        .chain(
            self.sends
                .iter()
                .enumerate()
                .map(|(i, lane)| (AutomationParam::Send(i as u8), lane.as_slice())),
        )
        .chain(self.devices.iter().map(|l| {
            (
                AutomationParam::Device(l.slot, l.param),
                l.points.as_slice(),
            )
        }))
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
            a.value + (b.value - a.value) * shape(t, a.curve)
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

    pub fn audio_offset(&self) -> u64 {
        match self.content {
            RegionContent::Audio { offset_frames, .. } => offset_frames,
            RegionContent::Midi { .. } => 0,
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
    #[serde(
        default = "default_lane_height",
        skip_serializing_if = "is_default_lane_height"
    )]
    pub lane_height: f32,
    pub kind: TrackKind,
    pub color: u8,
    pub mute: bool,
    pub solo: bool,
    pub record_enabled: bool,
    #[serde(default, skip_serializing)]
    pub input_monitoring: bool,
    #[serde(default)]
    pub monitor: Monitor,
    #[serde(default)]
    pub output: Output,
    #[serde(default)]
    pub audio_input: Option<AudioInput>,
    #[serde(default)]
    pub instrument: Option<Instrument>,
    #[serde(default)]
    pub strip: ChannelStrip,
    #[serde(default)]
    pub automation: Automation,
    pub regions: Vec<Region>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session: Vec<Option<crate::session::SessionClip>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<crate::takes::TakeLane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Id>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen: Option<Id>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub automation_off: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_stop: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi_effects: Vec<crate::midi_fx::MidiDevice>,
}

impl Track {
    pub fn all_regions(&self) -> impl Iterator<Item = &Region> {
        self.regions
            .iter()
            .chain(self.session.iter().flatten().map(|c| &c.region))
            .chain(self.takes.iter().flat_map(|l| l.regions.iter()))
    }

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
            (
                TrackKind::Instrument,
                Some(
                    Instrument::Vst3(_)
                    | Instrument::Sampler(_)
                    | Instrument::DrumSampler(_)
                    | Instrument::FmSynth(_),
                ),
            ) => None,
            (TrackKind::Instrument, None) => Some(SynthParams::default()),
            (TrackKind::Audio | TrackKind::Group, _) => None,
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
    #[serde(default)]
    pub punch: Punch,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tempo_changes: Vec<TempoChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meter_changes: Vec<MeterChange>,
    #[serde(default)]
    pub scale: crate::scale::Scale,
    #[serde(default)]
    pub quantize: QuantizeSettings,
    pub metronome: MetronomeSettings,
    pub snap: Snap,
    pub next_id: Id,
    #[serde(default)]
    pub audio_files: Vec<AudioFile>,
    #[serde(default, skip_serializing)]
    pub bus: Option<ReturnTrack>,
    #[serde(default)]
    pub returns: Vec<ReturnTrack>,
    #[serde(default)]
    pub master: Master,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default = "crate::session::default_scenes")]
    pub scenes: Vec<crate::session::Scene>,
    #[serde(default)]
    pub launch_quantize: crate::session::LaunchQuantize,
    #[serde(default)]
    pub midi_overdub: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi_map: Vec<crate::midi_map::MidiMapping>,
    #[serde(default = "crossfade_default")]
    pub crossfade_overlaps: bool,
    pub tracks: Vec<Track>,
}

impl Project {
    pub fn migrate(&mut self) {
        if let Some(mut old) = self.bus.take()
            && self.returns.is_empty()
        {
            if old.name == OLD_RETURN_NAME {
                old.name = RETURN_NAME.to_owned();
            }
            self.returns.push(old);
        }
        for track in &mut self.tracks {
            if let Some(send) = track.strip.send.take()
                && track.strip.sends.is_empty()
            {
                track.strip.sends.push(send);
            }
            if std::mem::take(&mut track.input_monitoring) {
                track.monitor = Monitor::In;
            }
        }
    }

    pub fn timeline(&self, sample_rate: u32) -> crate::time::Timeline {
        crate::time::Timeline::with_ramps(
            sample_rate,
            self.tempo_bpm,
            self.tempo_changes.iter().map(|c| (c.tick, c.bpm, c.ramp)),
        )
    }

    pub fn meter(&self) -> crate::time::MeterMap {
        crate::time::MeterMap::with_changes(
            self.time_signature,
            self.meter_changes
                .iter()
                .map(|c| (c.bar.saturating_sub(1), c.signature)),
        )
    }

    pub fn monitors(&self, track: &Track) -> bool {
        match track.monitor {
            Monitor::In => true,
            Monitor::Auto => track.record_enabled,
            Monitor::Off => false,
        }
    }

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
            punch: Punch::default(),
            tempo_changes: Vec::new(),
            meter_changes: Vec::new(),
            scale: crate::scale::Scale::default(),
            quantize: QuantizeSettings::default(),
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
            bus: None,
            returns: Vec::new(),
            master: Master::default(),
            markers: Vec::new(),
            scenes: crate::session::default_scenes(),
            launch_quantize: crate::session::LaunchQuantize::default(),
            midi_overdub: false,
            midi_map: Vec::new(),
            crossfade_overlaps: true,
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
            .find_map(|t| t.all_regions().find(|r| r.id == id).map(|r| (t, r)))
    }

    pub fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    pub fn ancestors(&self, track: &Track) -> Vec<&Track> {
        let mut found = Vec::new();
        let mut next = track.group;
        while let Some(id) = next {
            match self.track(id) {
                Some(parent) if !found.iter().any(|t: &&Track| t.id == parent.id) => {
                    next = parent.group;
                    found.push(parent);
                }
                _ => break,
            }
        }
        found
    }

    pub fn depth(&self, track: &Track) -> usize {
        self.ancestors(track).len()
    }

    pub fn is_inside(&self, track: &Track, group: Id) -> bool {
        self.ancestors(track).iter().any(|g| g.id == group)
    }

    pub fn is_track_audible(&self, track: &Track) -> bool {
        let ancestors = self.ancestors(track);
        if track.mute || ancestors.iter().any(|g| g.mute) {
            return false;
        }
        !self.any_solo()
            || track.solo
            || ancestors.iter().any(|g| g.solo)
            || (track.kind == TrackKind::Group
                && self
                    .tracks
                    .iter()
                    .any(|t| t.solo && self.is_inside(t, track.id)))
    }

    pub fn group_members(&self, group: Id) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(move |t| t.group == Some(group))
    }

    pub fn routes_to(&self, from: Id, to: Id) -> bool {
        let mut next = Some(from);
        let mut steps = 0;
        while let Some(id) = next {
            if id == to {
                return true;
            }
            steps += 1;
            if steps > self.tracks.len() {
                return false;
            }
            next = match self.track(id).map(|t| t.output) {
                Some(Output::Track(target)) => Some(target),
                _ => None,
            };
        }
        false
    }

    pub fn output_targets(&self, track: Id) -> Vec<Id> {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio && t.id != track && !self.routes_to(t.id, track))
            .map(|t| t.id)
            .collect()
    }

    pub fn sidechain_sources(&self, track: Id) -> Vec<Id> {
        self.tracks
            .iter()
            .filter(|t| t.kind != TrackKind::Group && t.id != track)
            .map(|t| t.id)
            .collect()
    }

    pub fn is_folded_away(&self, track: &Track) -> bool {
        self.ancestors(track).iter().any(|g| g.folded)
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
    fn automation_curves_bend_the_line() {
        let point = |tick, value, curve| AutomationPoint { tick, value, curve };
        let straight = [point(0, 0.0, 0.0), point(100, 1.0, 0.0)];
        let slow = [point(0, 0.0, 0.6), point(100, 1.0, 0.0)];
        let fast = [point(0, 0.0, -0.6), point(100, 1.0, 0.0)];
        assert!((value_at(&straight, 50, 0.0) - 0.5).abs() < 1e-6);
        assert!(value_at(&slow, 50, 0.0) < 0.3);
        assert!(value_at(&fast, 50, 0.0) > 0.7);
        assert_eq!(value_at(&slow, 100, 0.0), 1.0);
        let mut automation = Automation::default();
        automation
            .lane_mut(AutomationParam::Send(1))
            .push(point(0, -6.0, 0.0));
        assert_eq!(automation.lane(AutomationParam::Send(1)).len(), 1);
        assert!(automation.lane(AutomationParam::Send(0)).is_empty());
        assert_eq!(automation.lanes().count(), 4);
    }

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
        value.as_object_mut().unwrap().remove("returns");
        value.as_object_mut().unwrap().remove("master");
        let project: Project = serde_json::from_value(value).unwrap();
        assert!(project.returns.is_empty());
        assert_eq!(project.master, Master::default());
    }
}
