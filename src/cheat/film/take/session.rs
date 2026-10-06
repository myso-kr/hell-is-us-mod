//! What every kind of take shares while it rolls: the game opened for writing, the gimbal, the
//! camera's settings it holds and the room behind the camera, the obstacles, and what it changed
//! to put back.

use super::knobs::{ease_knobs, Knob, BLEND};
use super::{Plan, State, Wiring};
use crate::film::gimbal::back_of;
use crate::film::{wrap, Axis, Lens, Mode, FLIGHT_DISTANCE, ORBIT_DPS};
use crate::game::process::Game;
use crate::mem::Memory;
use crate::obstacles::Blocking;

/// What a camera setting is, for which a take holds it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Distance,
    Fov,
    Other,
}

pub(super) struct Session<'a> {
    pub game: Game,
    pub w: &'a Wiring,
    pub plan: &'a Plan,
    pub pitch: Axis,
    pub yaw: Axis,
    /// Circle's angle, turning `ORBIT_DPS` a second.
    pub orbit: f32,
    knobs: Vec<Knob>,
    /// The camera's room, kept between ticks: eased toward what is free.
    room: f32,
    pub blocking: Blocking<'a>,
    /// The camera mode's pivot as it was (its translation), and the safety checks' byte cleared
    /// on a flight: put back after.
    pub pivot0: Option<[f64; 3]>,
    safety0: Option<(u64, u8)>,
}

