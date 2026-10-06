//! Obstacles, after DJI's APAS: a flight's path cleared before it starts (over low obstacles,
//! round tall ones, each avoidance spread so it eases in and out), and the room behind the camera
//! kept every tick.

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
pub(super) const ROOM_AHEAD_S: f32 = 0.8;
pub(super) const ROOM_AHEAD_CM: f32 = 250.0;
pub(super) const ROOM_IN: f32 = 2.5;
pub(super) const ROOM_OUT: f32 = 0.7;

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

/// How far up from `at` the camera may go, up to `want` (cm): short of the first thing above (a
/// ceiling, an overhang), `CLEAR` under it.
pub fn headroom(b: &crate::obstacles::Blocking, at: [f32; 3], want: f32) -> f32 {
    if want <= 0.0 {
        return 0.0;
    }
    let top = [at[0], at[1], at[2] + want + CLEAR];
    if !b.blocks(at, top) {
        return want;
    }
    // the free height found by halving
    let (mut lo, mut hi) = (0.0f32, want + CLEAR);
    for _ in 0..8 {
        let mid = (lo + hi) / 2.0;
        if b.blocks(at, [at[0], at[1], at[2] + mid]) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    (lo - CLEAR).max(0.0)
}

/// How far above the floor (the landscape or water over it) a flight's camera stays, and a carried
/// hero's feet (cm); and how far the floor may be above before it is taken for the landscape over
/// a cave or a hall the flight is in, and left alone (cm).
pub const CAMERA_OVER_FLOOR: f32 = 150.0;
pub const FEET_OVER_FLOOR: f32 = 50.0;
const FLOOR_ABOVE_INDOORS: f32 = 800.0;

/// The lowest `p` may be at its (x, y) to be `over` the floor: the landscape or water there — not
/// sunk into the ground, not drowned. `None` where there is no floor, or where it lies far above
/// (a flight under the landscape, indoors).
pub fn above_floor(b: &crate::obstacles::Blocking, p: [f32; 3], over: f32) -> Option<f32> {
    let f = b.floor(p[0], p[1])?;
    (f - p[2] < FLOOR_ABOVE_INDOORS).then_some(f + over)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::film::gimbal::back_of;

    #[test]
    fn a_flight_stays_above_the_water_but_not_indoors_below_it() {
        let scene = crate::obstacles::Scene {
            pools: vec![crate::obstacles::Pool {
                hull: vec![[-1000.0, -1000.0], [1000.0, -1000.0], [1000.0, 1000.0], [-1000.0, 1000.0]],
                bottom: -500.0,
                top: 0.0,
            }],
            ..Default::default()
        };
        let b = scene.blocking();
        // over the water: kept above its surface
        assert_eq!(above_floor(&b, [0.0, 0.0, -100.0], CAMERA_OVER_FLOOR), Some(CAMERA_OVER_FLOOR));
        // far below it: a hall under it, left alone
        assert_eq!(above_floor(&b, [0.0, 0.0, -2000.0], CAMERA_OVER_FLOOR), None);
        // beside it: no floor known
        assert_eq!(above_floor(&b, [5000.0, 0.0, 0.0], CAMERA_OVER_FLOOR), None);
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
}
