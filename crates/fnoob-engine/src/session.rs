use crate::audio_data::{AudioData, AudioPool};
use crate::plan::PlannedNote;
use fnoob_core::model::{RegionContent, Track};
use fnoob_core::session::{FollowAction, LaunchMode, SessionClip};
use fnoob_core::time::Timeline;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionAudio {
    pub data: Arc<AudioData>,
    pub offset: u64,
    pub gain: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionClipPlan {
    pub length: u64,
    pub looping: bool,
    pub launch: LaunchMode,
    pub follow_after: u64,
    pub follow_a: FollowAction,
    pub follow_b: FollowAction,
    pub chance_a: f32,
    pub notes: Vec<PlannedNote>,
    pub audio: Option<SessionAudio>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Queued {
    pub slot: Option<usize>,
    pub at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionPlay {
    pub playing: Option<usize>,
    pub elapsed: u64,
    pub played: u64,
    pub cursor: usize,
    pub queued: Option<Queued>,
    pub held: Option<usize>,
    pub repeat_at: u64,
    pub overrides: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Session {
    pub slots: Vec<Option<SessionClipPlan>>,
    pub play: SessionPlay,
}

pub fn next_boundary(position: u64, quantum: u64) -> u64 {
    if quantum == 0 {
        position
    } else {
        position.div_ceil(quantum) * quantum
    }
}

pub fn random(state: &mut u64) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

fn clip_plan(clip: &SessionClip, timeline: Timeline, pool: &AudioPool) -> Option<SessionClipPlan> {
    let region = &clip.region;
    let length = timeline.tick_to_sample(region.length_tick);
    if length == 0 {
        return None;
    }
    let (notes, audio) = match &region.content {
        RegionContent::Midi { notes } => {
            let mut planned: Vec<PlannedNote> = notes
                .iter()
                .filter(|n| n.start_tick < region.length_tick)
                .map(|n| PlannedNote {
                    start: timeline.tick_to_sample(n.start_tick),
                    end: timeline
                        .tick_to_sample((n.start_tick + n.length_tick).min(region.length_tick)),
                    pitch: n.pitch,
                    velocity: n.velocity,
                })
                .filter(|n| n.end > n.start)
                .collect();
            planned.sort_unstable_by_key(|n| (n.start, n.pitch));
            (planned, None)
        }
        RegionContent::Audio {
            file_id,
            offset_frames,
            gain_db,
        } => {
            let data = pool.get(file_id)?;
            (
                Vec::new(),
                Some(SessionAudio {
                    data: Arc::clone(data),
                    offset: *offset_frames,
                    gain: 10f32.powf(gain_db / 20.0),
                }),
            )
        }
    };
    Some(SessionClipPlan {
        length,
        looping: clip.looping,
        launch: clip.launch,
        follow_after: timeline.tick_to_sample(clip.follow.after_tick),
        follow_a: clip.follow.a,
        follow_b: clip.follow.b,
        chance_a: clip.follow.chance_a,
        notes,
        audio,
    })
}

impl Session {
    pub fn build(track: &Track, timeline: Timeline, pool: &AudioPool) -> Self {
        Self {
            slots: track
                .session
                .iter()
                .map(|c| c.as_ref().and_then(|c| clip_plan(c, timeline, pool)))
                .collect(),
            play: SessionPlay::default(),
        }
    }

    fn has(&self, slot: usize) -> bool {
        matches!(self.slots.get(slot), Some(Some(_)))
    }

    pub fn clip(&self) -> Option<&SessionClipPlan> {
        self.slots.get(self.play.playing?)?.as_ref()
    }

    pub fn take_state(&mut self, old: &Session) {
        let mut play = old.play;
        if play.playing.is_some_and(|s| !self.has(s)) {
            play.playing = None;
        }
        if play
            .queued
            .is_some_and(|q| q.slot.is_some_and(|s| !self.has(s)))
        {
            play.queued = None;
        }
        if let Some(clip) = self.clip() {
            play.elapsed = play.elapsed.min(clip.length.saturating_sub(1));
            let elapsed = play.elapsed;
            play.cursor = clip.notes.partition_point(|n| n.start < elapsed);
        }
        self.play = play;
    }

    fn pending(&self) -> Option<usize> {
        match self.play.queued {
            Some(queued) => queued.slot,
            None => self.play.playing,
        }
    }

    pub fn launch(
        &mut self,
        slot: usize,
        position: u64,
        quantum: u64,
        immediate: bool,
        scene: bool,
    ) {
        let Some(Some(clip)) = self.slots.get(slot) else {
            self.stop(position, quantum, immediate);
            return;
        };
        let mode = if scene {
            LaunchMode::Trigger
        } else {
            clip.launch
        };
        let at = if immediate {
            position
        } else {
            next_boundary(position, quantum)
        };
        if mode == LaunchMode::Toggle && self.pending() == Some(slot) {
            self.play.queued = Some(Queued { slot: None, at });
            return;
        }
        self.play.queued = Some(Queued {
            slot: Some(slot),
            at,
        });
        if matches!(mode, LaunchMode::Gate | LaunchMode::Repeat) {
            self.play.held = Some(slot);
            self.play.repeat_at = at + quantum;
        }
    }

    pub fn release(&mut self, slot: usize, position: u64, quantum: u64) {
        if self.play.held != Some(slot) {
            return;
        }
        self.play.held = None;
        let gate = matches!(
            self.slots.get(slot),
            Some(Some(c)) if c.launch == LaunchMode::Gate
        );
        if gate && self.pending() == Some(slot) {
            self.play.queued = Some(Queued {
                slot: None,
                at: next_boundary(position, quantum),
            });
        }
    }

    pub fn stop(&mut self, position: u64, quantum: u64, immediate: bool) {
        if self.play.playing.is_none() && self.play.queued.is_none() {
            return;
        }
        let at = if immediate {
            position
        } else {
            next_boundary(position, quantum)
        };
        self.play.queued = Some(Queued { slot: None, at });
        self.play.held = None;
    }

    pub fn fire_queued_now(&mut self, position: u64) {
        if let Some(queued) = &mut self.play.queued {
            queued.at = position;
        }
        self.play.repeat_at = position;
    }

    pub fn halt(&mut self) {
        self.play.playing = None;
        self.play.queued = None;
        self.play.held = None;
    }

    fn start(&mut self, slot: Option<usize>) {
        self.play.playing = slot;
        self.play.elapsed = 0;
        self.play.played = 0;
        self.play.cursor = 0;
        self.play.overrides = true;
    }

    fn follow_target(
        &self,
        current: usize,
        action: FollowAction,
        rng: &mut u64,
    ) -> Option<Option<usize>> {
        let filled = || (0..self.slots.len()).filter(|&i| self.has(i));
        let count = filled().count();
        let pick = |rng: &mut u64, skip: Option<usize>| {
            let choices = filled().filter(|&i| Some(i) != skip).count();
            if choices == 0 {
                return Some(current);
            }
            let index = ((random(rng) * choices as f32) as usize).min(choices - 1);
            filled().filter(|&i| Some(i) != skip).nth(index)
        };
        match action {
            FollowAction::None => None,
            FollowAction::Stop => Some(None),
            FollowAction::Again => Some(Some(current)),
            FollowAction::Next => Some(filled().find(|&i| i > current).or_else(|| filled().next())),
            FollowAction::Previous => Some(
                filled()
                    .rfind(|&i| i < current)
                    .or_else(|| filled().next_back()),
            ),
            FollowAction::First => Some(filled().next()),
            FollowAction::Last => Some(filled().next_back()),
            FollowAction::Any if count > 0 => Some(pick(rng, None)),
            FollowAction::Other if count > 0 => Some(pick(rng, Some(current))),
            FollowAction::Any | FollowAction::Other => None,
        }
    }

    pub fn step(&mut self, position: u64, quantum: u64, rng: &mut u64) -> Option<Option<usize>> {
        let mut fired = None;
        if let Some(queued) = self.play.queued
            && position >= queued.at
        {
            self.play.queued = None;
            self.start(queued.slot);
            fired = Some(queued.slot);
        }
        if let (Some(held), Some(playing)) = (self.play.held, self.play.playing)
            && held == playing
            && quantum > 0
            && position >= self.play.repeat_at
            && self.clip().is_some_and(|c| c.launch == LaunchMode::Repeat)
        {
            self.play.repeat_at = position + quantum;
            if fired.is_none() {
                self.start(Some(playing));
                fired = Some(Some(playing));
            }
        }
        if let Some(current) = self.play.playing
            && let Some(clip) = self.clip()
            && clip.follow_after > 0
            && self.play.played >= clip.follow_after
        {
            let action = if random(rng) < clip.chance_a {
                clip.follow_a
            } else {
                clip.follow_b
            };
            self.play.played = 0;
            if let Some(target) = self.follow_target(current, action, rng) {
                self.start(target);
                fired = Some(target);
            }
        }
        fired
    }

    pub fn advance(&mut self) {
        let Some(clip) = self.clip() else {
            return;
        };
        let (length, looping) = (clip.length, clip.looping);
        self.play.elapsed += 1;
        self.play.played += 1;
        if self.play.elapsed >= length {
            if looping {
                self.play.elapsed = 0;
                self.play.cursor = 0;
            } else {
                self.play.playing = None;
            }
        }
    }

    pub fn audio_frame(&self) -> (f32, f32) {
        let Some(audio) = self.clip().and_then(|c| c.audio.as_ref()) else {
            return (0.0, 0.0);
        };
        let (l, r) = audio.data.frame(audio.offset + self.play.elapsed);
        (l * audio.gain, r * audio.gain)
    }

    pub fn status(&self) -> u64 {
        let encode = |slot: usize| (slot as u64 + 1).min(0xFFFE);
        let playing = self.play.playing.map_or(0, encode);
        let queued = match self.play.queued {
            None => 0,
            Some(Queued { slot: None, .. }) => 0xFFFF,
            Some(Queued { slot: Some(s), .. }) => encode(s),
        };
        let progress = self.clip().map_or(0, |c| {
            ((self.play.elapsed as f64 / c.length.max(1) as f64) * 65535.0) as u64
        });
        playing
            | (queued << 16)
            | (u64::from(self.play.overrides) << 32)
            | (progress.min(65535) << 40)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SessionStatus {
    pub playing: Option<usize>,
    pub queued: Option<Option<usize>>,
    pub overrides: bool,
    pub progress: f32,
}

impl SessionStatus {
    pub fn decode(bits: u64) -> Self {
        let slot = |v: u64| (v > 0).then(|| v as usize - 1);
        let queued = match (bits >> 16) & 0xFFFF {
            0 => None,
            0xFFFF => Some(None),
            v => Some(slot(v)),
        };
        Self {
            playing: slot(bits & 0xFFFF),
            queued,
            overrides: bits >> 32 & 1 == 1,
            progress: ((bits >> 40) & 0xFFFF) as f32 / 65535.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(length: u64, looping: bool, launch: LaunchMode) -> Option<SessionClipPlan> {
        Some(SessionClipPlan {
            length,
            looping,
            launch,
            follow_after: 0,
            follow_a: FollowAction::Next,
            follow_b: FollowAction::None,
            chance_a: 1.0,
            notes: Vec::new(),
            audio: None,
        })
    }

    fn session(slots: Vec<Option<SessionClipPlan>>) -> Session {
        Session {
            slots,
            play: SessionPlay::default(),
        }
    }

    fn run(s: &mut Session, from: u64, frames: u64, quantum: u64) -> Vec<(u64, Option<usize>)> {
        let mut rng = 1;
        let mut fired = Vec::new();
        for position in from..from + frames {
            if let Some(change) = s.step(position, quantum, &mut rng) {
                fired.push((position, change));
            }
            s.advance();
        }
        fired
    }

    #[test]
    fn launches_wait_for_the_next_boundary() {
        let mut s = session(vec![clip(100, true, LaunchMode::Trigger)]);
        s.launch(0, 30, 50, false, false);
        assert_eq!(run(&mut s, 30, 100, 50), vec![(50, Some(0))]);
        assert!(s.play.overrides);
        assert_eq!(s.play.elapsed, 80);
        s.stop(130, 50, false);
        assert_eq!(run(&mut s, 130, 30, 50), vec![(150, None)]);
        assert_eq!(s.play.playing, None);
    }

    #[test]
    fn one_shot_clips_end_and_looping_clips_wrap() {
        let mut s = session(vec![
            clip(10, false, LaunchMode::Trigger),
            clip(10, true, LaunchMode::Trigger),
        ]);
        s.launch(0, 0, 0, true, false);
        run(&mut s, 0, 12, 0);
        assert_eq!(s.play.playing, None);
        s.launch(1, 12, 0, true, false);
        run(&mut s, 12, 25, 0);
        assert_eq!((s.play.playing, s.play.elapsed), (Some(1), 5));
    }

    #[test]
    fn toggle_and_gate_modes() {
        let mut s = session(vec![
            clip(100, true, LaunchMode::Toggle),
            clip(100, true, LaunchMode::Gate),
        ]);
        s.launch(0, 0, 0, true, false);
        run(&mut s, 0, 5, 0);
        s.launch(0, 5, 0, true, false);
        assert_eq!(run(&mut s, 5, 1, 0), vec![(5, None)]);
        s.launch(1, 6, 0, true, false);
        run(&mut s, 6, 10, 0);
        assert_eq!(s.play.playing, Some(1));
        s.release(1, 16, 0);
        assert_eq!(run(&mut s, 16, 1, 0), vec![(16, None)]);
    }

    #[test]
    fn follow_actions_move_between_clips() {
        let mut first = clip(10, true, LaunchMode::Trigger);
        first.as_mut().unwrap().follow_after = 20;
        let mut s = session(vec![first, None, clip(10, true, LaunchMode::Trigger)]);
        s.launch(0, 0, 0, true, false);
        assert_eq!(run(&mut s, 0, 30, 0), vec![(0, Some(0)), (20, Some(2))]);
        let mut rng = 7;
        assert_eq!(
            s.follow_target(2, FollowAction::Next, &mut rng),
            Some(Some(0))
        );
        assert_eq!(
            s.follow_target(0, FollowAction::Previous, &mut rng),
            Some(Some(2))
        );
        assert_eq!(
            s.follow_target(0, FollowAction::Other, &mut rng),
            Some(Some(2))
        );
        assert_eq!(
            s.follow_target(0, FollowAction::Last, &mut rng),
            Some(Some(2))
        );
        assert_eq!(s.follow_target(0, FollowAction::Stop, &mut rng), Some(None));
    }

    #[test]
    fn status_round_trips() {
        let mut s = session(vec![
            clip(100, true, LaunchMode::Trigger),
            clip(100, true, LaunchMode::Trigger),
        ]);
        s.launch(1, 0, 0, true, false);
        run(&mut s, 0, 50, 0);
        s.launch(0, 50, 100, false, false);
        let status = SessionStatus::decode(s.status());
        assert_eq!(status.playing, Some(1));
        assert_eq!(status.queued, Some(Some(0)));
        assert!(status.overrides);
        assert!((status.progress - 0.5).abs() < 0.01);
        s.stop(50, 100, false);
        assert_eq!(SessionStatus::decode(s.status()).queued, Some(None));
    }
}
