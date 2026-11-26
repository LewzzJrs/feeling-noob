use crate::plan::NO_OFF;
use fnoob_core::midi_fx::ArpStyle;

#[derive(Debug, Clone, PartialEq)]
pub struct Arp {
    style: ArpStyle,
    step: u64,
    octaves: u8,
    gate: f32,
    velocity: [u8; 128],
    ends: [u64; 128],
    index: u64,
    next_at: u64,
    rng: u64,
}

impl Arp {
    pub fn new(style: ArpStyle, step: u64, octaves: u8, gate: f32) -> Self {
        Self {
            style,
            step: step.max(1),
            octaves: octaves.clamp(1, 4),
            gate: gate.clamp(0.05, 1.0),
            velocity: [0; 128],
            ends: [NO_OFF; 128],
            index: 0,
            next_at: 0,
            rng: 0x2545_F491_4F6C_DD1D,
        }
    }

    pub fn hold(&mut self, pitch: u8, velocity: u8, end: u64) {
        let i = usize::from(pitch.min(127));
        self.velocity[i] = velocity.max(1);
        self.ends[i] = end;
    }

    pub fn release(&mut self, pitch: u8) {
        self.velocity[usize::from(pitch.min(127))] = 0;
    }

    pub fn clear(&mut self) {
        self.velocity = [0; 128];
        self.index = 0;
        self.next_at = 0;
    }

    fn held(&self) -> usize {
        self.velocity.iter().filter(|v| **v > 0).count()
    }

    fn nth(&self, n: usize) -> Option<(u8, u8)> {
        self.velocity
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0)
            .nth(n)
            .map(|(p, v)| (p as u8, *v))
    }

    fn pick(&mut self, total: u64) -> u64 {
        let k = self.index;
        match self.style {
            ArpStyle::Up => k % total,
            ArpStyle::Down => total - 1 - k % total,
            ArpStyle::UpDown => {
                if total < 2 {
                    return 0;
                }
                let cycle = 2 * total - 2;
                let at = k % cycle;
                if at < total { at } else { cycle - at }
            }
            ArpStyle::Random => {
                self.rng ^= self.rng << 13;
                self.rng ^= self.rng >> 7;
                self.rng ^= self.rng << 17;
                self.rng % total
            }
        }
    }

    pub fn tick(&mut self, position: u64) -> Option<(u8, u8, u64)> {
        for (velocity, end) in self.velocity.iter_mut().zip(self.ends.iter()) {
            if *velocity > 0 && *end <= position {
                *velocity = 0;
            }
        }
        let held = self.held();
        if held == 0 {
            self.index = 0;
            self.next_at = 0;
            return None;
        }
        if position < self.next_at {
            return None;
        }
        let total = held as u64 * u64::from(self.octaves);
        let k = self.pick(total);
        self.index += 1;
        self.next_at = position + self.step;
        let (base, velocity) = self.nth((k % held as u64) as usize)?;
        let pitch = u16::from(base) + 12 * (k / held as u64) as u16;
        let pitch = u8::try_from(pitch.min(127)).unwrap_or(127);
        let length = ((self.step as f32 * self.gate) as u64).max(1);
        Some((pitch, velocity, position + length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(arp: &mut Arp, frames: u64) -> Vec<(u64, u8)> {
        (0..frames)
            .filter_map(|p| arp.tick(p).map(|(pitch, _, _)| (p, pitch)))
            .collect()
    }

    #[test]
    fn steps_up_through_held_notes_and_octaves() {
        let mut arp = Arp::new(ArpStyle::Up, 10, 2, 0.5);
        arp.hold(64, 100, NO_OFF);
        arp.hold(60, 90, NO_OFF);
        let played = run(&mut arp, 40);
        assert_eq!(played, vec![(0, 60), (10, 64), (20, 72), (30, 76)]);
    }

    #[test]
    fn up_down_turns_around_and_stops_when_released() {
        let mut arp = Arp::new(ArpStyle::UpDown, 5, 1, 0.5);
        for p in [60, 64, 67] {
            arp.hold(p, 100, NO_OFF);
        }
        let played: Vec<u8> = run(&mut arp, 25).into_iter().map(|(_, p)| p).collect();
        assert_eq!(played, vec![60, 64, 67, 64, 60]);
        arp.clear();
        assert!(arp.tick(30).is_none());
    }

    #[test]
    fn timed_notes_end_by_themselves() {
        let mut arp = Arp::new(ArpStyle::Down, 4, 1, 1.0);
        arp.hold(60, 100, 6);
        let played = run(&mut arp, 20);
        assert_eq!(played, vec![(0, 60), (4, 60)]);
    }
}
