//! Filming mode (.spec/ROADMAP.md §4.1): the hero walked along a route by the stick's input,
//! the camera turned the way a drone's gimbal would, for recording.
//!
//! Probed in play: `Pawn.ControlInputVector` written at 250 Hz has the game walk the hero
//! itself — its collisions, stairs, slopes, animation, and the body turned to the way — where
//! written positions slide it; `ControlRotation` turns the camera with the rig's own smoothing.
//! The game consumes the input every frame, so when the writes stop the hero stops.
//!
//! The camera follows drone practice (DJI's): every move eased in and out (Cine), the gimbal a
//! critically damped spring under a turn-rate cap, and four modes — left alone, ActiveTrack
//! (looking the way ahead), Spotlight (on the destination while walking) and Circle (turning
//! about the hero at a steady rate). The camera's distance and field of view are eased to the
//! ones asked for and put back after, through the exploration camera's own settings, which the
//! game reads every frame and interpolates toward itself (probed with the game in focus:
//! 484 → 832 cm without a tremor, 70° → 55°). Holding the camera mode's interpolator instead
//! fought the game's own target each frame and shook.

use crate::mem::Memory;
use crate::player::PoseSource;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How the camera moves while filming.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lens {
    /// The player's camera, untouched.
    Free,
    /// ActiveTrack: the way ahead, a little down.
    #[default]
    Follow,
    /// Spotlight: on the route's end the whole way.
    Spotlight,
    /// Circle: about the hero, `ORBIT_DPS` a second.
    Orbit,
}

impl Lens {
    pub const ALL: [Lens; 4] = [Lens::Free, Lens::Follow, Lens::Spotlight, Lens::Orbit];

    pub fn label(self) -> &'static str {
        match self {
            Lens::Free => tr!("FILM_LENS_FREE"),
            Lens::Follow => tr!("FILM_LENS_FOLLOW"),
            Lens::Spotlight => tr!("FILM_LENS_SPOTLIGHT"),
            Lens::Orbit => tr!("FILM_LENS_ORBIT"),
        }
    }
}

/// A take: the route (cm), the pace (the stick's length, 0–1), the camera, its distance (cm)
/// and field of view (degrees) (`None`, as they are), whether to walk back and forth until
/// stopped, and the countdown (s).
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub path: Vec<[f32; 3]>,
    pub pace: f32,
    pub lens: Lens,
    pub distance: Option<f32>,
    pub fov: Option<f32>,
    pub repeat: bool,
    pub countdown: u32,
}

/// Where a take writes and reads, found by the worker: the hero's `ControlInputVector`, the
/// controller's `ControlRotation`, the pose, and the exploration camera's
/// `DefaultDistanceFromPlayer` and `FieldOfView` (floats), when found.
pub struct Wiring {
    pub input: u64,
    pub rotation: u64,
    pub pose: PoseSource,
    pub distance: Option<u64>,
    pub fov: Option<u64>,
}

/// The filming card's settings, kept in `Mods\film.txt`: the points from the 3D map, whether
/// to walk them (else the guide's route), the pace, the camera, its distance, back and forth,
/// the key that starts and stops a take (F1–F12, 0 none), and the routes kept by name.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub points: Vec<[f32; 3]>,
    pub use_points: bool,
    pub pace: f32,
    pub lens: Lens,
    pub distance: Option<f32>,
    pub fov: Option<f32>,
    pub repeat: bool,
    pub key: u8,
    pub routes: Vec<(String, Vec<[f32; 3]>)>,
}

impl Default for Setup {
    fn default() -> Setup {
        Setup {
            points: Vec::new(),
            use_points: false,
            pace: 0.6,
            lens: Lens::Follow,
            distance: None,
            fov: None,
            repeat: false,
            key: 6,
            routes: Vec::new(),
        }
    }
}

/// The distance (cm) and field of view (degrees) the card offers.
pub const DISTANCE: std::ops::RangeInclusive<f32> = 250.0..=1500.0;
pub const FOV: std::ops::RangeInclusive<f32> = 30.0..=100.0;

impl Lens {
    fn word(self) -> &'static str {
        match self {
            Lens::Free => "free",
            Lens::Follow => "follow",
            Lens::Spotlight => "spotlight",
            Lens::Orbit => "orbit",
        }
    }
}

impl Setup {
    pub fn path() -> std::path::PathBuf {
        crate::paths::data_dir().join("film.txt")
    }

