//! Coverage: the shots the camera chooses from as the hero does things, shared by every kind of take
//! (.spec/FILMING-RESEARCH.md §5). After the games that cut a cinematic camera into play — RDR2's
//! cinematic mode (wide and close, bird's-eye, swooping), God of War's camera at the shoulder
//! (dropping low before something large), Cinemachine's state-driven camera and ClearShot (the
//! best unobstructed shot among candidates), its target groups (two subjects in frame), racing
//! replays' broadcast coverage: each situation has its candidate angles; the clearest, freshest
//! one is taken, held 4–8 s, and changed with a cut or a blend.

use super::shot::Rig;
use super::situation::{Sight, Situation};
use super::Cue;
use crate::film::wrap;
use crate::obstacles::Blocking;

/// How shots change: always eased, cut when the situation or the shot's size changes, or always cut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cuts {
    Smooth,
    #[default]
    Mixed,
    Cuts,
}

impl Cuts {
    pub const ALL: [Cuts; 3] = [Cuts::Smooth, Cuts::Mixed, Cuts::Cuts];

    pub fn label(self) -> &'static str {
        match self {
            Cuts::Smooth => tr!("FILM_CUTS_SMOOTH"),
            Cuts::Mixed => tr!("FILM_CUTS_MIXED"),
            Cuts::Cuts => tr!("FILM_CUTS_CUTS"),
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Cuts::Smooth => "smooth",
            Cuts::Mixed => "mixed",
            Cuts::Cuts => "cuts",
        }
    }

    pub fn from_word(w: &str) -> Option<Cuts> {
        Cuts::ALL.into_iter().find(|c| c.word() == w)
    }
}

/// The angles coverage takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Angle {
    /// Close at the right shoulder (God of War).
    Shoulder,
    Behind,
    LowChase,
    HighChase,
    /// Alongside.
    Profile,
    /// Ahead, looking back at the hero coming.
    Lead,
    Close,
    Wide,
    /// Looking down from high above (bird's-eye).
    Overhead,
    /// Sweeping round and down to the hero (RDR2).
    Swoop,
    /// Low, looking up at the hero.
    LowHero,
    /// Over the hero's shoulder at the one they face.
    OverShoulder,
    /// The hero and the one they face, both in frame, from the side.
    TwoShot,
    /// Over the shoulder at a thing to interact with, closing in.
    Focus,
    /// High behind the hero, the thing ahead and the place about it in frame.
    Reveal,
    /// A full circle round the hero standing.
    Orbit,
    /// The dolly zoom (Vertigo): the camera backing off as the lens closes in, the hero held, the
    /// world behind stretching.
    Vertigo,
    /// From close, pulling far back and up: the hero left small in the place (a dronie).
    Pullback,
    /// The camera passing the hero going the other way, turning to keep them.
    FlyBy,
    /// The route's planned shot (the director's beats, director/beats.rs).
    Planned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Size {
    Close,
    Medium,
    Wide,
}

impl Angle {
    fn size(self) -> Size {
        match self {
            Angle::Shoulder | Angle::Close | Angle::OverShoulder | Angle::Focus | Angle::LowHero => Size::Close,
            Angle::Wide | Angle::Overhead | Angle::TwoShot | Angle::HighChase | Angle::Reveal | Angle::Pullback => {
                Size::Wide
            }
            _ => Size::Medium,
        }
    }

    /// Behind the hero within ±35°: the player's controls stay as they are (camera-relative).
    fn keeps_controls(self) -> bool {
        matches!(self, Angle::Shoulder | Angle::Behind | Angle::LowChase | Angle::HighChase)
    }

    fn targeted(self) -> bool {
        matches!(self, Angle::OverShoulder | Angle::TwoShot | Angle::Focus | Angle::Reveal)
    }

