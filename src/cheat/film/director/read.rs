//! Reading a route for the director: every `STEP` along it, whether the ground either side is open
//! (the obstacles within `SIDE`), how much it rises or falls ahead, how much it turns ahead, and
//! whether it stands over the land around it (a view).

use crate::obstacles::Blocking;

/// How often the route is read (cm), how far either side is looked at (cm), how far ahead the rise
/// and the turn are measured (cm), how far round a view is judged (cm), and how much higher than
/// the lowest of it a point must be to have one (cm).
pub const STEP: f32 = 200.0;
const SIDE: f32 = 600.0;
const RISE_AHEAD: f32 = 1000.0;
const TURN_AHEAD: f32 = 600.0;
const VIEW_ROUND: f32 = 5000.0;
const VIEW_ABOVE: f32 = 600.0;
/// The height the sides are looked at from, above the route (cm).
const CHEST: f32 = 120.0;

/// What the route is like at one point of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sense {
    /// How far along the route (cm).
    pub at: f32,
    pub open_left: bool,
    pub open_right: bool,
    /// The rise over the next `RISE_AHEAD` (cm; negative going down).
    pub rise: f32,
    /// The turn over the next `TURN_AHEAD` (degrees; positive to the left).
    pub turn: f32,
    /// Higher than the land round it, with nothing either side: a view.
    pub view: bool,
}

/// The route, every `STEP` (x, y, z, cm) with how far along each is.
pub fn sample(route: &[[f32; 3]]) -> Vec<(f32, [f32; 3])> {
    let mut out = Vec::new();
    let mut along = 0.0;
    let mut next = 0.0;
    for w in route.windows(2) {
        let d = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        while next <= along + d && d > 0.0 {
            let t = (next - along) / d;
            out.push((next, [0, 1, 2].map(|i| w[0][i] + (w[1][i] - w[0][i]) * t)));
            next += STEP;
        }
        along += d;
    }
    if let Some(&last) = route.last() {
        if out.last().is_none_or(|(a, _)| along - a > 1.0) {
            out.push((along, last));
        }
    }
    out
}

/// The route read every `STEP`.
pub fn read(route: &[[f32; 3]], b: &Blocking) -> Vec<Sense> {
    let pts = sample(route);
    let n = pts.len();
    let k = |cm: f32| (cm / STEP).round().max(1.0) as usize;
    let heading = |i: usize| {
        let (a, c) = (pts[i.min(n - 1)].1, pts[(i + 1).min(n - 1)].1);
        if (c[0] - a[0]).hypot(c[1] - a[1]) < 1.0 && i > 0 {
            let p = pts[i - 1].1;
            (a[1] - p[1]).atan2(a[0] - p[0]).to_degrees()
        } else {
            (c[1] - a[1]).atan2(c[0] - a[0]).to_degrees()
        }
    };
    (0..n)
        .map(|i| {
            let (at, p) = pts[i];
            let h = heading(i).to_radians();
            let left = [-h.sin(), h.cos()];
            let from = [p[0], p[1], p[2] + CHEST];
            let side = |s: f32| [p[0] + left[0] * SIDE * s, p[1] + left[1] * SIDE * s, p[2] + CHEST];
            let open_left = !b.blocks(from, side(1.0));
            let open_right = !b.blocks(from, side(-1.0));
            let rise = pts[(i + k(RISE_AHEAD)).min(n - 1)].1[2] - p[2];
            let turn = crate::film::wrap(heading((i + k(TURN_AHEAD)).min(n - 1)) - heading(i));
            let r = k(VIEW_ROUND);
            let lowest = pts[i.saturating_sub(r)..(i + r + 1).min(n)].iter().map(|q| q.1[2]).fold(f32::MAX, f32::min);
            let view = open_left && open_right && p[2] - lowest > VIEW_ABOVE;
            Sense { at, open_left, open_right, rise, turn, view }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_is_read_every_step_with_its_rise_and_turn() {
        let scene = crate::obstacles::Scene::default();
        let route = [[0.0, 0.0, 0.0], [2000.0, 0.0, 500.0], [2000.0, 2000.0, 500.0]];
        let s = read(&route, &scene.blocking());
        assert!((s[1].at - STEP).abs() < 1e-3);
        assert!(s[0].rise > 200.0, "climbing at the start: {}", s[0].rise);
        let corner = s.iter().find(|x| (x.at - 1800.0).abs() < 1.0).unwrap();
        assert!(corner.turn > 60.0, "turning left before the corner: {}", corner.turn);
        assert!(s.iter().all(|x| x.open_left && x.open_right));
    }
}
