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
//! fought the game's own target each frame and shook. The exploration, combat and APC cameras
//! are all set, so a fight on the way keeps the shot.
//!
//! A flight moves the camera alone, the hero left where it stands: the camera mode's
//! `PivotToViewTarget` (an FTransform, its translation 70 cm up at rest) is where the camera
//! turns about, in the hero's frame (its body's yaw), and the game follows it smoothly
//! (probed: 500 cm ahead moved the camera 480 cm the way the hero faced). The path is flown
//! straight between the points, its corners rounded, lifted above the floor.
//!
//! Obstacles, after DJI's APAS: a drone set to film bypasses rather than brakes, its avoidance
//! planned ahead so the path stays smooth. Here: a flight's path is checked before it starts —
//! over low obstacles, round tall ones — and its heights and turns spread out so each begins
//! well before the obstacle. The camera's room behind it is checked every tick, now and a moment
//! ahead, and its distance eased in before the game's own pull-in (a snap in 0.15 s, out in
//! 0.25 s) would have to act, and back out slowly after; that pull-in, if it still acts, is
//! slowed for the take.

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
    /// Fly the camera alone along the path; the hero stays.
    pub flight: bool,
}

/// Where a take writes and reads, found by the worker: the hero's `ControlInputVector`, the
/// controller's `ControlRotation`, the pose, the cameras' (exploration, combat, APC)
/// `DefaultDistanceFromPlayer` and `FieldOfView` (floats), the camera mode's pivot translation
/// (`PivotToViewTarget` + 0x20, three doubles) and the hero root's `RelativeRotation`.
pub struct Wiring {
    pub input: u64,
    pub rotation: u64,
    pub pose: PoseSource,
    pub distance: Vec<u64>,
    pub fov: Vec<u64>,
    pub pivot: Option<u64>,
    pub body: Option<u64>,
    /// The camera mode's `PenetrationBlendInTime` and `PenetrationBlendOutTime` (floats).
    pub blend: Option<(u64, u64)>,
    /// The camera mode's byte of `bValidateSafeLoc` (bit 0) and `bPreventCameraPenetration`
    /// (bit 1): cleared on a flight, which plans its own way round obstacles.
    pub safety: Option<u64>,
    /// What stands in the way (obstacles.rs), for the flight's path and the camera's room.
    pub scene: std::sync::Arc<crate::obstacles::Scene>,
}

/// Clearing obstacles: the room kept from them (cm), how high an obstacle may stand above the
/// path to be flown over rather than round (cm), the step the path is checked at (cm), and how
/// far either side an avoidance is spread (cm).
const CLEAR: f32 = 120.0;
const CLIMB_MAX: f32 = 450.0;
const CHECK_STEP: f32 = 50.0;
const SPREAD: f32 = 400.0;
/// The camera's room: kept from what is behind it (cm), never closer than (cm), how far ahead
/// it looks (s of the flight, cm of the walk), and how fast it closes in and backs out (share a
/// second).
const ROOM_MARGIN: f32 = 60.0;
const ROOM_MIN: f32 = 80.0;
const ROOM_AHEAD_S: f32 = 0.8;
const ROOM_AHEAD_CM: f32 = 250.0;
const ROOM_IN: f32 = 2.5;
const ROOM_OUT: f32 = 0.7;
/// The game's own pull-in, slowed for a take (s): in, out.
const BLEND: (f32, f32) = (0.6, 1.2);

