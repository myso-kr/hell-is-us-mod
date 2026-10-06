//! The shots the director chooses from, and the rig each one holds as it plays (.spec/
//! FILMING-RESEARCH.md §2–4): where the camera is about the hero, how far, through what lens.

/// Where the camera is for a shot, about the hero and the way it goes: its azimuth (degrees from
/// the way; 0 in front of the hero looking back at it, 180 behind it, 90 to its left), its
/// elevation (degrees above the hero's head; below it, looking up), its distance (cm), its field
/// of view (degrees), how far ahead of the hero it looks (lead room, cm), and how high above the
/// hero's root the point it turns about is (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rig {
    pub az: f32,
    pub el: f32,
    pub dist: f32,
    pub fov: f32,
    pub ahead: f32,
    pub lift: f32,
}

impl Rig {
    /// Between `self` (k = 0) and `to` (k = 1): angles the short way round.
    pub fn mix(self, to: Rig, k: f32) -> Rig {
        let lerp = |a: f32, b: f32| a + (b - a) * k;
        Rig {
            az: self.az + crate::film::wrap(to.az - self.az) * k,
            el: lerp(self.el, to.el),
            dist: lerp(self.dist, to.dist),
            fov: lerp(self.fov, to.fov),
            ahead: lerp(self.ahead, to.ahead),
            lift: lerp(self.lift, to.lift),
        }
    }
}

/// A shot, each from film, television or drone practice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shot {
    /// The start in the open: from high behind, down to the follow (crane down, arrival).
    CraneDown,
    /// The start indoors: a low push in to a close follow.
    PushIn,
    /// A long stretch in the open: alongside, on one side, rising slowly (truck with a boom up).
    SideTrack,
    /// A corridor: low and close behind, a wide lens (Steadicam).
    Steadicam,
    /// Behind at the game's own rest, the way ahead in view: what a stretch needs when it calls
    /// for nothing more (the long open stretch's middle, a short route).
    Follow,
    /// A turn: swinging round the hero through it (arc).
    Arc,
    /// Climbing: low, looking up, rising with the hero (pedestal up, low angle).
    LowRise,
    /// Going down: high, looking down, following it down (high angle, boom down).
    HighFall,
    /// High ground with a view: rising over the hero and pulling back (crane up reveal).
    CraneReveal,
    /// The end: in front of the hero, back and up together, the hero left small (dronie).
    Dronie,
}

impl Shot {
    /// Its rig `u` of the way through it (0–1), on `side` (+1 left, −1 right of the way).
    pub fn rig(self, u: f32, side: f32) -> Rig {
        let u = u.clamp(0.0, 1.0);
        let ease = u * u * (3.0 - 2.0 * u);
        let lerp = |a: f32, b: f32| a + (b - a) * ease;
        let base = Rig { az: 180.0, el: 14.0, dist: 520.0, fov: 70.0, ahead: 120.0, lift: 70.0 };
        match self {
            Shot::CraneDown => Rig { el: lerp(55.0, 16.0), dist: lerp(1400.0, 560.0), ahead: 150.0, ..base },
            Shot::PushIn => Rig { el: 8.0, dist: lerp(750.0, 360.0), fov: lerp(78.0, 68.0), ..base },
            // a rear three-quarter rather than a profile: where the hero goes stays in view, and
            // the camera stays within what the reading looked at (6 m out)
            Shot::SideTrack => {
                Rig { az: 180.0 - 75.0 * side, el: lerp(8.0, 26.0), dist: 580.0, fov: 60.0, ahead: 220.0, ..base }
            }
            Shot::Follow => Rig { el: 12.0, dist: 550.0, fov: 70.0, ahead: 150.0, ..base },
            Shot::Steadicam => Rig { el: 9.0, dist: 380.0, fov: 82.0, ahead: 140.0, lift: 60.0, ..base },
            // round the outside of the turn (+1 is left, as everywhere), restrained
            Shot::Arc => Rig { az: lerp(180.0, 180.0 - 40.0 * side), el: 16.0, dist: 560.0, ..base },
            Shot::LowRise => Rig {
                az: 180.0 - 30.0 * side,
                el: lerp(-8.0, -2.0),
                dist: 480.0,
                fov: 76.0,
                ahead: 60.0,
                lift: lerp(60.0, 110.0),
            },
            Shot::HighFall => Rig { el: lerp(30.0, 48.0), dist: lerp(600.0, 760.0), fov: 64.0, ahead: 200.0, ..base },
            Shot::CraneReveal => Rig {
                az: lerp(180.0, 180.0 - 25.0 * side),
                el: lerp(14.0, 50.0),
                dist: lerp(560.0, 1350.0),
                fov: lerp(70.0, 80.0),
                ahead: lerp(150.0, 600.0),
                ..base
            },
            // from behind, rising and pulling back: the hero left small in the place (swinging to
            // the front was a 170° turn as the take ended)
            Shot::Dronie => {
                Rig { az: 180.0 - 20.0 * side, el: lerp(12.0, 42.0), dist: lerp(450.0, 1500.0), ahead: 0.0, ..base }
            }
        }
    }

    /// Its name, for the card while it plays.
    pub fn label(self) -> &'static str {
        match self {
            Shot::CraneDown => tr!("SHOT_CRANE_DOWN"),
            Shot::PushIn => tr!("SHOT_PUSH_IN"),
            Shot::SideTrack => tr!("SHOT_SIDE_TRACK"),
            Shot::Steadicam => tr!("SHOT_STEADICAM"),
            Shot::Follow => tr!("SHOT_FOLLOW"),
            Shot::Arc => tr!("SHOT_ARC"),
            Shot::LowRise => tr!("SHOT_LOW_RISE"),
            Shot::HighFall => tr!("SHOT_HIGH_FALL"),
            Shot::CraneReveal => tr!("SHOT_CRANE_REVEAL"),
            Shot::Dronie => tr!("SHOT_DRONIE"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_that_rise_and_pull_back_do_both_together() {
        for shot in [Shot::CraneReveal, Shot::Dronie] {
            let (a, b) = (shot.rig(0.0, 1.0), shot.rig(1.0, 1.0));
            assert!(b.el > a.el + 20.0 && b.dist > a.dist * 2.0, "{shot:?}");
        }
        let (a, b) = (Shot::CraneDown.rig(0.0, 1.0), Shot::CraneDown.rig(1.0, 1.0));
        assert!(a.el > b.el + 30.0 && a.dist > b.dist * 2.0, "comes down and in");
    }

    #[test]
    fn a_rig_mixes_the_short_way_round() {
        let a = Rig { az: 170.0, el: 0.0, dist: 100.0, fov: 60.0, ahead: 0.0, lift: 0.0 };
        let b = Rig { az: -170.0, ..a };
        assert!((crate::film::wrap(a.mix(b, 0.5).az) - 180.0).abs() < 0.01);
    }
}
