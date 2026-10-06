//! Steering the hero along a route by the stick: pure pursuit a little ahead on it, eased in and
//! out, stuck when it gains nothing.

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
}
