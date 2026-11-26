use fnoob_core::effects::{
    AutoFilterParams, BuiltinEffect, ChorusParams, CompressorParams, DelayParams, EqParams,
    FilterMode, GateParams, LimiterParams, MAX_CHORUS_MS, MAX_DELAY_MS, ReverbParams,
    SaturatorCurve, SaturatorParams, UtilityParams,
};
use fnoob_core::model::db_to_gain;
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn set(&mut self, coefficients: [f32; 5]) {
        let [b0, b1, b2, a1, a2] = coefficients;
        self.b0 = b0;
        self.b1 = b1;
        self.b2 = b2;
        self.a1 = a1;
        self.a2 = a2;
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        if !self.z1.is_finite() || !self.z2.is_finite() {
            self.z1 = 0.0;
            self.z2 = 0.0;
        }
        y
    }
}

fn normalize(b: [f32; 3], a: [f32; 3]) -> [f32; 5] {
    [
        b[0] / a[0],
        b[1] / a[0],
        b[2] / a[0],
        a[1] / a[0],
        a[2] / a[0],
    ]
}

fn peaking(rate: f32, hz: f32, q: f32, db: f32) -> [f32; 5] {
    let a = 10f32.powf(db / 40.0);
    let w = 2.0 * PI * hz.min(rate * 0.45) / rate;
    let alpha = w.sin() / (2.0 * q);
    let cos = w.cos();
    normalize(
        [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
        [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
    )
}

fn shelf(rate: f32, hz: f32, db: f32, high: bool) -> [f32; 5] {
    let a = 10f32.powf(db / 40.0);
    let w = 2.0 * PI * hz.min(rate * 0.45) / rate;
    let cos = w.cos();
    let alpha = w.sin() / 2.0 * std::f32::consts::SQRT_2;
    let root = 2.0 * a.sqrt() * alpha;
    let sign = if high { -1.0 } else { 1.0 };
    normalize(
        [
            a * ((a + 1.0) - sign * (a - 1.0) * cos + root),
            2.0 * sign * a * ((a - 1.0) - sign * (a + 1.0) * cos),
            a * ((a + 1.0) - sign * (a - 1.0) * cos - root),
        ],
        [
            (a + 1.0) + sign * (a - 1.0) * cos + root,
            -2.0 * sign * ((a - 1.0) + sign * (a + 1.0) * cos),
            (a + 1.0) + sign * (a - 1.0) * cos - root,
        ],
    )
}

fn high_pass(rate: f32, hz: f32) -> [f32; 5] {
    let w = 2.0 * PI * hz.min(rate * 0.45) / rate;
    let alpha = w.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
    let cos = w.cos();
    normalize(
        [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
        [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
    )
}

fn filter(rate: f32, mode: FilterMode, hz: f32, q: f32) -> [f32; 5] {
    let w = 2.0 * PI * hz.clamp(10.0, rate * 0.45) / rate;
    let alpha = w.sin() / (2.0 * q.max(0.1));
    let cos = w.cos();
    let b = match mode {
        FilterMode::LowPass => [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0],
        FilterMode::HighPass => [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
        FilterMode::BandPass => [alpha, 0.0, -alpha],
    };
    normalize(b, [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
}

const FILTER_STEP: u32 = 32;

#[derive(Debug, Clone, PartialEq)]
struct AutoFilter {
    params: AutoFilterParams,
    bands: [Biquad; 2],
    phase: f32,
    countdown: u32,
}

impl AutoFilter {
    fn retune(&mut self, rate: f32) {
        let p = self.params;
        let swing = (self.phase * 2.0 * PI).sin() * p.lfo_depth * 3.0;
        let hz = p.cutoff_hz * 2f32.powf(swing);
        let coefficients = filter(rate, p.mode, hz, p.resonance);
        for band in &mut self.bands {
            band.set(coefficients);
        }
    }

    fn run(&mut self, left: &mut f32, right: &mut f32, rate: f32) {
        if self.countdown == 0 {
            self.retune(rate);
            self.countdown = FILTER_STEP;
        }
        self.countdown -= 1;
        self.phase = (self.phase + self.params.lfo_hz / rate).fract();
        *left = self.bands[0].run(*left);
        *right = self.bands[1].run(*right);
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Chorus {
    params: ChorusParams,
    left: Vec<f32>,
    right: Vec<f32>,
    write: usize,
    phase: f32,
}

fn read_delayed(buffer: &[f32], write: usize, delay: f32) -> f32 {
    let len = buffer.len();
    let delay = delay.clamp(1.0, (len - 2) as f32);
    let whole = delay.floor() as usize;
    let fraction = delay - whole as f32;
    let a = buffer[(write + len - whole) % len];
    let b = buffer[(write + len - whole - 1) % len];
    a + (b - a) * fraction
}

impl Chorus {
    fn run(&mut self, left: &mut f32, right: &mut f32, rate: f32) {
        let p = self.params;
        let base = p.delay_ms * 0.001 * rate;
        let depth = p.depth_ms * 0.001 * rate;
        let swing_l = (self.phase * 2.0 * PI).sin();
        let swing_r = ((self.phase + 0.25) * 2.0 * PI).sin();
        let wet_l = read_delayed(&self.left, self.write, base + depth * swing_l);
        let wet_r = read_delayed(&self.right, self.write, base + depth * swing_r);
        self.left[self.write] = *left + wet_l * p.feedback;
        self.right[self.write] = *right + wet_r * p.feedback;
        self.write = (self.write + 1) % self.left.len();
        self.phase = (self.phase + p.rate_hz / rate).fract();
        *left += (wet_l - *left) * p.mix;
        *right += (wet_r - *right) * p.mix;
    }
}

fn saturate(curve: SaturatorCurve, x: f32) -> f32 {
    match curve {
        SaturatorCurve::Soft => x.tanh(),
        SaturatorCurve::Hard => x.clamp(-1.0, 1.0),
        SaturatorCurve::Warm => (x + 0.2 * x * x).tanh(),
    }
}

fn saturator(p: &SaturatorParams, left: &mut f32, right: &mut f32) {
    let drive = db_to_gain(p.drive_db);
    let output = db_to_gain(p.output_db);
    for sample in [left, right] {
        let wet = saturate(p.curve, *sample * drive) * output;
        *sample += (wet - *sample) * p.mix;
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Gate {
    params: GateParams,
    gain: f32,
    held: u32,
    attack: f32,
    release: f32,
}

impl Gate {
    fn configure(&mut self, rate: f32) {
        self.attack = coefficient(self.params.attack_ms, rate);
        self.release = coefficient(self.params.release_ms, rate);
    }

    fn run(&mut self, left: &mut f32, right: &mut f32, rate: f32, key: Option<f32>) {
        let p = self.params;
        let level = key.unwrap_or_else(|| left.abs().max(right.abs()));
        if level >= db_to_gain(p.threshold_db) {
            self.held = (p.hold_ms * 0.001 * rate) as u32;
        } else {
            self.held = self.held.saturating_sub(1);
        }
        let open = level >= db_to_gain(p.threshold_db) || self.held > 0;
        let target = if open { 1.0 } else { db_to_gain(p.floor_db) };
        let k = if target > self.gain {
            self.attack
        } else {
            self.release
        };
        self.gain = target + (self.gain - target) * k;
        *left *= self.gain;
        *right *= self.gain;
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Eq {
    params: EqParams,
    bands: [[Biquad; 2]; 4],
}

impl Eq {
    fn configure(&mut self, rate: f32) {
        let p = self.params;
        let sets = [
            if p.low_cut_hz > 0.0 {
                high_pass(rate, p.low_cut_hz)
            } else {
                [1.0, 0.0, 0.0, 0.0, 0.0]
            },
            shelf(rate, p.low_hz, p.low_gain_db, false),
            peaking(rate, p.mid_hz, p.mid_q, p.mid_gain_db),
            shelf(rate, p.high_hz, p.high_gain_db, true),
        ];
        for (band, coefficients) in self.bands.iter_mut().zip(sets) {
            for channel in band.iter_mut() {
                channel.set(coefficients);
            }
        }
    }

    fn run(&mut self, left: &mut f32, right: &mut f32) {
        for band in &mut self.bands {
            *left = band[0].run(*left);
            *right = band[1].run(*right);
        }
    }
}

fn coefficient(ms: f32, rate: f32) -> f32 {
    (-1.0 / (ms.max(0.01) * 0.001 * rate)).exp()
}

#[derive(Debug, Clone, PartialEq)]
struct Compressor {
    params: CompressorParams,
    envelope: f32,
    attack: f32,
    release: f32,
    gain_reduction_db: f32,
}

impl Compressor {
    fn configure(&mut self, rate: f32) {
        self.attack = coefficient(self.params.attack_ms, rate);
        self.release = coefficient(self.params.release_ms, rate);
    }

    fn run(&mut self, left: &mut f32, right: &mut f32, key: Option<f32>) {
        let level = key.unwrap_or_else(|| left.abs().max(right.abs()));
        let coeff = if level > self.envelope {
            self.attack
        } else {
            self.release
        };
        self.envelope = level + coeff * (self.envelope - level);
        let level_db = 20.0 * self.envelope.max(1e-9).log10();
        let over = level_db - self.params.threshold_db;
        self.gain_reduction_db = if over > 0.0 {
            over - over / self.params.ratio
        } else {
            0.0
        };
        let gain = db_to_gain(self.params.makeup_db - self.gain_reduction_db);
        *left *= gain;
        *right *= gain;
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Delay {
    params: DelayParams,
    left: Vec<f32>,
    right: Vec<f32>,
    write: usize,
    rate: f32,
}

impl Delay {
    fn run(&mut self, left: &mut f32, right: &mut f32, tempo: f64) {
        let size = self.left.len();
        let ms = match self.params.sync.beats() {
            Some(beats) => (beats * 60_000.0 / tempo.max(1.0)) as f32,
            None => self.params.time_ms,
        };
        let samples = ((ms.min(MAX_DELAY_MS) * 0.001 * self.rate) as usize).clamp(1, size - 1);
        let read = (self.write + size - samples) % size;
        let (dl, dr) = (self.left[read], self.right[read]);
        let fb = self.params.feedback;
        let (in_l, in_r) = (*left, *right);
        if self.params.ping_pong {
            self.left[self.write] = (in_l + in_r) * 0.5 + dr * fb;
            self.right[self.write] = dl * fb;
        } else {
            self.left[self.write] = in_l + dl * fb;
            self.right[self.write] = in_r + dr * fb;
        }
        for value in [&mut self.left[self.write], &mut self.right[self.write]] {
            if !value.is_finite() {
                *value = 0.0;
            }
        }
        self.write = (self.write + 1) % size;
        let mix = self.params.mix;
        *left = in_l * (1.0 - mix) + dl * mix;
        *right = in_r * (1.0 - mix) + dr * mix;
    }
}

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

#[derive(Debug, Clone, PartialEq)]
struct Comb {
    buffer: Vec<f32>,
    index: usize,
    store: f32,
}

impl Comb {
    fn run(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let output = self.buffer[self.index];
        self.store = output * (1.0 - damp) + self.store * damp;
        self.buffer[self.index] = input + self.store * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Allpass {
    buffer: Vec<f32>,
    index: usize,
}

impl Allpass {
    fn run(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.index];
        let output = delayed - input;
        self.buffer[self.index] = input + delayed * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Reverb {
    params: ReverbParams,
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
}

impl Reverb {
    fn new(params: ReverbParams, rate: f32) -> Self {
        let scale = rate / 44_100.0;
        let len = |n: usize, spread: usize| (((n + spread) as f32 * scale) as usize).max(1);
        let combs = [0, STEREO_SPREAD].map(|spread| {
            COMB_TUNING
                .iter()
                .map(|&n| Comb {
                    buffer: vec![0.0; len(n, spread)],
                    index: 0,
                    store: 0.0,
                })
                .collect()
        });
        let allpasses = [0, STEREO_SPREAD].map(|spread| {
            ALLPASS_TUNING
                .iter()
                .map(|&n| Allpass {
                    buffer: vec![0.0; len(n, spread)],
                    index: 0,
                })
                .collect()
        });
        Self {
            params,
            combs,
            allpasses,
        }
    }

    fn run(&mut self, left: &mut f32, right: &mut f32) {
        let input = (*left + *right) * 0.015;
        let feedback = 0.7 + self.params.size * 0.28;
        let damp = self.params.damping * 0.4;
        let mut wet = [0.0f32; 2];
        for (channel, out) in wet.iter_mut().enumerate() {
            let mut sum = 0.0;
            for comb in &mut self.combs[channel] {
                sum += comb.run(input, feedback, damp);
            }
            for allpass in &mut self.allpasses[channel] {
                sum = allpass.run(sum);
            }
            *out = if sum.is_finite() { sum } else { 0.0 };
        }
        let width = self.params.width;
        let wet1 = width * 0.5 + 0.5;
        let wet2 = (1.0 - width) * 0.5;
        let out_l = wet[0] * wet1 + wet[1] * wet2;
        let out_r = wet[1] * wet1 + wet[0] * wet2;
        let mix = self.params.mix;
        *left = *left * (1.0 - mix) + out_l * mix * 3.0;
        *right = *right * (1.0 - mix) + out_r * mix * 3.0;
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Limiter {
    params: LimiterParams,
    gain: f32,
    release: f32,
}

impl Limiter {
    fn run(&mut self, left: &mut f32, right: &mut f32) {
        let drive = db_to_gain(self.params.gain_db);
        let ceiling = db_to_gain(self.params.ceiling_db);
        let (l, r) = (*left * drive, *right * drive);
        let peak = l.abs().max(r.abs());
        let target = if peak > ceiling { ceiling / peak } else { 1.0 };
        self.gain = if target < self.gain {
            target
        } else {
            target + self.release * (self.gain - target)
        };
        *left = (l * self.gain).clamp(-ceiling, ceiling);
        *right = (r * self.gain).clamp(-ceiling, ceiling);
    }
}

fn utility(params: &UtilityParams, left: &mut f32, right: &mut f32) {
    let gain = db_to_gain(params.gain_db);
    let (mut l, mut r) = (*left * gain, *right * gain);
    if params.invert_left {
        l = -l;
    }
    if params.invert_right {
        r = -r;
    }
    let mid = (l + r) * 0.5;
    let side = (l - r) * 0.5 * if params.mono { 0.0 } else { params.width };
    *left = mid + side;
    *right = mid - side;
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Eq(Eq),
    Compressor(Compressor),
    Delay(Delay),
    Reverb(Reverb),
    Limiter(Limiter),
    Utility(UtilityParams),
    AutoFilter(AutoFilter),
    Chorus(Chorus),
    Saturator(SaturatorParams),
    Gate(Gate),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuiltinProcessor {
    state: State,
    rate: f32,
    key: Vec<f32>,
    keyed: usize,
}

impl BuiltinProcessor {
    pub fn new(effect: BuiltinEffect, sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let state = match effect {
            BuiltinEffect::Eq(params) => State::Eq(Eq {
                params,
                bands: Default::default(),
            }),
            BuiltinEffect::Compressor(params) => State::Compressor(Compressor {
                params,
                envelope: 0.0,
                attack: 0.0,
                release: 0.0,
                gain_reduction_db: 0.0,
            }),
            BuiltinEffect::Delay(params) => {
                let size = (MAX_DELAY_MS * 0.001 * rate) as usize + 2;
                State::Delay(Delay {
                    params,
                    left: vec![0.0; size],
                    right: vec![0.0; size],
                    write: 0,
                    rate,
                })
            }
            BuiltinEffect::Reverb(params) => State::Reverb(Reverb::new(params, rate)),
            BuiltinEffect::Limiter(params) => State::Limiter(Limiter {
                params,
                gain: 1.0,
                release: 0.0,
            }),
            BuiltinEffect::Utility(params) => State::Utility(params),
            BuiltinEffect::AutoFilter(params) => State::AutoFilter(AutoFilter {
                params,
                bands: Default::default(),
                phase: 0.0,
                countdown: 0,
            }),
            BuiltinEffect::Chorus(params) => {
                let size = (MAX_CHORUS_MS * 0.001 * rate) as usize + 4;
                State::Chorus(Chorus {
                    params,
                    left: vec![0.0; size],
                    right: vec![0.0; size],
                    write: 0,
                    phase: 0.0,
                })
            }
            BuiltinEffect::Saturator(params) => State::Saturator(params),
            BuiltinEffect::Gate(params) => State::Gate(Gate {
                params,
                gain: 0.0,
                held: 0,
                attack: 0.0,
                release: 0.0,
            }),
        };
        let mut processor = Self {
            state,
            rate,
            key: vec![0.0; crate::plan::MAX_BLOCK],
            keyed: 0,
        };
        processor.configure();
        processor
    }

    fn configure(&mut self) {
        let rate = self.rate;
        match &mut self.state {
            State::Eq(eq) => eq.configure(rate),
            State::Compressor(c) => c.configure(rate),
            State::Limiter(l) => l.release = coefficient(l.params.release_ms, rate),
            State::Gate(g) => g.configure(rate),
            State::AutoFilter(f) => f.countdown = 0,
            State::Delay(_)
            | State::Reverb(_)
            | State::Utility(_)
            | State::Chorus(_)
            | State::Saturator(_) => {}
        }
    }

    pub fn set_params(&mut self, effect: BuiltinEffect) {
        match (&mut self.state, effect) {
            (State::Eq(s), BuiltinEffect::Eq(p)) => s.params = p,
            (State::Compressor(s), BuiltinEffect::Compressor(p)) => s.params = p,
            (State::Delay(s), BuiltinEffect::Delay(p)) => s.params = p,
            (State::Reverb(s), BuiltinEffect::Reverb(p)) => s.params = p,
            (State::Limiter(s), BuiltinEffect::Limiter(p)) => s.params = p,
            (State::Utility(s), BuiltinEffect::Utility(p)) => *s = p,
            (State::AutoFilter(s), BuiltinEffect::AutoFilter(p)) => s.params = p,
            (State::Chorus(s), BuiltinEffect::Chorus(p)) => s.params = p,
            (State::Saturator(s), BuiltinEffect::Saturator(p)) => *s = p,
            (State::Gate(s), BuiltinEffect::Gate(p)) => s.params = p,
            _ => return,
        }
        self.configure();
    }

    pub fn set_key(&mut self, source: Option<(&[f32], &[f32])>, frames: usize) {
        let frames = frames.min(self.key.len());
        match source {
            Some((left, right)) => {
                for ((key, l), r) in self.key[..frames].iter_mut().zip(left).zip(right) {
                    *key = l.abs().max(r.abs());
                }
            }
            None => self.key[..frames].fill(0.0),
        }
        self.keyed = frames;
    }

    pub fn gain_reduction_db(&self) -> f32 {
        match &self.state {
            State::Compressor(c) => c.gain_reduction_db,
            _ => 0.0,
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], tempo: f64) {
        let rate = self.rate;
        let keyed = self.keyed >= left.len();
        self.keyed = 0;
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let key = if keyed { Some(self.key[i]) } else { None };
            match &mut self.state {
                State::Eq(eq) => eq.run(l, r),
                State::Compressor(c) => c.run(l, r, key),
                State::Delay(d) => d.run(l, r, tempo),
                State::Reverb(v) => v.run(l, r),
                State::Limiter(m) => m.run(l, r),
                State::Utility(p) => utility(p, l, r),
                State::AutoFilter(f) => f.run(l, r, rate),
                State::Chorus(c) => c.run(l, r, rate),
                State::Saturator(p) => saturator(p, l, r),
                State::Gate(g) => g.run(l, r, rate, key),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnoob_core::effects::DelaySync;

    const SR: u32 = 48_000;

    fn defaults(index: usize) -> BuiltinEffect {
        BuiltinEffect::all()[index]
    }

    fn sine(hz: f32, frames: usize, level: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| (2.0 * PI * hz * i as f32 / SR as f32).sin() * level)
            .collect()
    }

    fn rms(values: &[f32]) -> f32 {
        (values.iter().map(|v| v * v).sum::<f32>() / values.len() as f32).sqrt()
    }

    fn run(effect: BuiltinEffect, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut p = BuiltinProcessor::new(effect, SR);
        let (mut l, mut r) = (input.to_vec(), input.to_vec());
        p.process(&mut l, &mut r, 120.0);
        (l, r)
    }

    #[test]
    fn auto_filter_low_pass_quiets_highs() {
        let input = sine(8000.0, 4800, 0.5);
        let (l, _) = run(defaults(6), &input);
        assert!(rms(&l[2400..]) < rms(&input[2400..]) * 0.2);
        let low = sine(100.0, 4800, 0.5);
        let (l, _) = run(defaults(6), &low);
        assert!(rms(&l[2400..]) > rms(&low[2400..]) * 0.8);
    }

    #[test]
    fn chorus_stays_bounded_and_changes_the_sound() {
        let input = sine(440.0, 9600, 0.5);
        let (l, r) = run(defaults(7), &input);
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 2.0));
        assert!(l.iter().zip(&input).any(|(a, b)| (a - b).abs() > 0.01));
        assert!(l.iter().zip(&r).any(|(a, b)| (a - b).abs() > 0.001));
    }

    #[test]
    fn saturator_limits_peaks() {
        let input = sine(220.0, 4800, 0.9);
        let mut effect = defaults(8);
        if let BuiltinEffect::Saturator(p) = &mut effect {
            p.drive_db = 24.0;
            p.curve = SaturatorCurve::Hard;
        }
        let (l, _) = run(effect, &input);
        assert!(l.iter().all(|v| v.abs() <= 1.0 + 1e-6));
        assert!(rms(&l) > rms(&input));
    }

    #[test]
    fn gate_silences_quiet_sound_and_passes_loud_sound() {
        let quiet = sine(220.0, 9600, 0.001);
        let (l, _) = run(defaults(9), &quiet);
        assert!(rms(&l[4800..]) < 1e-5);
        let loud = sine(220.0, 9600, 0.5);
        let (l, _) = run(defaults(9), &loud);
        assert!(rms(&l[4800..]) > rms(&loud[4800..]) * 0.9);
    }

    #[test]
    fn flat_eq_passes_audio_unchanged() {
        let input = sine(440.0, 4800, 0.5);
        let (l, _) = run(defaults(0), &input);
        assert!(l.iter().zip(&input).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn eq_boost_and_low_cut_change_levels() {
        let mut effect = defaults(0);
        if let BuiltinEffect::Eq(p) = &mut effect {
            p.mid_gain_db = 12.0;
            p.mid_hz = 1000.0;
        }
        let input = sine(1000.0, 9600, 0.1);
        let (l, _) = run(effect, &input);
        assert!(rms(&l[4800..]) > rms(&input[4800..]) * 3.0);
        let mut cut = defaults(0);
        if let BuiltinEffect::Eq(p) = &mut cut {
            p.low_cut_hz = 400.0;
        }
        let low = sine(50.0, 9600, 0.5);
        let (l, _) = run(cut, &low);
        assert!(rms(&l[4800..]) < rms(&low[4800..]) * 0.1);
    }

    #[test]
    fn compressor_reduces_loud_signals() {
        let input = sine(200.0, 48_000, 0.9);
        let (l, _) = run(defaults(1), &input);
        assert!(rms(&l[24_000..]) < rms(&input[24_000..]) * 0.6);
        let quiet = sine(200.0, 4800, 0.01);
        let (q, _) = run(defaults(1), &quiet);
        assert!(q.iter().zip(&quiet).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn synced_delay_repeats_after_one_eighth() {
        let mut effect = defaults(2);
        if let BuiltinEffect::Delay(p) = &mut effect {
            p.sync = DelaySync::Eighth;
            p.mix = 1.0;
            p.feedback = 0.0;
        }
        let mut input = vec![0.0; 30_000];
        input[0] = 1.0;
        let (l, r) = run(effect, &input);
        assert_eq!(l[0], 0.0);
        assert_eq!(l[12_000], 1.0);
        assert_eq!(r[12_000], 1.0);
    }

    #[test]
    fn reverb_leaves_a_finite_tail() {
        let mut input = vec![0.0; 48_000];
        input[0] = 1.0;
        let (l, r) = run(defaults(3), &input);
        assert!(rms(&l[20_000..30_000]) > 1e-5);
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 2.0));
    }

    #[test]
    fn limiter_keeps_peaks_under_the_ceiling() {
        let mut effect = defaults(4);
        if let BuiltinEffect::Limiter(p) = &mut effect {
            p.gain_db = 12.0;
            p.ceiling_db = -6.0;
        }
        let (l, _) = run(effect, &sine(100.0, 9600, 0.9));
        let ceiling = db_to_gain(-6.0);
        assert!(l.iter().all(|v| v.abs() <= ceiling + 1e-6));
    }

    #[test]
    fn utility_mono_and_invert() {
        let mut effect = defaults(5);
        if let BuiltinEffect::Utility(p) = &mut effect {
            p.mono = true;
        }
        let mut proc = BuiltinProcessor::new(effect, SR);
        let (mut l, mut r) = (vec![1.0], vec![0.0]);
        proc.process(&mut l, &mut r, 120.0);
        assert_eq!((l[0], r[0]), (0.5, 0.5));
        if let BuiltinEffect::Utility(p) = &mut effect {
            p.mono = false;
            p.invert_left = true;
        }
        proc.set_params(effect);
        let (mut l, mut r) = (vec![0.5], vec![0.5]);
        proc.process(&mut l, &mut r, 120.0);
        assert_eq!((l[0], r[0]), (-0.5, 0.5));
    }

    #[test]
    fn params_of_another_kind_are_ignored() {
        let mut proc = BuiltinProcessor::new(defaults(5), SR);
        let before = proc.clone();
        proc.set_params(defaults(0));
        assert_eq!(proc, before);
    }
}
