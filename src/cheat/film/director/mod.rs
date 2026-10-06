//! The director (`Lens::Director`): before a walk it reads the route and what stands about it,
//! cuts it into beats and gives each a shot (`read`, `beats`, `shot`); while it rolls, it gives
//! the camera's targets for where the hero is — the shot's rig eased into the next one ahead of
//! the change (anticipation), the camera raised and slid aside when something hides the hero.
//! Its route's shots are one continuous take; coverage (`coverage`) cuts among them and the shots
//! about the hero as the hero's situation changes (`situation`).

mod aerial;
mod beats;
mod coverage;
mod glance;
mod read;
mod runs;
mod shot;
mod situation;

pub use aerial::{Aerial, AerialPlan};
pub use beats::Beat;
pub use coverage::{Angle, Coverage, Ctx, Cuts};
pub use glance::{Glance, GlanceLook};
pub use shot::{Rig, Shot};
pub use situation::{judge, openness, Motion, Sight, Situation};

use crate::obstacles::Blocking;

/// How far before a beat's end the next shot starts to take over (cm, ~1.5 s at a walk), how fast
/// the camera rises when the hero is hidden and settles back after (degrees a second), and how
/// high it may rise for it (degrees).
const ANTICIPATE: f32 = 550.0;
const HIDDEN_RISE: f32 = 25.0;
const HIDDEN_SETTLE: f32 = 12.0;
const HIDDEN_MOST: f32 = 35.0;
/// How long the hero must be in sight again before the camera settles back (s): a pillar passing
/// does not make it bob.
const HIDDEN_HOLD: f32 = 0.5;
/// The most the camera looks down (degrees): the game's own limit is 70.
const MOST_DOWN: f32 = 65.0;

/// What the camera should do now: its pitch and yaw (Unreal degrees), the point it turns about in
/// the hero's frame (cm), its distance (cm) and field of view (degrees), the shot's name, and
/// whether to cut to it (else eased).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub pitch: f32,
    pub yaw: f32,
    pub pivot: [f32; 3],
    pub distance: f32,
    pub fov: f32,
    pub label: &'static str,
    pub cut: bool,
}

pub struct Director {
    beats: Vec<Beat>,
    /// How much the camera is raised now to see the hero past something (degrees), and how long
    /// the hero has been in sight since (s).
    raised: f32,
    clear: f32,
}

impl Director {
    /// The director for `route`, reading the obstacles about it.
    pub fn new(route: &[[f32; 3]], b: &Blocking) -> Director {
        let senses = read::read(route, b);
        let length = senses.last().map_or(0.0, |s| s.at);
        Director { beats: beats::plan(&senses, length), raised: 0.0, clear: 0.0 }
    }

    pub fn beats(&self) -> &[Beat] {
        &self.beats
    }

    /// The rig `along` cm into the route, and the shot that owns it: the beat's own, blended into
    /// the next over the last `ANTICIPATE` of it.
    pub fn rig(&self, along: f32) -> (Rig, Shot) {
        let Some(i) = self.beats.iter().position(|b| along < b.to).or(self.beats.len().checked_sub(1)) else {
            return (Shot::Follow.rig(0.0, 1.0), Shot::Follow);
        };
        let b = self.beats[i];
        let u = ((along - b.from) / (b.to - b.from).max(1.0)).clamp(0.0, 1.0);
        let here = b.shot.rig(u, b.side);
        match self.beats.get(i + 1) {
            Some(n) if b.to - along < ANTICIPATE => {
                let k = runs::ahead_of(along, b.to, ANTICIPATE);
                (here.mix(n.shot.rig(0.0, n.side), k), if k > 0.5 { n.shot } else { b.shot })
            }
            _ => (here, b.shot),
        }
    }

    /// The camera's cue for the hero at `hero` (its root, cm), `along` the route, going `heading`
    /// (degrees): the rig turned into the camera's numbers, raised while the obstacles hide the
    /// hero from where the camera would be.
    pub fn cue(&mut self, along: f32, heading: f32, hero: [f32; 3], b: &Blocking, dt: f32) -> Cue {
        let (rig, shot) = self.rig(along);
        self.cue_rig(rig, shot, heading, hero, b, dt)
    }

