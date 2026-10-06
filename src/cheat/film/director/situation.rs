//! What the hero is doing and what is about it, for the camera to answer (Cinemachine's
//! state-driven camera, RDR2's cinematic mode): moving, standing, turning, climbing, going down,
//! fighting, meeting someone, coming on something to pick up or a thing to work; and how open the place is. Read from the hero's last second of movement, what
//! stands about it, and the obstacles.

use crate::film::{Subject, SubjectKind};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Situation {
    Moving,
    Still,
    Turning,
    Climbing,
    Descending,
    Combat,
    /// Someone near: framed together, as a conversation is.
    Meeting,
    /// Something to pick up ahead: looked at, closing in.
    Find,
    /// A thing to work ahead (a lever, a door, a save point): shown from high, with the hero.
    Landmark,
}

/// The hero's movement over its last second.
#[derive(Default)]
pub struct Motion {
    seen: VecDeque<(f32, [f32; 3])>,
    t: f32,
}

/// How long the movement is judged over (s), how fast is moving (cm/s), how fast a turn counts
/// (degrees a second), how fast a climb and a descent (cm/s), how near an enemy makes a fight and a
/// thing an interest (cm), and how far round the way ahead a thing must be (degrees).
const OVER: f32 = 1.0;
const MOVING: f32 = 80.0;
const TURNING: f32 = 70.0;
const CLIMBING: f32 = 60.0;
const DESCENDING: f32 = -80.0;
const FIGHT: f32 = 1200.0;
const MEETING: f32 = 800.0;
const INTEREST: f32 = 600.0;
const AHEAD: f32 = 70.0;

impl Motion {
    pub fn see(&mut self, p: [f32; 3], dt: f32) {
        self.t += dt;
        self.seen.push_back((self.t, p));
        while self.seen.front().is_some_and(|(t, _)| self.t - t > OVER) {
            self.seen.pop_front();
        }
    }

    /// Speed over the ground and up (cm/s), and the way it goes (degrees), over the last second.
    fn velocity(&self) -> Option<(f32, f32, f32)> {
        let (&(t0, a), &(t1, b)) = (self.seen.front()?, self.seen.back()?);
        let dt = t1 - t0;
        (dt > 0.2).then(|| {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            (dx.hypot(dy) / dt, (b[2] - a[2]) / dt, dy.atan2(dx).to_degrees())
        })
    }

    /// How fast the way it goes turns (degrees a second): the first half-second's way against
    /// the second's.
    fn turning(&self) -> f32 {
        let n = self.seen.len();
        if n < 6 {
            return 0.0;
        }
        let way = |a: usize, b: usize| {
            let (p, q) = (self.seen[a].1, self.seen[b].1);
            ((q[0] - p[0]).hypot(q[1] - p[1]) > 20.0).then(|| (q[1] - p[1]).atan2(q[0] - p[0]).to_degrees())
        };
        match (way(0, n / 2), way(n / 2, n - 1)) {
            (Some(a), Some(b)) => crate::film::wrap(b - a).abs() / (self.seen[n - 1].0 - self.seen[0].0).max(0.2),
            _ => 0.0,
        }
    }

    pub fn speed(&self) -> f32 {
        self.velocity().map_or(0.0, |v| v.0)
    }

    pub fn heading(&self) -> Option<f32> {
        self.velocity().filter(|v| v.0 > MOVING).map(|v| v.2)
    }
}

/// What the camera answers, who it looks at besides the hero (an enemy, a thing), and how open
/// the place is (0 walled in – 1 open all round).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sight {
    pub situation: Situation,
    pub target: Option<[f32; 3]>,
    pub open: f32,
}

/// How far round the hero is looked for walls (cm), and in how many ways.
const OPEN_REACH: f32 = 800.0;
const OPEN_WAYS: usize = 8;

/// How open the place about `hero` is: the share of the ways round it clear for `OPEN_REACH`, at
/// head height.
pub fn openness(b: &crate::obstacles::Blocking, hero: [f32; 3]) -> f32 {
    let head = [hero[0], hero[1], hero[2] + 150.0];
    let clear = (0..OPEN_WAYS)
        .filter(|&k| {
            let a = (k as f32 * 360.0 / OPEN_WAYS as f32).to_radians();
            !b.blocks(head, [head[0] + a.cos() * OPEN_REACH, head[1] + a.sin() * OPEN_REACH, head[2]])
        })
        .count();
    clear as f32 / OPEN_WAYS as f32
}