    pub fn label(self) -> &'static str {
        match self {
            Angle::Shoulder => tr!("COVER_SHOULDER"),
            Angle::Behind => tr!("COVER_BEHIND"),
            Angle::LowChase => tr!("COVER_LOW_CHASE"),
            Angle::HighChase => tr!("COVER_HIGH_CHASE"),
            Angle::Profile => tr!("COVER_PROFILE"),
            Angle::Lead => tr!("COVER_LEAD"),
            Angle::Close => tr!("COVER_CLOSE"),
            Angle::Wide => tr!("COVER_WIDE"),
            Angle::Overhead => tr!("COVER_OVERHEAD"),
            Angle::Swoop => tr!("COVER_SWOOP"),
            Angle::LowHero => tr!("COVER_LOW_HERO"),
            Angle::OverShoulder => tr!("COVER_OVER_SHOULDER"),
            Angle::TwoShot => tr!("COVER_TWO_SHOT"),
            Angle::Focus => tr!("COVER_FOCUS"),
            Angle::Reveal => tr!("COVER_REVEAL"),
            Angle::Orbit => tr!("COVER_ORBIT"),
            Angle::Vertigo => tr!("COVER_VERTIGO"),
            Angle::Pullback => tr!("COVER_PULLBACK"),
            Angle::FlyBy => tr!("COVER_FLY_BY"),
            Angle::Planned => "",
        }
    }

    /// Its rig `u` of the way through it, on `side` (+1 left), about the way the hero goes.
    fn rig(self, u: f32, side: f32) -> Rig {
        let e = {
            let u = u.clamp(0.0, 1.0);
            u * u * (3.0 - 2.0 * u)
        };
        let lerp = |a: f32, b: f32| a + (b - a) * e;
        let r = Rig { az: 180.0, el: 12.0, dist: 520.0, fov: 70.0, ahead: 150.0, lift: 70.0 };
        match self {
            Angle::Shoulder => Rig { az: 195.0, el: 8.0, dist: 270.0, ahead: 120.0, lift: 80.0, ..r },
            Angle::Behind => r,
            Angle::LowChase => Rig { az: 180.0 - 20.0 * side, el: -4.0, dist: 400.0, fov: 78.0, lift: 50.0, ..r },
            Angle::HighChase => Rig { el: 32.0, dist: 760.0, fov: 66.0, ahead: 250.0, ..r },
            Angle::Profile => Rig { az: 180.0 - 80.0 * side, el: 6.0, dist: 420.0, fov: 62.0, ahead: 100.0, ..r },
            Angle::Lead => Rig { az: 20.0 * side, el: 8.0, dist: lerp(600.0, 450.0), fov: 64.0, ahead: 0.0, ..r },
            Angle::Close => Rig { az: 180.0 - 130.0 * side, el: 4.0, dist: 230.0, fov: 55.0, ahead: 0.0, lift: 90.0 },
            Angle::Wide => Rig { az: 180.0 - 40.0 * side, el: 20.0, dist: 1300.0, fov: 78.0, ..r },
            Angle::Overhead => Rig { el: lerp(55.0, 62.0), dist: 900.0, fov: 72.0, ahead: 0.0, ..r },
            Angle::Swoop => Rig {
                az: lerp(180.0 - 60.0 * side, 180.0 + 20.0 * side),
                el: lerp(32.0, 10.0),
                dist: lerp(950.0, 480.0),
                ..r
            },
            Angle::LowHero => {
                Rig { az: 180.0 - 35.0 * side, el: -12.0, dist: 380.0, fov: 74.0, ahead: 60.0, lift: 40.0 }
            }
            Angle::Orbit => Rig { az: 180.0 + 360.0 * e * side, el: 14.0, dist: 480.0, fov: 66.0, ahead: 0.0, ..r },
            // the hero's size held: distance × tan(fov/2) kept as the lens closes
            Angle::Vertigo => {
                let fov: f32 = lerp(78.0, 34.0);
                let dist = 300.0 * (39.0f32).to_radians().tan() / (fov / 2.0).to_radians().tan();
                Rig { az: 180.0 - 25.0 * side, el: 6.0, dist, fov, ahead: 0.0, lift: 100.0 }
            }
            Angle::Pullback => Rig {
                az: 180.0 - 30.0 * side,
                el: lerp(8.0, 42.0),
                dist: lerp(260.0, 1500.0),
                fov: lerp(60.0, 78.0),
                ahead: 0.0,
                ..r
            },
            Angle::FlyBy => Rig { az: lerp(40.0, 150.0) * side, el: 8.0, dist: 420.0, fov: 66.0, ahead: 0.0, ..r },
            // the targeted ones are placed about their target (`Coverage::cue`)
            Angle::OverShoulder | Angle::TwoShot | Angle::Focus | Angle::Reveal | Angle::Planned => r,
        }
    }
}