    pub fn load() -> Setup {
        std::fs::read_to_string(Setup::path()).map(|t| Setup::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::write(Setup::path(), self.render());
    }

    pub fn render(&self) -> String {
        let pt = |p: &[f32; 3]| format!("{} {} {}", p[0], p[1], p[2]);
        let mut out = format!(
            "use_points {}\npace {}\nlens {}\ndistance {}\nfov {}\nrepeat {}\nkey {}\n",
            self.use_points,
            self.pace,
            self.lens.word(),
            self.distance.map_or("none".to_string(), |d| d.to_string()),
            self.fov.map_or("none".to_string(), |d| d.to_string()),
            self.repeat,
            self.key
        );
        for p in &self.points {
            out += &format!("point {}\n", pt(p));
        }
        for (name, pts) in &self.routes {
            // the name last, as it may hold spaces
            out += &format!("route {}\n", name.replace('\n', " "));
            for p in pts {
                out += &format!("route_point {}\n", pt(p));
            }
        }
        out
    }

    pub fn parse(text: &str) -> Setup {
        let mut s = Setup::default();
        let point = |f: &[&str]| -> Option<[f32; 3]> {
            let v: Vec<f32> = f.iter().filter_map(|x| x.parse().ok()).filter(|v: &f32| v.is_finite()).collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[..] {
                ["use_points", v] => s.use_points = v == "true",
                ["pace", v] => s.pace = v.parse().unwrap_or(s.pace).clamp(0.2, 1.0),
                ["lens", v] => s.lens = Lens::ALL.into_iter().find(|l| l.word() == v).unwrap_or_default(),
                ["distance", v] => {
                    s.distance = v.parse().ok().map(|d: f32| d.clamp(*DISTANCE.start(), *DISTANCE.end()))
                }
                ["fov", v] => s.fov = v.parse().ok().map(|d: f32| d.clamp(*FOV.start(), *FOV.end())),
                ["repeat", v] => s.repeat = v == "true",
                ["key", v] => s.key = v.parse().unwrap_or(s.key).min(12),
                ["point", ..] => s.points.extend(point(&f[1..])),
                ["route", ..] => s.routes.push((line["route ".len()..].trim().to_string(), Vec::new())),
                ["route_point", ..] => {
                    if let (Some(r), Some(p)) = (s.routes.last_mut(), point(&f[1..])) {
                        r.1.push(p);
                    }
                }
                _ => {}
            }
        }
        s
    }
}

/// A take's route from the hero: the guide's route as drawn (`route`), or through the setup's
/// points, each leg walked on the navmesh (straight where it has none).
pub fn path(setup: &Setup, hero: [f32; 3], route: &[[f32; 3]], nav: &crate::navmesh::NavMesh) -> Option<Vec<[f32; 3]>> {
    if !setup.use_points {
        return (route.len() >= 2).then(|| route.to_vec());
    }
    if setup.points.is_empty() {
        return None;
    }
    let mut path = vec![hero];
    let mut from = hero;
    for &to in &setup.points {
        match nav.route(from, to) {
            Some((leg, _)) => path.extend(leg.points.iter().skip(1).map(|q| [q[0], q[1], to[2]])),
            None => path.push(to),
        }
        from = to;
    }
    Some(path)
}

/// Where a take is.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum State {
    #[default]
    Idle,
    /// Seconds to the start.
    Countdown(u32),
    /// Rolling: how much of the route is walked (0–1).
    Rolling(f32),
    Finished,
    Stuck,
    /// Stopped by the player's keyboard or mouse.
    Interrupted,
    Stopped,
    Failed(String),
}

impl State {
    pub fn text(&self) -> String {
        match self {
            State::Idle => tr!("FILM_STATE_IDLE").into(),
            State::Countdown(n) => trf!("FILM_STATE_COUNTDOWN", n = n),
            State::Rolling(k) => trf!("FILM_STATE_ROLLING", pct = (k * 100.0).round() as u32),
            State::Finished => tr!("FILM_STATE_FINISHED").into(),
            State::Stuck => tr!("FILM_STATE_STUCK").into(),
            State::Interrupted => tr!("FILM_STATE_INTERRUPTED").into(),
            State::Stopped => tr!("FILM_STATE_STOPPED").into(),
            State::Failed(e) => trf!("FILM_STATE_FAILED", e = e),
        }
    }