impl<'a> Session<'a> {
    pub fn open(plan: &'a Plan, w: &'a Wiring) -> Result<Session<'a>, State> {
        let game = match Game::find() {
            Ok(Some(g)) => g,
            Ok(None) => return Err(State::Failed(tr!("GAME_NOT_RUNNING").into())),
            Err(e) => return Err(State::Failed(e)),
        };
        let mut s = Session {
            game,
            w,
            plan,
            pitch: Axis::new(0.0),
            yaw: Axis::new(0.0),
            orbit: 0.0,
            knobs: Vec::new(),
            room: f32::MAX,
            blocking: w.scene.blocking(),
            pivot0: None,
            safety0: None,
        };
        let Some(rot0) = s.read3(w.rotation) else { return Err(State::Failed(tr!("ROTATION_UNREADABLE").into())) };
        (s.pitch, s.yaw) = (Axis::new(wrap(rot0[0] as f32)), Axis::new(wrap(rot0[1] as f32)));
        s.orbit = s.yaw.angle;
        let flight = plan.mode == Mode::Flight;
        // A flight keeps the camera close to its pivot unless asked otherwise; the distance is held
        // either way, kept within the room behind the camera (the game's own when none is asked);
        // the field of view too when the director moves it.
        let directed = plan.lens == Lens::Director;
        let distance = plan.distance.or(flight.then_some(FLIGHT_DISTANCE));
        let read_f = |game: &Game, at: u64| -> Option<f32> {
            let mut b = [0u8; 4];
            game.read(at, &mut b).then(|| f32::from_le_bytes(b)).filter(|v| v.is_finite())
        };
        s.knobs = w
            .distance
            .iter()
            .map(|&at| (at, distance, Kind::Distance))
            .chain(w.fov.iter().map(|&at| (at, plan.fov, Kind::Fov)))
            .chain(
                w.blend.iter().flat_map(|&(i, o)| [(i, Some(BLEND.0), Kind::Other), (o, Some(BLEND.1), Kind::Other)]),
            )
            .filter_map(|(at, want, kind)| {
                let was = read_f(&s.game, at)?;
                let held = kind == Kind::Distance || (kind == Kind::Fov && directed);
                let want = want.or(held.then_some(was))?;
                Some(Knob { at, was, now: was, want, distance: kind == Kind::Distance, fov: kind == Kind::Fov })
            })
            .collect();
        s.pivot0 = w.pivot.and_then(|at| s.read3(at));
        // On a flight the game's own checks from the hero to the camera's pivot are off: they held
        // the camera on the hero's line of sight (probed: 80 m ahead reached 14 m with them, 78 m
        // without). The flight's path keeps clear of obstacles, and the camera's room is kept.
        if flight {
            s.safety0 = w.safety.and_then(|at| {
                let mut b = [0u8; 1];
                s.game.read(at, &mut b).then_some((at, b[0]))
            });
        }
        Ok(s)
    }

    pub fn read3(&self, at: u64) -> Option<[f64; 3]> {
        let mut b = [0u8; 24];
        self.game
            .read(at, &mut b)
            .then(|| [0, 1, 2].map(|i| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap())))
    }

    pub fn write3(&self, at: u64, v: [f64; 3]) -> bool {
        let b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.game.write(at, &b)
    }

    /// The hero's root (cm), or `None` once it changed (a load, a cutscene).
    pub fn hero(&self) -> Option<[f32; 3]> {
        self.w.pose.read(&self.game).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32])
    }

    /// The way the hero's body faces (degrees).
    pub fn body_yaw(&self) -> Option<f32> {
        self.w.body.and_then(|at| self.read3(at)).map(|r| r[1] as f32)
    }

    /// One tick of time for what turns by itself (Circle).
    pub fn tick(&mut self, dt: f32) {
        self.orbit = wrap(self.orbit + ORBIT_DPS * dt);
    }

    /// The camera turned toward (pitch, yaw) by the gimbal, its roll kept.
    pub fn turn(&mut self, (pitch, yaw): (f32, f32), dt: f32) {
        let (p, y) = (self.pitch.toward(pitch, dt), self.yaw.toward(yaw, dt));
        let roll = self.read3(self.w.rotation).map_or(0.0, |r| r[2]);
        self.write3(self.w.rotation, [p.rem_euclid(360.0) as f64, y as f64, roll]);
    }

    /// Behind the view: as the gimbal holds it, or (the camera left alone) as the game has it.
    pub fn back(&self) -> [f32; 3] {
        if self.plan.lens == Lens::Free && self.plan.mode != Mode::Flight {
            return self.read3(self.w.rotation).map_or([0.0; 3], |r| back_of(wrap(r[0] as f32), r[1] as f32));
        }
        back_of(self.pitch.angle, self.yaw.angle)
    }

    /// The distance and field of view asked of the knobs from now (`None`: as they are asked).
    pub fn want(&mut self, distance: Option<f32>, fov: Option<f32>) {
        for k in self.knobs.iter_mut() {
            match (k.distance, k.fov, distance, fov) {
                (true, _, Some(d), _) => k.want = d,
                (_, true, _, Some(f)) => k.want = f,
                _ => {}
            }
        }
    }

    /// The camera's settings eased one tick, the distance kept within the room behind the camera
    /// from `pivots` (now and a moment ahead).
    pub fn ease(&mut self, pivots: [[f32; 3]; 2], dt: f32) {
        let back = self.back();
        ease_knobs(&mut self.knobs, &mut self.room, &self.blocking, pivots, back, dt, &self.game);
    }

    /// The pivot moved, for a flight: the safety checks held off while it is away.
    pub fn hold_checks_off(&self) {
        if let Some((at, b)) = self.safety0 {
            self.game.write(at, &[b & !0b11]);
        }
    }

    /// Everything the take changed put back: the stick at rest (when the mod drove the hero), the
    /// pivot, the safety checks, the camera's settings.
    pub fn restore(&self) {
        if self.plan.mode.drives() {
            self.write3(self.w.input, [0.0, 0.0, 0.0]);
        }
        if let (Some(pv), Some(p0)) = (self.w.pivot, self.pivot0) {
            self.write3(pv, p0);
        }
        if let Some((at, b)) = self.safety0 {
            self.game.write(at, &[b]);
        }
        for k in &self.knobs {
            self.game.write(k.at, &k.was.to_le_bytes());
        }
    }
}
