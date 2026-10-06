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

/// How long the hero is held where it stood when a flight ends, and Unreal's falling mode.
const PUT_BACK: std::time::Duration = std::time::Duration::from_millis(500);
const MOVE_FALLING: u8 = 3;
/// Unreal's walking mode: the hero has landed. How long to wait for it at most, and how long
/// after it before the take ends (the guard's cheats, no fall damage among them, go with it).
const MOVE_WALKING: u8 = 1;
const LANDING_WAIT: std::time::Duration = std::time::Duration::from_secs(3);
const LANDED_GRACE: std::time::Duration = std::time::Duration::from_millis(500);
/// The carried hero's mesh's scale: nothing to see, not quite nothing (a zero scale can upset the
/// engine's maths).
const HIDDEN_SCALE: f64 = 0.001;

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
    /// Where the take last put the camera (pitch, yaw), to tell the mouse's turn from its own.
    written: Option<(f32, f32)>,
    /// Where the hero stood when a flight began, to bring it back to (its root, cm).
    hero0: Option<[f64; 3]>,
    /// The hero's mesh's scale as it was, while it is shrunk out of sight.
    scale0: Option<[f64; 3]>,
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
            written: None,
            hero0: None,
            scale0: None,
        };
        let Some(rot0) = s.read3(w.rotation) else { return Err(State::Failed(tr!("ROTATION_UNREADABLE").into())) };
        (s.pitch, s.yaw) = (Axis::new(wrap(rot0[0] as f32)), Axis::new(wrap(rot0[1] as f32)));
        s.orbit = s.yaw.angle;
        let flight = plan.mode == Mode::Flight;
        // A flight keeps the camera close to its pivot unless asked otherwise; the distance is held
        // either way, kept within the room behind the camera (the game's own when none is asked);
        // the field of view too when the director moves it.
        let directed = plan.lens == Lens::Director;
        // a flight is a drone's: its lens turns about itself, whatever distance the card asks
        let distance = if flight { Some(FLIGHT_DISTANCE) } else { plan.distance };
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
        if flight && w.hero_root.is_some() {
            s.hero0 = w.pose.read(&s.game).map(|(p, _)| p);
            // only a scale that reads as one is shrunk and put back
            s.scale0 = w
                .hero_root
                .and_then(|h| h.scale)
                .and_then(|at| s.read3(at))
                .filter(|v| v.iter().all(|x| x.is_finite() && *x > 0.01 && *x < 100.0));
        }
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
        self.written = Some((p, y));
    }

    /// A cut: the camera put at once at (pitch, yaw), the distance and field of view asked made
    /// what they are (the distance within the room behind the camera), the game's own easing of the
    /// distance skipped.
    pub fn cut(&mut self, (pitch, yaw): (f32, f32), distance: f32, fov: f32) {
        self.pitch = Axis::new(wrap(pitch));
        self.yaw = Axis::new(wrap(yaw));
        let roll = self.read3(self.w.rotation).map_or(0.0, |r| r[2]);
        self.write3(self.w.rotation, [pitch.rem_euclid(360.0) as f64, yaw as f64, roll]);
        self.written = Some((wrap(pitch), wrap(yaw)));
        self.want(Some(distance), Some(fov));
        let room = if self.room == f32::MAX { distance } else { self.room };
        for k in self.knobs.iter_mut().filter(|k| k.distance || k.fov) {
            k.now = if k.distance { k.want.min(room) } else { k.want };
            self.game.write(k.at, &k.now.to_le_bytes());
        }
        if let Some(zoom) = self.w.zoom {
            let d = -(distance.min(room) as f64);
            for at in zoom {
                self.game.write(at, &d.to_le_bytes());
            }
        }
    }

    /// How far the camera is from where the take last put it (degrees): the mouse's turn since.
    pub fn drift(&self) -> f32 {
        let (Some((p, y)), Some(r)) = (self.written, self.read3(self.w.rotation)) else { return 0.0 };
        wrap(r[0] as f32 - p).abs() + wrap(r[1] as f32 - y).abs()
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

    /// The hero put at `p` (its root, cm), still: a flight carries it behind the camera.
    pub fn place_hero(&self, p: [f64; 3]) {
        let Some(h) = self.w.hero_root else { return };
        if let (Some(at), Some(_)) = (h.scale, self.scale0) {
            self.write3(at, [HIDDEN_SCALE; 3]);
        }
        self.put(p);
    }

    /// Waits until the hero has landed (its movement walking again), and a moment more: the fall
    /// is weighed as it lands, and the cheats guarding it must outlast that.
    fn landed(&self, mode: u64) {
        let until = std::time::Instant::now() + LANDING_WAIT;
        // the game takes up the falling mode on its next frame
        std::thread::sleep(std::time::Duration::from_millis(50));
        while std::time::Instant::now() < until {
            let mut b = [0u8; 1];
            if self.game.read(mode, &mut b) && b[0] == MOVE_WALKING {
                std::thread::sleep(LANDED_GRACE);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        crate::logfile::line("film: the hero put back did not land within 3 s");
    }

    /// The hero's place written, as a teleport does: its root, its world transform, still.
    fn put(&self, p: [f64; 3]) {
        let Some(h) = self.w.hero_root else { return };
        let bytes: Vec<u8> = p.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.game.write(h.location, &bytes);
        self.game.write(h.world, &bytes);
        if let Some(v) = h.velocity {
            self.game.write(v, &[0u8; 24]);
        }
    }

    /// The pivot moved, for a flight: the safety checks held off while it is away.
    pub fn hold_checks_off(&self) {
        if let Some((at, b)) = self.safety0 {
            self.game.write(at, &[b & !0b11]);
        }
    }

    /// Everything the take changed put back: the hero where it stood (a flight), the stick at rest
    /// (when the mod drove the hero), the pivot, the safety checks, the camera's settings.
    pub fn restore(&self) {
        // the hero back where it stood — unless it is gone (a load, a cutscene): held there a
        // moment, as one write may be moved over, then let fall into place, which has the game
        // move it there at once; its mesh its own size again first
        if let (Some(p), Some(_)) = (self.hero0, self.hero()) {
            if let (Some(at), Some(s0)) = (self.w.hero_root.and_then(|h| h.scale), self.scale0) {
                self.write3(at, s0);
            }
            let until = std::time::Instant::now() + PUT_BACK;
            while std::time::Instant::now() < until {
                self.put(p);
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            if let Some(m) = self.w.hero_root.and_then(|h| h.mode) {
                self.game.write(m, &[MOVE_FALLING]);
                self.landed(m);
            }
        }
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
