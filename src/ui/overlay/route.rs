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

/// Work the route out again after this long, or when the hero has moved this far (cm).
const ROUTE_EVERY: Duration = Duration::from_secs(1);
const ROUTE_MOVED: f32 = 500.0;
/// The compass points at the route this far ahead (cm), not at the goal itself.
pub const ROUTE_AHEAD: f32 = 800.0;
/// A goal's actor stands about this far above the floor it is on (cm).
const GOAL_FEET: f32 = 50.0;

#[derive(Default)]
pub struct Route {
    path: Path,
    goal: Option<u64>,
    from: [f32; 2],
    at: Option<Instant>,
    /// A route being worked out off this thread (it can take a few hundred ms), and
    /// the goal it is for.
    pending: Option<std::thread::JoinHandle<(Path, u64)>>,
    /// Goals whose last route had to go through something.
    pub blocked: HashSet<u64>,
}

impl Route {
    /// Keep the route to `goal` current from the hero at `p`; `None` drops it (but
    /// not what is known to be blocked). `trail` is where the hero has walked — cheap
    /// ground for the grid.
    pub fn follow(&mut self, goal: Option<&Goal>, p: [f32; 3], trail: impl FnOnce() -> Vec<[f32; 2]>, scene: &Arc<Scene>, nav: &Arc<crate::navmesh::NavMesh>) {
        let Some(g) = goal else {
            let blocked = std::mem::take(&mut self.blocked);
            *self = Route { blocked, ..Route::default() };
            return;
        };
        let moved = ((p[0] - self.from[0]).powi(2) + (p[1] - self.from[1]).powi(2)).sqrt();
        let due = self.goal != Some(g.id) || moved > ROUTE_MOVED || self.at.is_none_or(|t| t.elapsed() >= ROUTE_EVERY);
        if self.pending.as_ref().is_some_and(|h| h.is_finished()) {
            if let Ok((path, id)) = self.pending.take().unwrap().join() {
                // Whether it can be walked to at all: a route that has to go through
                // (a locked door, a puzzle) marks its goal blocked.
                if path.uncertain() {
                    self.blocked.insert(id);
                } else {
                    self.blocked.remove(&id);
                }
                // Same goal: keep the way being followed unless this one is clearly
                // better, so the compass does not swing.
                if id == g.id
                    && (self.path.points.len() < 2 || crate::pathfind::better(&self.path, &path, [p[0], p[1]], ROUTE_AHEAD))
                {
                    self.path = path;
                }
            }
        }
        if due && self.pending.is_none() {
            let trail = trail();
            let (from, to, feet, id, scene, nav) = ([p[0], p[1]], [g.at[0], g.at[1]], p[2] - 90.0, g.id, scene.clone(), nav.clone());
            let goal_feet = g.at[2] - GOAL_FEET;
            self.pending = Some(std::thread::spawn(move || {
                let path = nav
                    .route([from[0], from[1], feet], [to[0], to[1], goal_feet])
                    .map(|(path, _)| path)
                    .unwrap_or_else(|| crate::pathfind::route(from, to, feet, &scene.obstacles, &scene.terrain, &trail));
                (path, id)
            }));
            if self.goal != Some(g.id) {
                self.path = Default::default();
            }
            self.goal = Some(g.id);
            self.from = [p[0], p[1]];
            self.at = Some(Instant::now());
        }
    }

    /// The route as drawn: starting at the hero.
    pub fn drawn(&self, p: [f32; 3]) -> Path {
        let mut path = self.path.clone();
        if let Some(first) = path.points.first_mut() {
            *first = [p[0], p[1]];
        }
        path
    }
}