/// A flight's path kept clear of obstacles: each point inside one lifted over it (when it stands
/// at most `CLIMB_MAX` above) or moved out past its side, then the heights spread so a climb
/// starts `SPREAD` before (the most within reach, then the mean) and the moves smoothed, twice.
pub fn clear_flight(path: &[[f32; 3]], b: &crate::obstacles::Blocking) -> Vec<[f32; 3]> {
    // every CHECK_STEP
    let mut pts: Vec<[f32; 3]> = Vec::new();
    for w in path.windows(2) {
        let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt();
        let n = (d / CHECK_STEP).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = k as f32 / n as f32;
            pts.push([0, 1, 2].map(|i| w[0][i] + (w[1][i] - w[0][i]) * t));
        }
    }
    pts.extend(path.last());
    if pts.len() < 3 {
        return pts;
    }
    let reach = (SPREAD / CHECK_STEP).round() as usize;
    let n = pts.len();
    // a window as wide either side (narrower near the ends, which stay where they are)
    let window = |i: usize| {
        let r = reach.min(i).min(n - 1 - i);
        i - r..i + r + 1
    };
    for _ in 0..3 {
        // what each point needs: a height, a move aside
        let mut need = vec![f32::MIN; n];
        let mut aside = vec![[0.0f32; 2]; n];
        for i in 0..n {
            let p = &pts[i];
            for o in b.under(p[0], p[1]) {
                if p[2] < o.zmin - CLEAR || p[2] > o.zmax + CLEAR {
                    continue;
                }
                if o.zmax - p[2] <= CLIMB_MAX {
                    need[i] = need[i].max(o.zmax + CLEAR);
                } else {
                    // round it, square to the way: on the side away from its middle, as far as
                    // takes the point out of it and CLEAR beyond
                    let (prev, next) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
                    let (wx, wy) = (next[0] - prev[0], next[1] - prev[1]);
                    let wl = wx.hypot(wy).max(1e-3);
                    let mut side = [-wy / wl, wx / wl];
                    let mid = o.hull.iter().fold([0.0f32, 0.0], |m, q| [m[0] + q[0], m[1] + q[1]]);
                    let mid = [mid[0] / o.hull.len() as f32, mid[1] / o.hull.len() as f32];
                    if (mid[0] - p[0]) * side[0] + (mid[1] - p[1]) * side[1] > 0.0 {
                        side = [-side[0], -side[1]];
                    }
                    let out = crate::obstacles::clip(&o.hull, *p, [side[0], side[1], 0.0], 0.0, 100_000.0)
                        .map_or(0.0, |(_, t1)| t1);
                    let m = [side[0] * (out + CLEAR), side[1] * (out + CLEAR)];
                    if m[0].hypot(m[1]) > aside[i][0].hypot(aside[i][1]) {
                        aside[i] = m;
                    }
                }
            }
        }
        if need.iter().all(|&z| z == f32::MIN) && aside.iter().all(|a| a[0] == 0.0 && a[1] == 0.0) {
            break;
        }
        // spread: the most any point within reach needs, then the mean of that — so where an
        // obstacle is, all of it is kept, and it eases in and out either side
        let most_z: Vec<f32> = (0..n).map(|i| window(i).map(|j| need[j]).fold(pts[i][2], f32::max)).collect();
        let most_aside: Vec<[f32; 2]> = (0..n)
            .map(|i| {
                window(i)
                    .map(|j| aside[j])
                    .fold([0.0f32, 0.0], |a, m| if m[0].hypot(m[1]) > a[0].hypot(a[1]) { m } else { a })
            })
            .collect();
        let mean = |i: usize, v: &dyn Fn(usize) -> f32| {
            let r = window(i);
            let len = r.len() as f32;
            r.map(v).sum::<f32>() / len
        };
        let next: Vec<[f32; 3]> = (0..n)
            .map(|i| {
                [
                    pts[i][0] + mean(i, &|j| most_aside[j][0]),
                    pts[i][1] + mean(i, &|j| most_aside[j][1]),
                    mean(i, &|j| most_z[j]).max(pts[i][2]),
                ]
            })
            .collect();
        pts = next;
    }
    pts
}