/// The candidates for a situation, the likeliest first.
fn candidates(s: Situation) -> &'static [Angle] {
    match s {
        Situation::Moving => &[
            Angle::Planned,
            Angle::Shoulder,
            Angle::Behind,
            Angle::LowChase,
            Angle::HighChase,
            Angle::Profile,
            Angle::Lead,
            Angle::Swoop,
            Angle::FlyBy,
        ],
        Situation::Turning => &[Angle::Planned, Angle::Behind, Angle::Swoop, Angle::HighChase, Angle::Shoulder],
        Situation::Climbing => &[Angle::Planned, Angle::LowHero, Angle::LowChase, Angle::Behind],
        Situation::Descending => &[Angle::Planned, Angle::HighChase, Angle::Overhead, Angle::Behind],
        Situation::Still => &[
            Angle::Close,
            Angle::Orbit,
            Angle::Profile,
            Angle::Wide,
            Angle::Vertigo,
            Angle::Swoop,
            Angle::Pullback,
            Angle::LowHero,
            Angle::Overhead,
            Angle::Shoulder,
        ],
        Situation::Combat => &[Angle::OverShoulder, Angle::TwoShot, Angle::LowChase, Angle::Shoulder],
        Situation::Meeting => &[Angle::TwoShot, Angle::OverShoulder, Angle::Profile, Angle::Orbit],
        Situation::Find => &[Angle::Focus, Angle::OverShoulder, Angle::Vertigo],
        Situation::Landmark => &[Angle::Reveal, Angle::Focus, Angle::Pullback, Angle::TwoShot],
    }
}

/// How long a shot holds at least, and how much longer at most (s); how long a new situation must
/// last before the camera answers it (s; a fight at once); how far a cut must move the camera
/// (degrees) not to read as a jump; how fast and how high the camera rises over what hides the
/// hero, and how long it waits before settling back.
const HOLD: f32 = 4.0;
const HOLD_MORE: f32 = 4.0;
const SETTLE: f32 = 0.4;
const NO_JUMP: f32 = 30.0;
const HIDDEN_RISE: f32 = 25.0;
const HIDDEN_SETTLE: f32 = 12.0;
const HIDDEN_MOST: f32 = 35.0;
const HIDDEN_HOLD: f32 = 0.5;
const MOST_DOWN: f32 = 65.0;

/// What a take gives coverage each tick.
pub struct Ctx<'a> {
    pub hero: [f32; 3],
    /// The way the hero goes (degrees), and the way its body faces (the pivot's frame).
    pub heading: f32,
    pub body: f32,
    pub sight: Sight,
    /// The route's planned shot here, with its name, if the take has a route and the hero is on it.
    pub planned: Option<(Rig, &'static str)>,
    /// The player plays (keep the controls as they are while the hero moves).
    pub live: bool,
    pub b: &'a Blocking<'a>,
    pub cuts: Cuts,
    pub dt: f32,
}

pub struct Coverage {
    angle: Angle,
    side: f32,
    t: f32,
    started: f32,
    hold: f32,
    situation: Situation,
    /// A new situation not yet answered, since when.
    coming: Option<(Situation, f32)>,
    recent: [Option<Angle>; 3],
    raised: f32,
    clear: f32,
    /// The camera's last cue (for the jump-cut rule).
    last: Option<(f32, f32)>,
}

