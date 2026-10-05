//! The walking route to the guide's goal, worked out off the overlay thread at most
//! once a second, and whenever the goal changes or the hero has moved on: the game's
//! navmesh first (it knows stairs, cellars and closed doors), the obstacle grid
//! (pathfind.rs) where it has nothing.

use crate::goals::Goal;
use crate::obstacles::Scene;
use crate::pathfind::Path;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Work the route out again after this long, or when the hero has moved this far (cm);
/// a route not in focus (guide/track.rs) the slower pace: only one is looked at closely.
const ROUTE_EVERY: Duration = Duration::from_secs(1);
const ROUTE_MOVED: f32 = 500.0;
const SLOW_EVERY: Duration = Duration::from_secs(3);
const SLOW_MOVED: f32 = 1500.0;
/// The compass points at the route this far ahead (cm), not at the goal itself.
pub const ROUTE_AHEAD: f32 = 800.0;
/// A goal's actor stands about this far above the floor it is on (cm).
const GOAL_FEET: f32 = 50.0;

#[derive(Default)]
pub struct Route {
    path: Path,
    /// The height of each of `path`'s points (cm): the navmesh's floor, or the ground.
    heights: Vec<f32>,
    goal: Option<u64>,
    from: [f32; 2],
    at: Option<Instant>,
    /// A route being worked out off this thread (it can take a few hundred ms), and
    /// the goal it is for.
    pending: Option<std::thread::JoinHandle<(Path, Vec<f32>, u64)>>,
    /// Goals whose last route had to go through something.
    pub blocked: HashSet<u64>,
    /// What each route worked out since last asked said of its goal: blocked or not
    /// (`verdicts`), for the overlay to remember past this route.
    seen: Vec<(u64, bool)>,
}

impl Route {
    /// What the routes worked out since last asked: each goal, and whether it was blocked.
    pub fn verdicts(&mut self) -> Vec<(u64, bool)> {
        std::mem::take(&mut self.seen)
    }

    /// Keep the route to `goal` current from the hero at `p`; `None` drops it (but
    /// not what is known to be blocked). `trail` is where the hero has walked — cheap
    /// ground for the grid.
    pub fn follow(
        &mut self,
        goal: Option<&Goal>,
        p: [f32; 3],
        trail: impl FnOnce() -> Vec<[f32; 2]>,
        scene: &Arc<Scene>,
        nav: &Arc<crate::navmesh::NavMesh>,
        slow: bool,
    ) {
        let Some(g) = goal else {
            let blocked = std::mem::take(&mut self.blocked);
            *self = Route { blocked, ..Route::default() };
            return;
        };
        let moved = ((p[0] - self.from[0]).powi(2) + (p[1] - self.from[1]).powi(2)).sqrt();
        let (every, far) = if slow { (SLOW_EVERY, SLOW_MOVED) } else { (ROUTE_EVERY, ROUTE_MOVED) };
        let due = self.goal != Some(g.id) || moved > far || self.at.is_none_or(|t| t.elapsed() >= every);
        if self.pending.as_ref().is_some_and(|h| h.is_finished()) {
            if let Ok((path, heights, id)) = self.pending.take().unwrap().join() {
                // Whether it can be walked to at all: a route that has to go through
                // (a locked door, a puzzle) marks its goal blocked.
                if path.uncertain() {
                    self.blocked.insert(id);
                } else {
                    self.blocked.remove(&id);
                }
                self.seen.push((id, path.uncertain()));
                // Same goal: keep the way being followed unless this one is clearly
                // better, so the compass does not swing.
                if id == g.id
                    && (self.path.points.len() < 2
                        || crate::pathfind::better(&self.path, &path, [p[0], p[1]], ROUTE_AHEAD))
                {
                    self.path = path;
                    self.heights = heights;
                }
            }
        }
        if due && self.pending.is_none() {
            let trail = trail();
            let (from, to, feet, id, scene, nav) =
                ([p[0], p[1]], [g.at[0], g.at[1]], p[2] - 90.0, g.id, scene.clone(), nav.clone());
            let goal_feet = g.at[2] - GOAL_FEET;
            self.pending = Some(std::thread::spawn(move || {
                let (path, heights) =
                    nav.route([from[0], from[1], feet], [to[0], to[1], goal_feet]).unwrap_or_else(|| {
                        let path = crate::pathfind::route(from, to, feet, &scene.obstacles, &scene.terrain, &trail);
                        // The grid's way knows no floors: the ground under it, or the hero's feet.
                        let heights =
                            path.points.iter().map(|q| scene.terrain.height(q[0], q[1]).unwrap_or(feet)).collect();
                        (path, heights)
                    });
                (path, heights, id)
            }));
            if self.goal != Some(g.id) {
                self.path = Default::default();
                self.heights.clear();
            }
            self.goal = Some(g.id);
            self.from = [p[0], p[1]];
            self.at = Some(Instant::now());
        }
    }

    /// The route in 3D (cm), from the hero's feet: for the 3D map and the route drawn into the
    /// game's view. Points without a height (a path kept from before heights) take the hero's.
    pub fn drawn3d(&self, p: [f32; 3]) -> Vec<[f32; 3]> {
        let feet = p[2] - 90.0;
        let pts: Vec<[f32; 3]> =
            self.path
                .points
                .iter()
                .enumerate()
                .map(|(k, q)| {
                    if k == 0 {
                        [p[0], p[1], feet]
                    } else {
                        [q[0], q[1], self.heights.get(k).copied().unwrap_or(feet)]
                    }
                })
                .collect();
        crate::pathfind::smooth3(&pts)
    }

    /// The route as drawn: starting at the hero.
    pub fn drawn(&self, p: [f32; 3]) -> Path {
        let mut path = self.path.clone();
        if let Some(first) = path.points.first_mut() {
            *first = [p[0], p[1]];
        }
        // drawn with round bends, not a polyline's corners
        path.smooth()
    }
}

/// How far apart the points of a draped route are (cm).
const DRAPE_STEP: f32 = 50.0;
/// The ground above the navmesh by less than this (cm) is the floor (`drape`).
const DRAPE_GROUND: f32 = 100.0;

/// `pts` laid over the floor: a point every `DRAPE_STEP`, each at the height of the floor under
/// it — the navmesh's poly there, or the ground's where the ground stands a little above it (the
/// navmesh's polys are coarse, the landscape's bumps are not). Between the route's corners the
/// height was a straight line, and the band drawn on it sank into stairs and slopes (seen in
/// play). Off the navmesh and the ground, the straight line stays.
pub fn drape(pts: &[[f32; 3]], nav: &crate::navmesh::NavMesh, scene: &Scene) -> Vec<[f32; 3]> {
    let floor = |q: [f32; 3]| {
        let mut z = nav.floor_at(q).unwrap_or(q[2]);
        if let Some(g) = scene.terrain.height(q[0], q[1]) {
            if g > z && g - z < DRAPE_GROUND {
                z = g;
            }
        }
        [q[0], q[1], z]
    };
    let mut out = Vec::with_capacity(pts.len() * 4);
    for (k, w) in pts.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let n = (len / DRAPE_STEP).ceil().max(1.0) as usize;
        // the hero's own point stays where the hero stands
        let from = if k == 0 { 1 } else { 0 };
        if k == 0 {
            out.push(a);
        }
        for i in from..n {
            let t = i as f32 / n as f32;
            out.push(floor([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]));
        }
    }
    if let Some(&last) = pts.last() {
        out.push(last);
    }
    if pts.len() == 1 {
        out.truncate(1);
    }
    out
}