/// How far behind `pivot` (along `back`) the camera can be, up to `want`: the obstacles' room
/// less `ROOM_MARGIN`, never under `ROOM_MIN`.
pub fn room_behind(b: &crate::obstacles::Blocking, pivot: [f32; 3], back: [f32; 3], want: f32) -> f32 {
    let at = |d: f32| [0, 1, 2].map(|i| pivot[i] + back[i] * d);
    if !b.blocks(pivot, at(want + ROOM_MARGIN)) {
        return want;
    }
    let (mut lo, mut hi) = (0.0, want + ROOM_MARGIN);
    for _ in 0..6 {
        let mid = (lo + hi) / 2.0;
        if b.blocks(pivot, at(mid)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    (lo - ROOM_MARGIN).clamp(ROOM_MIN, want)
}

/// Behind the camera's view, as (pitch, yaw) in degrees.
fn back_of(pitch: f32, yaw: f32) -> [f32; 3] {
    let (p, y) = (pitch.to_radians(), yaw.to_radians());
    [-p.cos() * y.cos(), -p.cos() * y.sin(), -p.sin()]
}

/// A flight: how high above the floor points it goes (cm), how fast at full pace (cm/s), how
/// long it eases in (s) and over how far it eases out (cm), and the camera's distance behind
/// the pivot when none is asked for (cm), so the camera is on the path.
const FLIGHT_LIFT: f32 = 180.0;
const FLIGHT_SPEED: f32 = 450.0;
const FLIGHT_EASE_IN: f32 = 2.0;
const FLIGHT_EASE_OUT: f32 = 600.0;
pub const FLIGHT_DISTANCE: f32 = 150.0;

/// Corners rounded: Chaikin's corner cutting, `rounds` times, the ends kept.
pub fn rounded(path: &[[f32; 3]], rounds: usize) -> Vec<[f32; 3]> {
    let mut p = path.to_vec();
    for _ in 0..rounds {
        if p.len() < 3 {
            break;
        }
        let mut q = vec![p[0]];
        for w in p.windows(2) {
            let lerp = |k: f32| [0, 1, 2].map(|i| w[0][i] + (w[1][i] - w[0][i]) * k);
            q.push(lerp(0.25));
            q.push(lerp(0.75));
        }
        q.push(*p.last().unwrap());
        p = q;
    }
    p
}

/// The camera along a flight: where it is each moment, eased in and out.
pub struct Flight {
    path: Vec<[f32; 3]>,
    at_len: Vec<f32>,
    s: f32,
    t: f32,
}

/// One moment of a flight: where the camera turns about, the way ahead (yaw, degrees), the
/// share flown, whether it is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fly {
    pub at: [f32; 3],
    pub ahead_yaw: f32,
    pub share: f32,
    pub done: bool,
}

impl Flight {
    pub fn new(path: &[[f32; 3]]) -> Flight {
        let path = rounded(path, 3);
        let mut at_len = vec![0.0];
        for w in path.windows(2) {
            let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt();
            at_len.push(at_len.last().unwrap() + d);
        }
        Flight { path, at_len, s: 0.0, t: 0.0 }
    }

    pub fn length(&self) -> f32 {
        *self.at_len.last().unwrap_or(&0.0)
    }

    fn point(&self, s: f32) -> [f32; 3] {
        if self.path.len() < 2 {
            return self.path.first().copied().unwrap_or([0.0; 3]);
        }
        let s = s.clamp(0.0, self.length());
        let i = self.at_len.partition_point(|&l| l <= s).clamp(1, self.path.len() - 1);
        let seg = self.at_len[i] - self.at_len[i - 1];
        let k = if seg > 0.0 { (s - self.at_len[i - 1]) / seg } else { 0.0 };
        [0, 1, 2].map(|j| self.path[i - 1][j] + (self.path[i][j] - self.path[i - 1][j]) * k)
    }

    /// Where the flight will be in `secs` at full pace.
    pub fn ahead(&self, pace: f32, secs: f32) -> [f32; 3] {
        self.point(self.s + pace.clamp(0.05, 1.0) * FLIGHT_SPEED * secs)
    }

    pub fn step(&mut self, pace: f32, dt: f32) -> Fly {
        self.t += dt;
        let left = self.length() - self.s;
        let ease_in = {
            let k = (self.t / FLIGHT_EASE_IN).clamp(0.0, 1.0);
            k * k * (3.0 - 2.0 * k)
        };
        let ease_out = (left / FLIGHT_EASE_OUT).clamp(0.0, 1.0).sqrt().max(0.04);
        self.s = (self.s + pace.clamp(0.05, 1.0) * FLIGHT_SPEED * ease_in.max(0.05) * ease_out * dt).min(self.length());
        let at = self.point(self.s);
        let ahead = self.point(self.s + 300.0);
        let ahead_yaw = if (ahead[0] - at[0]).hypot(ahead[1] - at[1]) > 1.0 {
            (ahead[1] - at[1]).atan2(ahead[0] - at[0]).to_degrees()
        } else {
            let back = self.point(self.s - 300.0);
            (at[1] - back[1]).atan2(at[0] - back[0]).to_degrees()
        };
        let share = if self.length() > 0.0 { self.s / self.length() } else { 1.0 };
        Fly { at, ahead_yaw, share, done: self.length() - self.s < 2.0 }
    }
}

/// A flight's points from the setup: the 3D map's, or the guide's route, lifted above the floor
/// (the camera's own start is put in front when it rolls).
pub fn flight_path(setup: &Setup, route: &[[f32; 3]]) -> Option<Vec<[f32; 3]>> {
    let lift = |p: &[f32; 3]| [p[0], p[1], p[2] + FLIGHT_LIFT];
    if setup.use_points {
        return (!setup.points.is_empty()).then(|| setup.points.iter().map(lift).collect());
    }
    (route.len() >= 2).then(|| route.iter().map(lift).collect())
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
    /// The key (F8): alone, it starts and stops a take; with Ctrl, it adds where the hero stands
    /// to the points, or takes away the one it stands by — as the map's marker key does.
    pub key: u8,
    /// Fly the camera alone; the hero stays.
    pub flight: bool,
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
            key: KEY,
            flight: false,
            routes: Vec::new(),
        }
    }
}

