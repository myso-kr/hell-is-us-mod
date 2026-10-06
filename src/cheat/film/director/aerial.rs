//! The director on a flight (the camera alone, the land as the subject): the flight's path read as
//! a walk's is, cut into stretches, each looked at as an aerial film would — gliding with the way,
//! a rising reveal, diving, leaning into a turn, looking straight down over high ground, pushing in
//! at the end (.spec/FILMING-RESEARCH.md §2, drones).

use super::read::{self, Sense};
use super::runs::{ahead_of, merge, runs, Run};
use crate::obstacles::Blocking;

/// How a flight looks at a stretch of the land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aerial {
    /// Level with the way, a little down: the land going by.
    Glide,
    /// Starting low and tilted down, the horizon coming up as the camera climbs (a reveal).
    Rise,
    /// Tilted down after the land as it falls away.
    Dive,
    /// Looking into a turn before the path takes it.
    Bank,
    /// Straight down over high open ground.
    Overhead,
    /// The end: tilting down onto it, the lens closing in.
    Arrive,
}

impl Aerial {
    pub fn label(self) -> &'static str {
        match self {
            Aerial::Glide => tr!("AERIAL_GLIDE"),
            Aerial::Rise => tr!("AERIAL_RISE"),
            Aerial::Dive => tr!("AERIAL_DIVE"),
            Aerial::Bank => tr!("AERIAL_BANK"),
            Aerial::Overhead => tr!("AERIAL_OVERHEAD"),
            Aerial::Arrive => tr!("AERIAL_ARRIVE"),
        }
    }

    /// Its look `u` of the way through it (0–1), turning `turn` (+1 left): (pitch, yaw from the way
    /// ahead, field of view), degrees.
    fn look(self, u: f32, turn: f32) -> (f32, f32, f32) {
        let u = u.clamp(0.0, 1.0);
        let e = u * u * (3.0 - 2.0 * u);
        let lerp = |a: f32, b: f32| a + (b - a) * e;
        match self {
            Aerial::Glide => (-8.0, 0.0, 74.0),
            Aerial::Rise => (lerp(-40.0, -6.0), 0.0, lerp(68.0, 82.0)),
            Aerial::Dive => (lerp(-18.0, -34.0), 0.0, 72.0),
            Aerial::Bank => (-12.0, 22.0 * turn, 76.0),
            Aerial::Overhead => (lerp(-35.0, -60.0), 0.0, lerp(74.0, 80.0)),
            Aerial::Arrive => (lerp(-10.0, -32.0), 0.0, lerp(76.0, 58.0)),
        }
    }
}

/// How long the opening rise and the closing push are (cm), the shortest stretch (cm, ~3 s at a
/// flight's 450 cm/s), the rise and fall that count (cm over 10 m), and the turn (degrees).
const OPENING: f32 = 1500.0;
const CLOSING: f32 = 1500.0;
const MIN_STRETCH: f32 = 1400.0;
const CLIMB: f32 = 250.0;
const FALL: f32 = -250.0;
const TURN: f32 = 50.0;
/// How far before a stretch's end the next look starts to take over (cm).
const ANTICIPATE: f32 = 700.0;

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
        AerialPlan { stretches, length }
    }

    /// The look `share` of the way along (0–1): (pitch, yaw, field of view) with the way ahead at
    /// `ahead_yaw`, eased into the next stretch's ahead of it; and the stretch's look.
    pub fn cue(&self, share: f32, ahead_yaw: f32) -> (f32, f32, f32, Aerial) {
        let along = share * self.length;
        let Some(i) = self.stretches.iter().position(|r| along < r.to).or(self.stretches.len().checked_sub(1)) else {
            let (p, y, f) = Aerial::Glide.look(0.0, 0.0);
            return (p, ahead_yaw + y, f, Aerial::Glide);
        };
        let r = self.stretches[i];
        let u = (along - r.from) / (r.to - r.from).max(1.0);
        let (mut p, mut y, mut f) = r.what.look(u, r.turn);
        let mut what = r.what;
        if let Some(n) = self.stretches.get(i + 1) {
            let k = ahead_of(along, r.to, ANTICIPATE);
            let (np, ny, nf) = n.what.look(0.0, n.turn);
            p += (np - p) * k;
            y += crate::film::wrap(ny - y) * k;
            f += (nf - f) * k;
            if k > 0.5 {
                what = n.what;
            }
        }
        (p, crate::film::wrap(ahead_yaw + y), f, what)
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
        let mut last = plan.cue(0.0, 0.0);
        for k in 1..=1000 {
            let c = plan.cue(k as f32 / 1000.0, 0.0);
            assert!((c.0 - last.0).abs() < 2.0 && (c.2 - last.2).abs() < 2.0, "a jump at {k}: {last:?} → {c:?}");
            last = c;
        }
    }
}