    pub fn rolling(&self) -> bool {
        matches!(self, State::Countdown(_) | State::Rolling(_))
    }
}

/// How far ahead on the route the hero steers for, and the camera looks (cm).
const STEER_AHEAD: f32 = 150.0;
const LOOK_AHEAD: f32 = 450.0;
/// Close enough to the end (cm).
const ARRIVED: f32 = 60.0;
/// Eased in over this long (s); eased out over the last this far (cm), never under `CREEP`.
const EASE_IN: f32 = 1.5;
const EASE_OUT: f32 = 350.0;
const CREEP: f32 = 0.3;
/// Stuck: less than `STUCK_CM` gained along the route in `STUCK_S`.
const STUCK_CM: f32 = 30.0;
const STUCK_S: f32 = 2.5;
/// The gimbal: its spring (rad/s), its turn-rate cap (°/s), the camera's pitch on the way.
const GIMBAL_OMEGA: f32 = 2.2;
const GIMBAL_MAX_DPS: f32 = 70.0;
const FOLLOW_PITCH: f32 = -8.0;
pub const ORBIT_DPS: f32 = 18.0;
/// Where the camera is taken to look from for Spotlight: the hero's head (cm above the feet).
const EYE: f32 = 160.0;
/// Writes a second.
const RATE_HZ: f64 = 250.0;
/// The countdown from the card, which leaves time to switch to the game; from the key, a
/// moment for the key to be let go of before input is watched.
pub const COUNTDOWN_S: u32 = 3;
pub const KEY_GRACE: Duration = Duration::from_millis(600);
/// How fast the camera's distance and field of view ease to the ones asked for (share a second).
const ZOOM_EASE: f32 = 1.2;

/// A camera setting a take eases to the value asked for, and puts back after.
struct Knob {
    at: u64,
    was: f32,
    now: f32,
    want: f32,
}

/// The walk along a route: the stick's input each moment from where the hero is.
pub struct Driver {
    path: Vec<[f32; 2]>,
    /// Distance along the route at each point (cm).
    at_len: Vec<f32>,
    /// How far along the hero is (cm), never going back.
    along: f32,
    t: f32,
    /// The best `along` and when it was reached, for being stuck.
    best: (f32, f32),
}

/// One moment of the walk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    /// The stick (x, y), its length the pace eased.
    pub input: [f32; 2],
    /// The way ahead, for the camera (degrees, Unreal yaw).
    pub ahead_yaw: f32,
    /// Walked share of the route (0–1).
    pub share: f32,
    pub done: bool,
    pub stuck: bool,
}

