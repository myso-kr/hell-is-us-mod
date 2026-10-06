//! The director on a flight (the camera alone, the land as the subject): the flight's path read as
//! a walk's is, cut into stretches, each looked at as an aerial film would — gliding with the way,
//! a rising reveal, diving, leaning into a turn, looking straight down over high ground, pushing in
//! at the end (.spec/FILMING-RESEARCH.md §2, drones) — each at its own height (`look`), long
//! stretches cut into varied shots (`vary`).

mod look;
mod vary;

pub use look::{Aerial, Look};

use super::read::{self, Sense};
use super::runs::{ahead_of, merge, runs, Run};
use crate::obstacles::Blocking;

/// How long the opening rise and the closing push are (cm), the shortest stretch (cm, ~3 s at a
/// flight's 450 cm/s), the rise and fall that count (cm over 10 m), and the turn (degrees).
const OPENING: f32 = 1500.0;
const CLOSING: f32 = 1500.0;
const MIN_STRETCH: f32 = 1400.0;
const CLIMB: f32 = 250.0;
const FALL: f32 = -250.0;
const TURN: f32 = 50.0;
/// How far before a stretch's end the next look starts to take over (cm).
const ANTICIPATE: f32 = 1200.0;

/// A flight's plan: its stretches and how each is looked at.
pub struct AerialPlan {
    stretches: Vec<Run<Aerial>>,
    length: f32,
}

impl AerialPlan {
    /// The plan for the flight along `path` (its points lifted as flown), reading the obstacles.
    pub fn new(path: &[[f32; 3]], b: &Blocking) -> AerialPlan {
        let senses = read::read(path, b);
        let length = senses.last().map_or(0.0, |s| s.at);
        let call = |s: &Sense| {
            if s.at < OPENING {
                Aerial::Rise
            } else if length - s.at < CLOSING {
                Aerial::Arrive
            } else if s.turn.abs() > TURN {
                Aerial::Bank
            } else if s.rise > CLIMB {
                Aerial::Rise
            } else if s.rise < FALL {
                Aerial::Dive
            } else if s.view {
                Aerial::Overhead
            } else {
                Aerial::Glide
            }
        };
        let mut stretches = runs(&senses, length, call);
        merge(&mut stretches, MIN_STRETCH, |a| matches!(a, Aerial::Arrive));
        AerialPlan { stretches: vary::vary(stretches), length }
    }

    /// The look `share` of the way along (0–1), its yaw turned to the way ahead at `ahead_yaw`,
    /// eased into the next stretch's ahead of it; and the stretch's look.
    pub fn cue(&self, share: f32, ahead_yaw: f32) -> (Look, Aerial) {
        let along = share * self.length;
        let Some(i) = self.stretches.iter().position(|r| along < r.to).or(self.stretches.len().checked_sub(1)) else {
            let l = Aerial::Glide.look(0.0, 0.0);
            return (Look { yaw: ahead_yaw, ..l }, Aerial::Glide);
        };
        let r = self.stretches[i];
        let u = (along - r.from) / (r.to - r.from).max(1.0);
        let mut look = r.what.look(u, r.turn);
        let mut what = r.what;
        if let Some(n) = self.stretches.get(i + 1) {
            let k = ahead_of(along, r.to, ANTICIPATE);
            look = look.mix(n.what.look(0.0, n.turn), k);
            if k > 0.5 {
                what = n.what;
            }
        }
        look.yaw = crate::film::wrap(ahead_yaw + look.yaw);
        (look, what)
    }

    pub fn stretches(&self) -> Vec<(f32, f32, Aerial)> {
        self.stretches.iter().map(|r| (r.from, r.to, r.what)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flight_rises_to_open_and_pushes_in_to_close() {
        let scene = crate::obstacles::Scene::default();
        let path: Vec<[f32; 3]> = (0..=30).map(|i| [i as f32 * 300.0, 0.0, 200.0]).collect();
        let plan = AerialPlan::new(&path, &scene.blocking());
        let s = plan.stretches();
        assert_eq!(s.first().unwrap().2, Aerial::Rise);
        assert_eq!(s.last().unwrap().2, Aerial::Arrive);
        // the look never jumps
        let mut last = plan.cue(0.0, 0.0).0;
        for k in 1..=1000 {
            let c = plan.cue(k as f32 / 1000.0, 0.0).0;
            assert!(
                (c.pitch - last.pitch).abs() < 2.0 && (c.fov - last.fov).abs() < 2.0,
                "a jump at {k}: {last:?} → {c:?}"
            );
            last = c;
        }
    }

    #[test]
    fn a_long_flight_has_many_varied_shots() {
        // a kilometre over open ground
        let scene = crate::obstacles::Scene::default();
        let path: Vec<[f32; 3]> = (0..=400).map(|i| [i as f32 * 250.0, 0.0, 200.0]).collect();
        let s = AerialPlan::new(&path, &scene.blocking()).stretches();
        assert!((15..=40).contains(&s.len()), "{} shots in a kilometre", s.len());
        let kinds: std::collections::HashSet<_> = s.iter().map(|x| format!("{:?}", x.2)).collect();
        assert!(kinds.len() >= 6, "{kinds:?}");
        let lens: Vec<f32> = s.iter().map(|x| x.1 - x.0).collect();
        let (lo, hi) = lens[1..lens.len() - 1].iter().fold((f32::MAX, 0.0f32), |(a, b), &l| (a.min(l), b.max(l)));
        assert!(hi - lo > 800.0, "every shot as long: {lo}..{hi}");
        for w in s.windows(2) {
            assert_ne!(w[0].2, w[1].2, "the same look twice running");
        }
        let lifts: Vec<f32> =
            (0..=100).map(|k| AerialPlan::new(&path, &scene.blocking()).cue(k as f32 / 100.0, 0.0).0.lift).collect();
        let (lo, hi) = lifts.iter().fold((f32::MAX, 0.0f32), |(a, b), &l| (a.min(l), b.max(l)));
        assert!(hi - lo > 1000.0, "the height hardly changes: {lo}..{hi}");
    }
}