impl Default for Coverage {
    fn default() -> Coverage {
        Coverage {
            angle: Angle::Planned,
            side: 1.0,
            t: 0.0,
            started: 0.0,
            hold: 0.0,
            situation: Situation::Still,
            coming: None,
            recent: [None; 3],
            raised: 0.0,
            clear: 0.0,
            last: None,
        }
    }
}

/// A small, steady variety: the same moment always gives the same choice (no randomness to chase).
fn jitter(t: f32, k: u32) -> f32 {
    let x = (t as u32).wrapping_mul(2_654_435_761).wrapping_add(k.wrapping_mul(40_503));
    (x >> 16) as f32 / 65_535.0
}

impl Coverage {
    /// The cue for this tick.
    pub fn cue(&mut self, c: &Ctx) -> Cue {
        self.t += c.dt;
        let moving = matches!(
            c.sight.situation,
            Situation::Moving | Situation::Turning | Situation::Climbing | Situation::Descending
        );
        // a new situation, once it has lasted (a fight at once)
        let mut changed = false;
        if c.sight.situation != self.situation {
            let since = match self.coming {
                Some((s, at)) if s == c.sight.situation => at,
                _ => {
                    self.coming = Some((c.sight.situation, self.t));
                    self.t
                }
            };
            if c.sight.situation == Situation::Combat || self.t - since >= SETTLE {
                self.situation = c.sight.situation;
                self.coming = None;
                changed = true;
            }
        } else {
            self.coming = None;
        }
        let due = self.t - self.started >= self.hold;
        // the shot no longer fits (moving with the controls to keep; a target gone)
        let unfit = (c.live && moving && !self.fits_controls(c))
            || (self.angle.targeted() && c.sight.target.is_none())
            || (self.angle == Angle::Planned && c.planned.is_none());
        let mut cut = false;
        if changed || due || unfit {
            let before = self.angle;
            self.angle = self.choose(c, moving);
            self.started = self.t;
            self.hold = HOLD + HOLD_MORE * jitter(self.t, 7);
            self.recent = [Some(before), self.recent[0], self.recent[1]];
            cut = match c.cuts {
                Cuts::Smooth => false,
                Cuts::Cuts => true,
                Cuts::Mixed => changed || before.size() != self.angle.size(),
            };
        }
        let u = (self.t - self.started) / self.hold.max(1.0);
        let mut cue = self.place(c, u);
        // a cut that would barely move the camera reads as a jump: eased instead
        if cut {
            if let Some((p, y)) = self.last {
                if wrap(cue.yaw - y).abs() < NO_JUMP && (cue.pitch - p).abs() < NO_JUMP / 2.0 {
                    cut = false;
                }
            }
        }
        cue.cut = cut;
        self.last = Some((cue.pitch, cue.yaw));
        cue
    }

    fn fits_controls(&self, c: &Ctx) -> bool {
        match self.angle {
            Angle::Planned => c.planned.is_some_and(|(r, _)| wrap(r.az - 180.0).abs() <= 35.0),
            a => a.keeps_controls() || a.targeted(),
        }
    }