/// The key by default (F7 is the game's photo mode).
pub const KEY: u8 = 8;
/// A point taken within this of another takes that one away instead (cm), as the map's marker.
pub const NEAR: f32 = 500.0;

/// From the hero's root to its feet (cm): a point taken where it stands is on the floor, as the
/// 3D map's are.
pub const FEET: f32 = 90.0;

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
            "use_points {}\npace {}\nlens {}\ndistance {}\nfov {}\nrepeat {}\nkey {}\nkeys_version 2\nflight {}\n",
            self.use_points,
            self.pace,
            self.lens.word(),
            self.distance.map_or("none".to_string(), |d| d.to_string()),
            self.fov.map_or("none".to_string(), |d| d.to_string()),
            self.repeat,
            self.key,
            self.flight
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
        let mut keys_version = 1;
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[..] {
                ["keys_version", v] => keys_version = v.parse().unwrap_or(1),
                ["use_points", v] => s.use_points = v == "true",
                ["pace", v] => s.pace = v.parse().unwrap_or(s.pace).clamp(0.2, 1.0),
                ["lens", v] => s.lens = Lens::ALL.into_iter().find(|l| l.word() == v).unwrap_or_default(),
                ["distance", v] => {
                    s.distance = v.parse().ok().map(|d: f32| d.clamp(*DISTANCE.start(), *DISTANCE.end()))
                }
                ["fov", v] => s.fov = v.parse().ok().map(|d: f32| d.clamp(*FOV.start(), *FOV.end())),
                ["repeat", v] => s.repeat = v == "true",
                ["flight", v] => s.flight = v == "true",
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
        // the first keys (F6 to start, F8 for points) gave way to F8, with Ctrl for points
        if keys_version < 2 && !text.is_empty() {
            s.key = KEY;
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

/// A camera setting a take eases to the value asked for, and puts back after. A distance's
/// value is also kept within the room behind the camera.
struct Knob {
    at: u64,
    was: f32,
    now: f32,
    want: f32,
    distance: bool,
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

    /// The route `cm` on from where the hero is along it.
    pub fn ahead(&self, cm: f32) -> [f32; 2] {
        self.point(self.along + cm)
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

/// Where the gimbal aims for `lens` from `eye`: (pitch, yaw), or `None` to leave the camera be.
/// Spotlight looks at `look_at`: the route's end on a walk, the hero on a flight.
pub fn aim(lens: Lens, eye: [f32; 3], ahead_yaw: f32, look_at: [f32; 3], orbit: f32) -> Option<(f32, f32)> {
    match lens {
        Lens::Free => None,
        Lens::Follow => Some((FOLLOW_PITCH, ahead_yaw)),
        Lens::Spotlight => {
            let (dx, dy, dz) = (look_at[0] - eye[0], look_at[1] - eye[1], look_at[2] - eye[2]);
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

/// The camera's settings eased one tick toward what is asked: a distance no further than the
/// room behind the camera from the pivots given (now and a moment ahead) — closing in faster
/// than backing out, as the room is eased itself — the rest straight to their values.
#[allow(clippy::too_many_arguments)]
fn ease_knobs(
    knobs: &mut [Knob],
    room: &mut f32,
    b: &crate::obstacles::Blocking,
    pivots: [[f32; 3]; 2],
    back: [f32; 3],
    dt: f32,
    game: &crate::game::process::Game,
) {
    let want = knobs.iter().filter(|k| k.distance).map(|k| k.want).fold(0.0, f32::max);
    if want > 0.0 {
        let free = pivots.iter().map(|&p| room_behind(b, p, back, want)).fold(want, f32::min);
        if *room == f32::MAX {
            *room = free;
        }
        let rate = if free < *room { ROOM_IN } else { ROOM_OUT };
        *room += (free - *room) * (rate * dt).min(1.0);
    }
    for k in knobs.iter_mut() {
        let target = if k.distance { k.want.min(*room) } else { k.want };
        k.now += (target - k.now) * (ZOOM_EASE * dt).min(1.0);
        game.write(k.at, &k.now.to_le_bytes());
    }
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
    // a flight keeps the camera close to its pivot unless asked otherwise; the distance is held
    // either way, kept within the room behind the camera (the game's own when none is asked)
    let distance = plan.distance.or(plan.flight.then_some(FLIGHT_DISTANCE));
    let mut knobs: Vec<Knob> = w
        .distance
        .iter()
        .map(|&at| (at, distance, true))
        .chain(w.fov.iter().map(|&at| (at, plan.fov, false)))
        .chain(w.blend.iter().flat_map(|&(i, o)| [(i, Some(BLEND.0), false), (o, Some(BLEND.1), false)]))
        .filter_map(|(at, want, is_distance)| {
            let was = read_f(at)?;
            let want = want.or(is_distance.then_some(was))?;
            Some(Knob { at, was, now: was, want, distance: is_distance })
        })
        .collect();
    let blocking = w.scene.blocking();
    // the camera's room, kept between ticks: eased toward what is free
    let mut room = f32::MAX;
    // A flight: from where the camera turns about now (the pivot, in the hero's frame).
    let pivot0 = w.pivot.and_then(read3);
    // On a flight the game's own checks from the hero to the camera's pivot are off: they held
    // the camera on the hero's line of sight (probed: 80 m ahead reached 14 m with them, 78 m
    // without). The flight's path keeps clear of obstacles, and the camera's room is kept.
    let read_b = |at: u64| -> Option<u8> {
        let mut b = [0u8; 1];
        game.read(at, &mut b).then_some(b[0])
    };
    let safety0 = w.safety.filter(|_| plan.flight).and_then(|at| Some((at, read_b(at)?)));
    let mut flight = None;
    if plan.flight {
        let (Some(p0), Some(body), Some((hp, _))) = (pivot0, w.body.and_then(read3), w.pose.read(&game)) else {
            return State::Failed(trf!("NO_PROPERTY", name = "PivotToViewTarget"));
        };
        let (s, c) = (body[1] as f32).to_radians().sin_cos();
        let (lx, ly) = (p0[0] as f32, p0[1] as f32);
        let start = [hp[0] as f32 + lx * c - ly * s, hp[1] as f32 + lx * s + ly * c, hp[2] as f32 + p0[2] as f32];
        let mut path = vec![start];
        path.extend(plan.path.iter().copied());
        flight = Some(Flight::new(&clear_flight(&path, &blocking)));
    }
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
        orbit = wrap(orbit + ORBIT_DPS * dt);
        if let (Some(f), Some(pv), Some(body)) = (flight.as_mut(), w.pivot, w.body) {
            let fly = f.step(plan.pace, dt);
            if fly.done && plan.repeat {
                let mut back = f.path.clone();
                back.reverse();
                *f = Flight::new(&back);
                continue;
            }
            let ahead = f.ahead(plan.pace, ROOM_AHEAD_S);
            if fly.done {
                break State::Finished;
            }
            // into the hero's frame: the body's yaw undone
            let yaw_b = read3(body).map_or(0.0, |r| r[1] as f32).to_radians();
            let (s, c) = yaw_b.sin_cos();
            let (dx, dy) = (fly.at[0] - hero[0], fly.at[1] - hero[1]);
            let local = [(dx * c + dy * s) as f64, (-dx * s + dy * c) as f64, (fly.at[2] - hero[2]) as f64];
            write3(pv, local);
            if let Some((at, b)) = safety0 {
                game.write(at, &[b & !0b11]);
            }
            let head = [hero[0], hero[1], hero[2] + EYE - 90.0];
            // Circling turns the camera about its pivot, which on a flight is on the path, not
            // the hero: there it looks at the hero instead.
            let lens = if plan.lens == Lens::Orbit { Lens::Spotlight } else { plan.lens };
            if let Some((tp, ty)) = aim(lens, fly.at, fly.ahead_yaw, head, orbit) {
                let (np, ny) = (pitch.toward(tp, dt), yaw.toward(ty, dt));
                let roll = read3(w.rotation).map_or(0.0, |r| r[2]);
                write3(w.rotation, [np.rem_euclid(360.0) as f64, ny as f64, roll]);
            }
            let back = back_of(pitch.angle, yaw.angle);
            ease_knobs(&mut knobs, &mut room, &blocking, [fly.at, ahead], back, dt, &game);
            *state.lock().unwrap() = State::Rolling(fly.share);
            next += tick;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now;
            }
            continue;
        }
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
        let eye = [hero[0], hero[1], hero[2] + EYE];
        if let Some((tp, ty)) = aim(plan.lens, eye, s.ahead_yaw, end3, orbit) {
            let (np, ny) = (pitch.toward(tp, dt), yaw.toward(ty, dt));
            let roll = read3(w.rotation).map_or(0.0, |r| r[2]);
            write3(w.rotation, [np.rem_euclid(360.0) as f64, ny as f64, roll]);
        }
        // the camera's room from the pivot (the hero, raised as the mode has it), now and ahead
        let lift = pivot0.map_or(70.0, |p| p[2] as f32);
        let pivot = [hero[0], hero[1], hero[2] + lift];
        let a2 = driver.ahead(ROOM_AHEAD_CM);
        let ahead = [a2[0], a2[1], pivot[2]];
        let back = if plan.lens == Lens::Free {
            read3(w.rotation).map_or([0.0; 3], |r| back_of(wrap(r[0] as f32), r[1] as f32))
        } else {
            back_of(pitch.angle, yaw.angle)
        };
        ease_knobs(&mut knobs, &mut room, &blocking, [pivot, ahead], back, dt, &game);
        *state.lock().unwrap() = State::Rolling(s.share);
        next += tick;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    };
    if !plan.flight {
        write3(w.input, [0.0, 0.0, 0.0]);
    }
    if let (Some(pv), Some(p0)) = (w.pivot, pivot0) {
        write3(pv, p0);
    }
    if let Some((at, b)) = safety0 {
        game.write(at, &[b]);
    }
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
            key: 9,
            flight: true,
            routes: vec![("bridge at dusk".into(), vec![[4.0, 5.0, 6.0], [7.0, 8.0, 9.0]])],
        };
        assert_eq!(Setup::parse(&s.render()), s);
        assert_eq!(Setup::parse(""), Setup::default());
        // the first keys move to the new ones
        let old = Setup::parse("key 6\npoint_key 8\npace 0.5\n");
        assert_eq!((old.key, old.pace), (KEY, 0.5));
    }

    #[test]
    fn a_flight_rounds_its_corners_and_lands_at_the_end() {
        let path = [[0.0, 0.0, 200.0], [1000.0, 0.0, 200.0], [1000.0, 1000.0, 400.0]];
        let r = rounded(&path, 3);
        assert_eq!(r[0], path[0]);
        assert_eq!(*r.last().unwrap(), path[2]);
        // the corner is cut: no point of the rounded path is at it
        assert!(r.iter().all(|p| (p[0] - 1000.0).hypot(p[1]) > 50.0));
        let mut f = Flight::new(&path);
        let (mut last, mut n) = (f.step(1.0, 0.02), 0);
        let mut fastest: f32 = 0.0;
        while !last.done {
            let next = f.step(1.0, 0.02);
            let d = ((next.at[0] - last.at[0]).powi(2) + (next.at[1] - last.at[1]).powi(2)).sqrt();
            fastest = fastest.max(d / 0.02);
            last = next;
            n += 1;
            assert!(n < 10_000);
        }
        assert!(
            (last.at[0] - 1000.0).abs() < 3.0 && (last.at[1] - 1000.0).abs() < 3.0 && (last.at[2] - 400.0).abs() < 3.0
        );
        assert!(fastest <= FLIGHT_SPEED + 1.0, "{fastest}");
    }

    fn wall(x0: f32, x1: f32, y0: f32, y1: f32, zmax: f32) -> crate::obstacles::Obstacle {
        crate::obstacles::Obstacle { hull: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], zmin: 0.0, zmax, water: false }
    }

    #[test]
    fn a_flight_climbs_over_a_low_obstacle_early_and_smoothly() {
        let scene = crate::obstacles::Scene {
            obstacles: vec![wall(900.0, 1100.0, -500.0, 500.0, 300.0)],
            ..Default::default()
        };
        let b = scene.blocking();
        let path = clear_flight(&[[0.0, 0.0, 200.0], [2000.0, 0.0, 200.0]], &b);
        for w in path.windows(2) {
            assert!(!b.blocks(w[0], w[1]), "through the obstacle at {:?}", w[0]);
            // never steeper than 1 in 2
            let run = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            assert!((w[1][2] - w[0][2]).abs() <= run * 0.5 + 1.0, "too steep at {:?}", w[0]);
        }
        // the climb starts well before the obstacle
        let at = |x: f32| path.iter().min_by(|a, b| (a[0] - x).abs().total_cmp(&(b[0] - x).abs())).unwrap()[2];
        assert!(at(700.0) > 250.0, "late climb: {}", at(700.0));
        assert_eq!(path.first().unwrap()[2], 200.0);
        assert_eq!(path.last().unwrap()[2], 200.0);
    }

    #[test]
    fn a_flight_goes_round_a_tall_wall() {
        let scene = crate::obstacles::Scene {
            obstacles: vec![wall(900.0, 1100.0, -300.0, 100.0, 3000.0)],
            ..Default::default()
        };
        let b = scene.blocking();
        let path = clear_flight(&[[0.0, 0.0, 200.0], [2000.0, 0.0, 200.0]], &b);
        assert!(path.iter().all(|p| p[2] < 400.0), "not over a tall wall");
        assert!(path.windows(2).all(|w| !b.blocks(w[0], w[1])), "through the wall");
    }

    #[test]
    fn the_camera_keeps_its_room() {
        let scene = crate::obstacles::Scene {
            obstacles: vec![wall(-400.0, -350.0, -500.0, 500.0, 1000.0)],
            ..Default::default()
        };
        let b = scene.blocking();
        let back = back_of(0.0, 0.0);
        let r = room_behind(&b, [0.0, 0.0, 100.0], back, 800.0);
        assert!((r - (350.0 - ROOM_MARGIN)).abs() < 15.0, "{r}");
        assert_eq!(room_behind(&b, [0.0, 0.0, 100.0], back_of(0.0, 180.0), 800.0), 800.0);
    }

    #[test]
    fn spotlight_looks_at_the_end() {
        let (p, y) = aim(Lens::Spotlight, [0.0, 0.0, 160.0], 0.0, [0.0, 1000.0, 160.0], 0.0).unwrap();
        assert!((y - 90.0).abs() < 0.01 && p.abs() < 0.01);
        assert_eq!(aim(Lens::Free, [0.0; 3], 0.0, [0.0; 3], 0.0), None);
    }
}
