//! A flight's glance: something passed near is looked at for a moment, then the flight's own look
//! again; not too often. Soaring as an eagle, the camera hunts: it looks much further, and the
//! lens closes right in on what it finds.

use super::Cuts;
use crate::film::Subject;

/// How near a thing must be to be glanced at, for how long, and how often at most (cm, s, s); as
/// an eagle, how far it sees, how long it stares, how often, and how close its lens goes (degrees).
const NEAR: f32 = 2500.0;
const FOR: f32 = 3.0;
const EVERY: f32 = 10.0;
const HUNT_NEAR: f32 = 5000.0;
const HUNT_FOR: f32 = 3.5;
const HUNT_EVERY: f32 = 6.0;
const HUNT_FOV: f32 = 30.0;

/// Where to look while glancing: pitch, yaw, the lens if it changes, whether to cut to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlanceLook {
    pub pitch: f32,
    pub yaw: f32,
    pub fov: Option<f32>,
    pub cut: bool,
}

#[derive(Default)]
pub struct Glance {
    /// What is looked at, since when, and whether hunting.
    on: Option<([f32; 3], f32, bool)>,
    t: f32,
    last: f32,
}

impl Glance {
    pub fn on(&self) -> bool {
        self.on.is_some()
    }

    pub fn hunting(&self) -> bool {
        self.on.is_some_and(|o| o.2)
    }

    /// From the camera at `from`, among `subjects`, `hunt` as an eagle: where to look, while
    /// glancing.
    pub fn look(
        &mut self,
        from: [f32; 3],
        subjects: &[Subject],
        dt: f32,
        cuts: Cuts,
        hunt: bool,
    ) -> Option<GlanceLook> {
        self.t += dt;
        if let Some((at, since, h)) = self.on {
            if self.t - since > if h { HUNT_FOR } else { FOR } {
                self.on = None;
                self.last = self.t;
            } else {
                return Some(aim_at(from, at, h, false));
            }
        }
        let (near, every) = if hunt { (HUNT_NEAR, HUNT_EVERY) } else { (NEAR, EVERY) };
        if self.last > 0.0 && self.t - self.last < every {
            return None;
        }
        let d = |s: &Subject| (s.at[0] - from[0]).hypot(s.at[1] - from[1]);
        // the eagle takes what is furthest it can still see; a glance, the nearest
        let pick = subjects.iter().filter(|s| d(s) < near);
        let found =
            if hunt { pick.max_by(|a, b| d(a).total_cmp(&d(b))) } else { pick.min_by(|a, b| d(a).total_cmp(&d(b))) }?;
        self.on = Some((found.at, self.t, hunt));
        Some(aim_at(from, found.at, hunt, cuts != Cuts::Smooth))
    }
}

fn aim_at(from: [f32; 3], at: [f32; 3], hunt: bool, cut: bool) -> GlanceLook {
    let (dx, dy, dz) = (at[0] - from[0], at[1] - from[1], at[2] + 80.0 - from[2]);
    GlanceLook {
        pitch: dz.atan2(dx.hypot(dy)).to_degrees().clamp(-65.0, 30.0),
        yaw: dy.atan2(dx).to_degrees(),
        fov: hunt.then_some(HUNT_FOV),
        cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::film::SubjectKind;

    #[test]
    fn the_eagle_hunts_further_and_closer() {
        let far = [Subject { kind: SubjectKind::Npc, at: [4000.0, 0.0, 0.0] }];
        let mut g = Glance::default();
        assert!(g.look([0.0, 0.0, 2500.0], &far, 0.1, Cuts::Mixed, false).is_none(), "too far for a glance");
        let l = g.look([0.0, 0.0, 2500.0], &far, 0.1, Cuts::Mixed, true).unwrap();
        assert_eq!(l.fov, Some(HUNT_FOV));
        assert!(l.pitch < -20.0, "looks down at it: {}", l.pitch);
        assert!(g.hunting());
    }
}