/// The hero at `hero` going `heading`, moving as `m`, among `subjects`, in a place `open`.
pub fn judge(m: &Motion, hero: [f32; 3], heading: f32, subjects: &[Subject], open: f32) -> Sight {
    let dist = |s: &Subject| (s.at[0] - hero[0]).hypot(s.at[1] - hero[1]);
    let nearest = |kind: SubjectKind, within: f32, ahead: bool| {
        subjects
            .iter()
            .filter(|s| s.kind == kind && dist(s) < within)
            .filter(|s| {
                !ahead
                    || crate::film::wrap((s.at[1] - hero[1]).atan2(s.at[0] - hero[0]).to_degrees() - heading).abs()
                        < AHEAD
            })
            .min_by(|a, b| dist(a).total_cmp(&dist(b)))
            .map(|s| s.at)
    };
    let at = |situation, target| Sight { situation, target: Some(target), open };
    if let Some(e) = nearest(SubjectKind::Enemy, FIGHT, false) {
        return at(Situation::Combat, e);
    }
    if let Some(t) = nearest(SubjectKind::Npc, MEETING, false) {
        return at(Situation::Meeting, t);
    }
    if let Some(t) = nearest(SubjectKind::Item, INTEREST, true) {
        return at(Situation::Find, t);
    }
    if let Some(t) = nearest(SubjectKind::Thing, INTEREST, true) {
        return at(Situation::Landmark, t);
    }
    let (speed, rise) = m.velocity().map_or((0.0, 0.0), |v| (v.0, v.1));
    let situation = if rise > CLIMBING {
        Situation::Climbing
    } else if rise < DESCENDING {
        Situation::Descending
    } else if speed > MOVING && m.turning() > TURNING {
        Situation::Turning
    } else if speed > MOVING {
        Situation::Moving
    } else {
        Situation::Still
    };
    Sight { situation, target: None, open }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(points: impl Fn(f32) -> [f32; 3]) -> Motion {
        let mut m = Motion::default();
        for i in 0..60 {
            m.see(points(i as f32 / 50.0), 0.02);
        }
        m
    }

    #[test]
    fn what_the_hero_does_is_told() {
        let at = |m: &Motion| judge(m, m.seen.back().unwrap().1, 0.0, &[], 1.0).situation;
        assert_eq!(at(&walk(|_| [0.0, 0.0, 0.0])), Situation::Still);
        assert_eq!(at(&walk(|t| [t * 300.0, 0.0, 0.0])), Situation::Moving);
        assert_eq!(at(&walk(|t| [t * 300.0, 0.0, t * 150.0])), Situation::Climbing);
        assert_eq!(at(&walk(|t| [(t * 3.0).cos() * 150.0, (t * 3.0).sin() * 150.0, 0.0])), Situation::Turning);
        let enemy = [Subject { kind: SubjectKind::Enemy, at: [500.0, 0.0, 0.0] }];
        let m = walk(|_| [0.0, 0.0, 0.0]);
        assert_eq!(judge(&m, [0.0; 3], 0.0, &enemy, 1.0).situation, Situation::Combat);
        let lever = [Subject { kind: SubjectKind::Thing, at: [300.0, 0.0, 0.0] }];
        assert_eq!(judge(&m, [0.0; 3], 0.0, &lever, 1.0).situation, Situation::Landmark);
        let item = [Subject { kind: SubjectKind::Item, at: [300.0, 0.0, 0.0] }];
        assert_eq!(judge(&m, [0.0; 3], 0.0, &item, 1.0).situation, Situation::Find);
        // behind the hero, an item is not looked for; a person is met from any side
        let behind = [Subject { kind: SubjectKind::Item, at: [-300.0, 0.0, 0.0] }];
        assert_eq!(judge(&m, [0.0; 3], 0.0, &behind, 1.0).situation, Situation::Still);
        let npc = [Subject { kind: SubjectKind::Npc, at: [-500.0, 0.0, 0.0] }];
        assert_eq!(judge(&m, [0.0; 3], 0.0, &npc, 1.0).situation, Situation::Meeting);
    }
}