    /// The best angle for the situation: in sight of the hero, not one of the last few, a change
    /// of size, the route's plan preferred while going along it.
    fn choose(&mut self, c: &Ctx, moving: bool) -> Angle {
        let list = candidates(self.situation);
        let mut best = (f32::MIN, self.angle);
        for (i, &a) in list.iter().enumerate() {
            if a == Angle::Planned && c.planned.is_none() {
                continue;
            }
            if a.targeted() && c.sight.target.is_none() {
                continue;
            }
            if c.live && moving && !(a.keeps_controls() || a.targeted() || a == Angle::Planned) {
                continue;
            }
            if c.live
                && moving
                && a == Angle::Planned
                && !c.planned.is_some_and(|(r, _)| wrap(r.az - 180.0).abs() <= 35.0)
            {
                continue;
            }
            let mut score = 10.0 - i as f32;
            if self.recent.contains(&Some(a)) {
                score -= 6.0;
            }
            if a.size() != self.angle.size() {
                score += 3.0;
            }
            // an open place asks for the wide shots, a walled one for the close ones
            let open = (c.sight.open - 0.5) * 8.0;
            match a.size() {
                Size::Wide => score += open,
                Size::Close => score -= open,
                Size::Medium => {}
            }
            score += 4.0 * jitter(self.t, i as u32);
            // ClearShot: a shot that cannot see the hero is no shot
            let probe = self.place_angle(a, c, 0.0);
            let cam = camera_at(c.hero, probe.yaw, probe.pitch, probe.distance, probe.world_pivot);
            if c.b.blocks(cam, [c.hero[0], c.hero[1], c.hero[2] + 60.0]) {
                score -= 20.0;
            }
            if score > best.0 {
                best = (score, a);
            }
        }
        // the line's side kept, but changed now and then on a cut (a continuous move may cross it)
        if jitter(self.t, 99) > 0.8 {
            self.side = -self.side;
        }
        best.1
    }

    /// The cue for the angle playing `u` of the way through, the camera raised while the hero is
    /// hidden.
    fn place(&mut self, c: &Ctx, u: f32) -> Cue {
        let p = self.place_angle(self.angle, c, u);
        let cam = camera_at(c.hero, p.yaw, p.pitch - self.raised, p.distance, p.world_pivot);
        let hidden = c.b.blocks(cam, [c.hero[0], c.hero[1], c.hero[2] + 60.0]);
        if hidden {
            self.clear = 0.0;
            self.raised = (self.raised + HIDDEN_RISE * c.dt).min(HIDDEN_MOST);
        } else {
            self.clear += c.dt;
            if self.clear > HIDDEN_HOLD {
                self.raised = (self.raised - HIDDEN_SETTLE * c.dt).max(0.0);
            }
        }
        // the pivot into the hero's frame (the body's yaw undone)
        let (s, co) = c.body.to_radians().sin_cos();
        let (dx, dy) = (p.world_pivot[0], p.world_pivot[1]);
        Cue {
            pitch: (p.pitch - self.raised).max(-MOST_DOWN),
            yaw: p.yaw,
            pivot: [dx * co + dy * s, -dx * s + dy * co, p.world_pivot[2]],
            distance: p.distance,
            fov: p.fov,
            label: p.label,
            cut: false,
        }
    }

    fn place_angle(&self, a: Angle, c: &Ctx, u: f32) -> Placed {
        let bearing = |to: [f32; 3]| (to[1] - c.hero[1]).atan2(to[0] - c.hero[0]).to_degrees();
        match (a, c.sight.target) {
            // over the shoulder at the target: the camera behind the hero across from it
            (Angle::OverShoulder | Angle::Focus, Some(t)) => {
                let dir = bearing(t);
                let (px, py) = perp(dir, self.side * -55.0);
                let (el, dist, fov) = if a == Angle::Focus {
                    (12.0, 520.0 + (380.0 - 520.0) * u.clamp(0.0, 1.0), 58.0)
                } else {
                    (8.0, 380.0, 72.0)
                };
                Placed { yaw: dir, pitch: -el, distance: dist, fov, world_pivot: [px, py, 80.0], label: a.label() }
            }
            // high behind the hero, the thing ahead: the hero, the thing and the place about them
            (Angle::Reveal, Some(t)) => {
                let dir = bearing(t);
                let mid = [(t[0] - c.hero[0]) * 0.4, (t[1] - c.hero[1]) * 0.4, 60.0];
                let rise = 22.0 + 10.0 * u.clamp(0.0, 1.0);
                Placed { yaw: dir, pitch: -rise, distance: 900.0, fov: 74.0, world_pivot: mid, label: a.label() }
            }
            // both in frame from the side of the line between them
            (Angle::TwoShot, Some(t)) => {
                let dir = bearing(t);
                let mid = [(t[0] - c.hero[0]) / 2.0, (t[1] - c.hero[1]) / 2.0, 80.0];
                let sep = (t[0] - c.hero[0]).hypot(t[1] - c.hero[1]);
                Placed {
                    yaw: wrap(dir - 90.0 * self.side),
                    pitch: -18.0,
                    distance: sep * 0.8 + 500.0,
                    fov: 70.0,
                    world_pivot: mid,
                    label: a.label(),
                }
            }
            _ => {
                let (rig, label) = match (a, c.planned) {
                    (Angle::Planned, Some((r, l))) => (r, l),
                    _ => (a.rig(u, self.side), a.label()),
                };
                let h = c.heading.to_radians();
                Placed {
                    yaw: wrap(c.heading + rig.az + 180.0),
                    pitch: -rig.el,
                    distance: rig.dist,
                    fov: rig.fov,
                    world_pivot: [h.cos() * rig.ahead, h.sin() * rig.ahead, rig.lift],
                    label,
                }
            }
        }
    }
}