    /// The cue for a hero off the route (a player gone their own way): behind them as they go,
    /// raised as ever when something hides them.
    pub fn cue_follow(&mut self, heading: f32, hero: [f32; 3], b: &Blocking, dt: f32) -> Cue {
        self.cue_rig(Shot::Follow.rig(0.0, 1.0), Shot::Follow, heading, hero, b, dt)
    }

    fn cue_rig(&mut self, rig: Rig, shot: Shot, heading: f32, hero: [f32; 3], b: &Blocking, dt: f32) -> Cue {
        // from where the camera would be (azimuth about the hero, its elevation, its distance)
        let place = |el: f32| {
            let (a, e) = ((heading + rig.az).to_radians(), el.to_radians());
            let head = [hero[0], hero[1], hero[2] + rig.lift];
            [
                head[0] + a.cos() * e.cos() * rig.dist,
                head[1] + a.sin() * e.cos() * rig.dist,
                head[2] + e.sin() * rig.dist,
            ]
        };
        let eye = [hero[0], hero[1], hero[2] + 60.0];
        let hidden = b.blocks(place(rig.el + self.raised), eye);
        if hidden {
            self.clear = 0.0;
            self.raised = (self.raised + HIDDEN_RISE * dt).min(HIDDEN_MOST);
        } else {
            self.clear += dt;
            if self.clear > HIDDEN_HOLD {
                self.raised = (self.raised - HIDDEN_SETTLE * dt).max(0.0);
            }
        }
        Cue {
            // the camera looks back at the hero from its place: the azimuth turned round
            pitch: -(rig.el + self.raised).min(MOST_DOWN),
            yaw: crate::film::wrap(heading + rig.az + 180.0),
            pivot: [rig.ahead, 0.0, rig.lift],
            distance: rig.dist,
            fov: rig.fov,
            label: shot.label(),
            cut: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_take_flows_without_jumps() {
        let scene = crate::obstacles::Scene::default();
        let route: Vec<[f32; 3]> = (0..=40).map(|i| [i as f32 * 250.0, (i as f32 * 0.3).sin() * 300.0, 0.0]).collect();
        let d = Director::new(&route, &scene.blocking());
        assert!(d.beats().len() >= 3, "{:?}", d.beats());
        let mut last = d.rig(0.0).0;
        let mut s = 0.0;
        while s < 10_000.0 {
            let (r, _) = d.rig(s);
            // 10 cm of walk moves nothing far: no cut
            assert!(crate::film::wrap(r.az - last.az).abs() < 8.0, "azimuth jumps at {s}");
            assert!((r.dist - last.dist).abs() < 40.0, "distance jumps at {s}: {} → {}", last.dist, r.dist);
            assert!((r.el - last.el).abs() < 4.0, "elevation jumps at {s}");
            last = r;
            s += 10.0;
        }
    }

    #[test]
    fn a_hidden_hero_raises_the_camera() {
        // walls all round the hero, as tall as the camera would be: hidden from any side
        let wall = |x0: f32, x1: f32, y0: f32, y1: f32| crate::obstacles::Obstacle {
            hull: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            zmin: 0.0,
            zmax: 500.0,
            water: false,
        };
        let scene = crate::obstacles::Scene {
            obstacles: vec![
                wall(-400.0, -300.0, -800.0, 800.0),
                wall(300.0, 400.0, -800.0, 800.0),
                wall(-800.0, 800.0, 300.0, 400.0),
                wall(-800.0, 800.0, -400.0, -300.0),
            ],
            ..Default::default()
        };
        let b = scene.blocking();
        let route: Vec<[f32; 3]> = (0..=40).map(|i| [i as f32 * 250.0, 0.0, 0.0]).collect();
        let mut d = Director::new(&route, &b);
        let mut c = d.cue(3000.0, 0.0, [0.0, 0.0, 0.0], &b, 0.05);
        for _ in 0..60 {
            c = d.cue(3000.0, 0.0, [0.0, 0.0, 0.0], &b, 0.05);
        }
        assert!(d.raised > 5.0, "not raised: {} ({:?})", d.raised, c);
    }
}
