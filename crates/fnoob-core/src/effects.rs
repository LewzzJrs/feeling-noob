use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EqParams {
    pub low_cut_hz: f32,
    pub low_gain_db: f32,
    pub low_hz: f32,
    pub mid_gain_db: f32,
    pub mid_hz: f32,
    pub mid_q: f32,
    pub high_gain_db: f32,
    pub high_hz: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CompressorParams {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelaySync {
    Free,
    Quarter,
    DottedEighth,
    Eighth,
    Sixteenth,
}

impl DelaySync {
    pub const ALL: [DelaySync; 5] = [
        DelaySync::Free,
        DelaySync::Quarter,
        DelaySync::DottedEighth,
        DelaySync::Eighth,
        DelaySync::Sixteenth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "ms",
            Self::Quarter => "1/4",
            Self::DottedEighth => "1/8 dotted",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
        }
    }

    pub fn beats(self) -> Option<f64> {
        match self {
            Self::Free => None,
            Self::Quarter => Some(1.0),
            Self::DottedEighth => Some(0.75),
            Self::Eighth => Some(0.5),
            Self::Sixteenth => Some(0.25),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DelayParams {
    pub sync: DelaySync,
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub ping_pong: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReverbParams {
    pub size: f32,
    pub damping: f32,
    pub width: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LimiterParams {
    pub gain_db: f32,
    pub ceiling_db: f32,
    pub release_ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UtilityParams {
    pub gain_db: f32,
    pub width: f32,
    pub mono: bool,
    pub invert_left: bool,
    pub invert_right: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMode {
    LowPass,
    HighPass,
    BandPass,
}

impl FilterMode {
    pub const ALL: [FilterMode; 3] = [
        FilterMode::LowPass,
        FilterMode::HighPass,
        FilterMode::BandPass,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FilterMode::LowPass => "Low-pass",
            FilterMode::HighPass => "High-pass",
            FilterMode::BandPass => "Band-pass",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoFilterParams {
    pub mode: FilterMode,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub lfo_hz: f32,
    pub lfo_depth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChorusParams {
    pub rate_hz: f32,
    pub depth_ms: f32,
    pub delay_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaturatorCurve {
    Soft,
    Hard,
    Warm,
}

impl SaturatorCurve {
    pub const ALL: [SaturatorCurve; 3] = [
        SaturatorCurve::Soft,
        SaturatorCurve::Hard,
        SaturatorCurve::Warm,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SaturatorCurve::Soft => "Soft",
            SaturatorCurve::Hard => "Hard",
            SaturatorCurve::Warm => "Warm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SaturatorParams {
    pub curve: SaturatorCurve,
    pub drive_db: f32,
    pub output_db: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GateParams {
    pub threshold_db: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    pub floor_db: f32,
}

pub const MAX_CHORUS_MS: f32 = 50.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BuiltinEffect {
    Eq(EqParams),
    Compressor(CompressorParams),
    Delay(DelayParams),
    Reverb(ReverbParams),
    Limiter(LimiterParams),
    Utility(UtilityParams),
    AutoFilter(AutoFilterParams),
    Chorus(ChorusParams),
    Saturator(SaturatorParams),
    Gate(GateParams),
}

pub const MAX_DELAY_MS: f32 = 2000.0;

fn within(value: f32, low: f32, high: f32) -> bool {
    (low..=high).contains(&value)
}

impl BuiltinEffect {
    pub fn all() -> [BuiltinEffect; 10] {
        [
            Self::Eq(EqParams {
                low_cut_hz: 0.0,
                low_gain_db: 0.0,
                low_hz: 120.0,
                mid_gain_db: 0.0,
                mid_hz: 1000.0,
                mid_q: 0.8,
                high_gain_db: 0.0,
                high_hz: 8000.0,
            }),
            Self::Compressor(CompressorParams {
                threshold_db: -18.0,
                ratio: 4.0,
                attack_ms: 10.0,
                release_ms: 120.0,
                makeup_db: 0.0,
            }),
            Self::Delay(DelayParams {
                sync: DelaySync::Eighth,
                time_ms: 350.0,
                feedback: 0.35,
                mix: 0.3,
                ping_pong: false,
            }),
            Self::Reverb(ReverbParams {
                size: 0.7,
                damping: 0.4,
                width: 1.0,
                mix: 0.25,
            }),
            Self::Limiter(LimiterParams {
                gain_db: 0.0,
                ceiling_db: -0.3,
                release_ms: 80.0,
            }),
            Self::Utility(UtilityParams {
                gain_db: 0.0,
                width: 1.0,
                mono: false,
                invert_left: false,
                invert_right: false,
            }),
            Self::AutoFilter(AutoFilterParams {
                mode: FilterMode::LowPass,
                cutoff_hz: 1200.0,
                resonance: 1.0,
                lfo_hz: 0.0,
                lfo_depth: 0.0,
            }),
            Self::Chorus(ChorusParams {
                rate_hz: 0.8,
                depth_ms: 3.0,
                delay_ms: 12.0,
                feedback: 0.1,
                mix: 0.5,
            }),
            Self::Saturator(SaturatorParams {
                curve: SaturatorCurve::Soft,
                drive_db: 6.0,
                output_db: 0.0,
                mix: 1.0,
            }),
            Self::Gate(GateParams {
                threshold_db: -40.0,
                attack_ms: 1.0,
                hold_ms: 20.0,
                release_ms: 100.0,
                floor_db: -80.0,
            }),
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Eq(_) => "EQ",
            Self::Compressor(_) => "Compressor",
            Self::Delay(_) => "Delay",
            Self::Reverb(_) => "Reverb",
            Self::Limiter(_) => "Limiter",
            Self::Utility(_) => "Utility",
            Self::AutoFilter(_) => "Auto Filter",
            Self::Chorus(_) => "Chorus",
            Self::Saturator(_) => "Saturator",
            Self::Gate(_) => "Gate",
        }
    }

    pub fn same_kind(&self, other: &BuiltinEffect) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Self::Eq(p) => {
                within(p.low_cut_hz, 0.0, 1000.0)
                    && [p.low_gain_db, p.mid_gain_db, p.high_gain_db]
                        .iter()
                        .all(|g| within(*g, -24.0, 24.0))
                    && within(p.low_hz, 20.0, 1000.0)
                    && within(p.mid_hz, 100.0, 10_000.0)
                    && within(p.mid_q, 0.1, 10.0)
                    && within(p.high_hz, 1000.0, 20_000.0)
            }
            Self::Compressor(p) => {
                within(p.threshold_db, -60.0, 0.0)
                    && within(p.ratio, 1.0, 20.0)
                    && within(p.attack_ms, 0.1, 200.0)
                    && within(p.release_ms, 5.0, 2000.0)
                    && within(p.makeup_db, 0.0, 24.0)
            }
            Self::Delay(p) => {
                within(p.time_ms, 1.0, MAX_DELAY_MS)
                    && within(p.feedback, 0.0, 0.95)
                    && within(p.mix, 0.0, 1.0)
            }
            Self::Reverb(p) => {
                within(p.size, 0.0, 1.0)
                    && within(p.damping, 0.0, 1.0)
                    && within(p.width, 0.0, 1.0)
                    && within(p.mix, 0.0, 1.0)
            }
            Self::Limiter(p) => {
                within(p.gain_db, 0.0, 24.0)
                    && within(p.ceiling_db, -24.0, 0.0)
                    && within(p.release_ms, 1.0, 1000.0)
            }
            Self::Utility(p) => within(p.gain_db, -60.0, 24.0) && within(p.width, 0.0, 2.0),
            Self::AutoFilter(p) => {
                within(p.cutoff_hz, 20.0, 20_000.0)
                    && within(p.resonance, 0.3, 12.0)
                    && within(p.lfo_hz, 0.0, 20.0)
                    && within(p.lfo_depth, 0.0, 1.0)
            }
            Self::Chorus(p) => {
                within(p.rate_hz, 0.01, 10.0)
                    && within(p.depth_ms, 0.0, 10.0)
                    && within(p.delay_ms, 1.0, 30.0)
                    && within(p.feedback, 0.0, 0.9)
                    && within(p.mix, 0.0, 1.0)
            }
            Self::Saturator(p) => {
                within(p.drive_db, 0.0, 36.0)
                    && within(p.output_db, -24.0, 12.0)
                    && within(p.mix, 0.0, 1.0)
            }
            Self::Gate(p) => {
                within(p.threshold_db, -80.0, 0.0)
                    && within(p.attack_ms, 0.1, 100.0)
                    && within(p.hold_ms, 0.0, 500.0)
                    && within(p.release_ms, 5.0, 2000.0)
                    && within(p.floor_db, -80.0, 0.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_distinct() {
        let all = BuiltinEffect::all();
        assert!(all.iter().all(BuiltinEffect::is_valid));
        assert!(!all[0].same_kind(&all[1]));
        assert!(all[0].same_kind(&all[0]));
        let mut bad = all[2];
        if let BuiltinEffect::Delay(p) = &mut bad {
            p.feedback = 1.5;
        }
        assert!(!bad.is_valid());
    }

    #[test]
    fn delay_sync_maps_to_beats() {
        assert_eq!(DelaySync::Free.beats(), None);
        assert_eq!(DelaySync::DottedEighth.beats(), Some(0.75));
    }
}