/// A shot placed: the camera's yaw and pitch, its distance and lens, the pivot from the hero's root
/// in the world's axes (cm), its name.
struct Placed {
    yaw: f32,
    pitch: f32,
    distance: f32,
    fov: f32,
    world_pivot: [f32; 3],
    label: &'static str,
}

/// `len` cm to the left of the way `deg` (negative: right), as (x, y).
fn perp(deg: f32, len: f32) -> (f32, f32) {
    let h = deg.to_radians();
    (-h.sin() * len, h.cos() * len)
}

/// Where the camera is: behind the pivot along its view.
fn camera_at(hero: [f32; 3], yaw: f32, pitch: f32, dist: f32, pivot: [f32; 3]) -> [f32; 3] {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    let at = [hero[0] + pivot[0], hero[1] + pivot[1], hero[2] + pivot[2]];
    [at[0] - y.cos() * p.cos() * dist, at[1] - y.sin() * p.cos() * dist, at[2] - p.sin() * dist]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::film::director::situation::{Sight, Situation};

    fn ctx<'a>(b: &'a Blocking<'a>, s: Situation, live: bool, cuts: Cuts) -> Ctx<'a> {
        Ctx {
            hero: [0.0; 3],
            heading: 0.0,
            body: 0.0,
            sight: Sight { situation: s, target: None, open: 0.5 },
            planned: None,
            live,
            b,
            cuts,
            dt: 0.1,
        }
    }

    #[test]
    fn a_live_take_keeps_the_controls_while_moving() {
        let scene = crate::obstacles::Scene::default();
        let b = scene.blocking();
        let mut cov = Coverage::default();
        for _ in 0..2000 {
            let cue = cov.cue(&ctx(&b, Situation::Moving, true, Cuts::Mixed));
            // the camera behind within 35°: it looks the way the hero goes, ±35°
            assert!(wrap(cue.yaw).abs() <= 35.5, "{:?} looks {}", cov.angle, cue.yaw);
        }
    }

    #[test]
    fn shots_change_and_cut_when_asked() {
        let scene = crate::obstacles::Scene::default();
        let b = scene.blocking();
        let mut cov = Coverage::default();
        let mut seen = std::collections::HashSet::new();
        let mut cuts = 0;
        for _ in 0..3000 {
            let cue = cov.cue(&ctx(&b, Situation::Still, false, Cuts::Cuts));
            seen.insert(cov.angle);
            cuts += cue.cut as u32;
        }
        assert!(seen.len() >= 4, "too few angles: {seen:?}");
        assert!(cuts >= 10, "{cuts} cuts in 300 s");
        let mut cov = Coverage::default();
        let smooth = (0..3000).filter(|_| cov.cue(&ctx(&b, Situation::Still, false, Cuts::Smooth)).cut).count();
        assert_eq!(smooth, 0);
    }
}