impl Driver {
    pub fn new(path: &[[f32; 3]]) -> Driver {
        let mut pts: Vec<[f32; 2]> = Vec::new();
        for p in path {
            if pts.last().is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 1.0) {
                pts.push([p[0], p[1]]);
            }
        }
        let mut at_len = vec![0.0];
        for w in pts.windows(2) {
            let d = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            at_len.push(at_len.last().unwrap() + d);
        }
        Driver { path: pts, at_len, along: 0.0, t: 0.0, best: (0.0, 0.0) }
    }

    pub fn length(&self) -> f32 {
        *self.at_len.last().unwrap_or(&0.0)
    }

    /// The point `s` cm along the route.
    fn point(&self, s: f32) -> [f32; 2] {
        let s = s.clamp(0.0, self.length());
        let i = self.at_len.partition_point(|&l| l <= s).clamp(1, self.path.len().max(2) - 1);
        if self.path.len() < 2 {
            return self.path.first().copied().unwrap_or([0.0, 0.0]);
        }
        let (a, b) = (self.path[i - 1], self.path[i]);
        let seg = self.at_len[i] - self.at_len[i - 1];
        let k = if seg > 0.0 { (s - self.at_len[i - 1]) / seg } else { 0.0 };
        [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
    }

    /// Where along the route `p` is nearest, looking from the last place on (never back,
    /// and not past the next few metres, so a route doubling back is not skipped).
    fn locate(&self, p: [f32; 2]) -> f32 {
        let mut best = (f32::MAX, self.along);
        for i in 1..self.path.len() {
            if self.at_len[i] < self.along - 1.0 || self.at_len[i - 1] > self.along + 600.0 {
                continue;
            }
            let (a, b) = (self.path[i - 1], self.path[i]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dy * dy;
            let k = if len2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let q = [a[0] + dx * k, a[1] + dy * k];
            let d = (p[0] - q[0]).hypot(p[1] - q[1]);
            let s = self.at_len[i - 1] + k * len2.sqrt();
            if d < best.0 && s >= self.along {
                best = (d, s);
            }
        }
        best.1
    }

    pub fn step(&mut self, p: [f32; 2], pace: f32, dt: f32) -> Steer {
        self.t += dt;
        self.along = self.locate(p);
        let left = self.length() - self.along;
        let end = self.point(self.length());
        let to_end = (end[0] - p[0]).hypot(end[1] - p[1]);
        let share = if self.length() > 0.0 { self.along / self.length() } else { 1.0 };
        let yaw_to = |q: [f32; 2]| (q[1] - p[1]).atan2(q[0] - p[0]).to_degrees();
        let ahead_yaw = yaw_to(self.point(self.along + LOOK_AHEAD));
        if left < ARRIVED && to_end < ARRIVED * 2.0 {
            return Steer { input: [0.0, 0.0], ahead_yaw, share: 1.0, done: true, stuck: false };
        }
        if self.along > self.best.0 + STUCK_CM {
            self.best = (self.along, self.t);
        }
        let stuck = self.t > EASE_IN && self.t - self.best.1 > STUCK_S;
        let target = self.point(self.along + STEER_AHEAD);
        let (dx, dy) = (target[0] - p[0], target[1] - p[1]);
        let d = dx.hypot(dy).max(1e-3);
        let ease_in = {
            let k = (self.t / EASE_IN).clamp(0.0, 1.0);
            k * k * (3.0 - 2.0 * k)
        };
        let ease_out = (left / EASE_OUT).clamp(CREEP, 1.0);
        let len = pace.clamp(0.05, 1.0) * ease_in.max(0.15) * ease_out;
        Steer { input: [dx / d * len, dy / d * len], ahead_yaw, share, done: false, stuck }
    }
}

/// Degrees into (−180, 180].
pub fn wrap(d: f32) -> f32 {
    let r = (d + 180.0).rem_euclid(360.0) - 180.0;
    if r == -180.0 {
        180.0
    } else {
        r
    }
}

/// A gimbal axis: a critically damped spring toward its target, its speed capped (Cine).
#[derive(Clone, Copy, Debug, Default)]
pub struct Axis {
    pub angle: f32,
    speed: f32,
}

impl Axis {
    pub fn new(angle: f32) -> Axis {
        Axis { angle, speed: 0.0 }
    }

    pub fn toward(&mut self, target: f32, dt: f32) -> f32 {
        let x = wrap(target - self.angle);
        let w = GIMBAL_OMEGA;
        self.speed += (w * w * x - 2.0 * w * self.speed) * dt;
        self.speed = self.speed.clamp(-GIMBAL_MAX_DPS, GIMBAL_MAX_DPS);
        self.angle = wrap(self.angle + self.speed * dt);
        self.angle
    }
}

/// Where the gimbal aims for `lens`: (pitch, yaw), or `None` to leave the camera be.
pub fn aim(lens: Lens, hero: [f32; 3], ahead_yaw: f32, end: [f32; 3], orbit: f32) -> Option<(f32, f32)> {
    match lens {
        Lens::Free => None,
        Lens::Follow => Some((FOLLOW_PITCH, ahead_yaw)),
        Lens::Spotlight => {
            let (dx, dy, dz) = (end[0] - hero[0], end[1] - hero[1], end[2] - (hero[2] + EYE));
            let flat = dx.hypot(dy);
            if flat < 100.0 {
                return Some((FOLLOW_PITCH, ahead_yaw));
            }
            Some((dz.atan2(flat).to_degrees().clamp(-45.0, 30.0), dy.atan2(dx).to_degrees()))
        }
        Lens::Orbit => Some((FOLLOW_PITCH, orbit)),
    }
}

/// A take running: stopping it ends the thread, which leaves the stick at rest.
pub struct Take {
    stop: Arc<AtomicBool>,
}

impl Take {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for Take {
    fn drop(&mut self) {
        self.stop();
    }
}

/// When the player last touched the keyboard or mouse (the system's tick count).
fn last_input() -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: `info` is a valid LASTINPUTINFO with its size set.
    unsafe { GetLastInputInfo(&mut info) };
    info.dwTime
}

/// Roll a take on a thread of its own, the game opened for writing there: a countdown, then
/// the walk at `RATE_HZ` until the end, a stop, the player's input, or the hero stuck.
pub fn roll(plan: Plan, wiring: Wiring, state: Arc<Mutex<State>>, ended: Arc<Mutex<Option<Instant>>>) -> Take {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    *state.lock().unwrap() = State::Countdown(plan.countdown.max(1));
    std::thread::spawn(move || {
        let end = run(&plan, &wiring, &flag, &state);
        *state.lock().unwrap() = end;
        *ended.lock().unwrap() = Some(Instant::now());
    });
    Take { stop }
}

fn run(plan: &Plan, w: &Wiring, stop: &AtomicBool, state: &Mutex<State>) -> State {
    let game = match crate::game::process::Game::find() {
        Ok(Some(g)) => g,
        Ok(None) => return State::Failed(tr!("GAME_NOT_RUNNING").into()),
        Err(e) => return State::Failed(e),
    };
    let start = Instant::now();
    let wait = if plan.countdown == 0 { KEY_GRACE } else { Duration::from_secs(plan.countdown as u64) };
    while start.elapsed() < wait {
        if stop.load(Ordering::SeqCst) {
            return State::Stopped;
        }
        let left = plan.countdown.saturating_sub(start.elapsed().as_secs() as u32);
        *state.lock().unwrap() = State::Countdown(left.max(1));
        std::thread::sleep(Duration::from_millis(50));
    }
    let write3 = |at: u64, v: [f64; 3]| {
        let b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        game.write(at, &b)
    };
    let read3 = |at: u64| -> Option<[f64; 3]> {
        let mut b = [0u8; 24];
        game.read(at, &mut b).then(|| [0, 1, 2].map(|i| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap())))
    };
    let Some(rot0) = read3(w.rotation) else { return State::Failed(tr!("ROTATION_UNREADABLE").into()) };
    let (mut pitch, mut yaw) = (Axis::new(wrap(rot0[0] as f32)), Axis::new(wrap(rot0[1] as f32)));
    let mut orbit = yaw.angle;
    let mut route = plan.path.clone();
    let mut driver = Driver::new(&route);
    let mut end3 = *route.last().unwrap_or(&[0.0; 3]);
    // The camera's distance and field of view as they were, put back after; eased from there.
    let read_f = |at: u64| -> Option<f32> {
        let mut b = [0u8; 4];
        game.read(at, &mut b).then(|| f32::from_le_bytes(b)).filter(|v| v.is_finite())
    };
    let mut knobs: Vec<Knob> = [(w.distance, plan.distance), (w.fov, plan.fov)]
        .into_iter()
        .filter_map(|(at, want)| {
            let (at, want) = (at?, want?);
            let was = read_f(at)?;
            Some(Knob { at, was, now: was, want })
        })
        .collect();
    let touched = last_input();
    let tick = Duration::from_secs_f64(1.0 / RATE_HZ);
    let mut next = Instant::now();
    let mut last = Instant::now();
    let outcome = loop {
        if stop.load(Ordering::SeqCst) {
            break State::Stopped;
        }
        if last_input() != touched {
            break State::Interrupted;
        }
        let Some((p, _)) = w.pose.read(&game) else { break State::Failed(tr!("THE_HERO_CHANGED").into()) };
        let dt = last.elapsed().as_secs_f32().min(0.05);
        last = Instant::now();
        let hero = [p[0] as f32, p[1] as f32, p[2] as f32];
        let s = driver.step([hero[0], hero[1]], plan.pace, dt);
        if s.done && plan.repeat {
            // back the way it came, the camera and the clock carried on
            route.reverse();
            driver = Driver::new(&route);
            end3 = *route.last().unwrap_or(&[0.0; 3]);
            continue;
        }
        if s.done {
            break State::Finished;
        }
        if s.stuck {
            break State::Stuck;
        }
        if !write3(w.input, [s.input[0] as f64, s.input[1] as f64, 0.0]) {
            break State::Failed(tr!("COULD_NOT_WRITE_THE_HEROS_POSITION").into());
        }
        orbit = wrap(orbit + ORBIT_DPS * dt);
        if let Some((tp, ty)) = aim(plan.lens, hero, s.ahead_yaw, end3, orbit) {
            let (np, ny) = (pitch.toward(tp, dt), yaw.toward(ty, dt));
            let roll = read3(w.rotation).map_or(0.0, |r| r[2]);
            write3(w.rotation, [np.rem_euclid(360.0) as f64, ny as f64, roll]);
        }
        for k in &mut knobs {
            k.now += (k.want - k.now) * (ZOOM_EASE * dt).min(1.0);
            game.write(k.at, &k.now.to_le_bytes());
        }
        *state.lock().unwrap() = State::Rolling(s.share);
        next += tick;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    };
    write3(w.input, [0.0, 0.0, 0.0]);
    for k in &knobs {
        game.write(k.at, &k.was.to_le_bytes());
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_steers_along_the_route_and_arrives() {
        let path = [[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], [1000.0, 1000.0, 0.0]];
        let mut d = Driver::new(&path);
        assert_eq!(d.length(), 2000.0);
        let mut p = [0.0f32, 0.0];
        let mut steps = 0;
        // walk it: the hero moves as the stick says, 300 cm/s at full length
        loop {
            let s = d.step(p, 1.0, 0.02);
            assert!(!s.stuck, "stuck at {p:?}");
            if s.done {
                break;
            }
            p[0] += s.input[0] * 300.0 * 0.02;
            p[1] += s.input[1] * 300.0 * 0.02;
            steps += 1;
            assert!(steps < 5000, "never arrived: {p:?}");
            // never far off the route
            let off = if p[1] < 0.0 || p[0] < 1000.0 - 160.0 { p[1].abs() } else { (p[0] - 1000.0).abs() };
            assert!(off < 160.0, "off the route at {p:?}");
        }
        assert!((p[0] - 1000.0).abs() < 130.0 && (p[1] - 1000.0).abs() < 130.0, "ended at {p:?}");
    }

    #[test]
    fn a_hero_held_still_is_stuck() {
        let mut d = Driver::new(&[[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0]]);
        let stuck = (0..300).map(|_| d.step([0.0, 0.0], 1.0, 0.02)).any(|s| s.stuck);
        assert!(stuck);
    }

    #[test]
    fn the_walk_eases_in_and_out() {
        let mut d = Driver::new(&[[0.0, 0.0, 0.0], [2000.0, 0.0, 0.0]]);
        let first = d.step([0.0, 0.0], 1.0, 0.02);
        let len = |s: Steer| s.input[0].hypot(s.input[1]);
        assert!(len(first) < 0.3, "starts gently: {}", len(first));
        let mut d = Driver::new(&[[0.0, 0.0, 0.0], [2000.0, 0.0, 0.0]]);
        for _ in 0..100 {
            d.step([0.0, 0.0], 1.0, 0.02);
        }
        assert!(len(d.step([1000.0, 0.0], 1.0, 0.02)) > 0.95, "full pace in the middle");
        assert!(len(d.step([1850.0, 0.0], 1.0, 0.02)) < 0.5, "slows near the end");
    }

    #[test]
    fn the_gimbal_turns_smoothly_the_short_way() {
        let mut a = Axis::new(170.0);
        let mut last = a.angle;
        for _ in 0..500 {
            let now = a.toward(-170.0, 0.01);
            // across ±180, not the long way round, and never faster than the cap
            assert!(wrap(now - last).abs() <= GIMBAL_MAX_DPS * 0.01 + 1e-3);
            last = now;
        }
        assert!(wrap(a.angle + 170.0).abs() < 1.0, "settled at {}", a.angle);
    }

    #[test]
    fn the_setup_is_kept() {
        let s = Setup {
            points: vec![[1.0, 2.0, 3.0]],
            use_points: true,
            pace: 0.8,
            lens: Lens::Orbit,
            distance: Some(900.0),
            fov: Some(55.0),
            repeat: true,
            key: 7,
            routes: vec![("bridge at dusk".into(), vec![[4.0, 5.0, 6.0], [7.0, 8.0, 9.0]])],
        };
        assert_eq!(Setup::parse(&s.render()), s);
        assert_eq!(Setup::parse(""), Setup::default());
    }

    #[test]
    fn spotlight_looks_at_the_end() {
        let (p, y) = aim(Lens::Spotlight, [0.0, 0.0, 0.0], 0.0, [0.0, 1000.0, 160.0], 0.0).unwrap();
        assert!((y - 90.0).abs() < 0.01 && p.abs() < 0.01);
        assert_eq!(aim(Lens::Free, [0.0; 3], 0.0, [0.0; 3], 0.0), None);
    }
}
