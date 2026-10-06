//! A live take: the player plays — walks, fights — and the take only moves the camera, keeping the
//! hero in view. Directed, it follows the guide's route as it changes (or the points): the shot for
//! where the player is along it, or behind them as they go when they leave it. The player's input
//! does not stop it; the key or the card does.

use super::cover::Cover;
use super::session::Session;
use super::{direct, State, Step};
use crate::film::avoid::ROOM_AHEAD_CM;
use crate::film::gimbal::EYE;
use crate::film::{aim, Director, Driver, Lens};
use std::time::{Duration, Instant};

/// How often the guide's route is looked at again (s), how far it must have moved to be taken up
/// (cm, its end), and how far from it the player is off it (cm).
const REREAD: Duration = Duration::from_secs(1);
const MOVED: f32 = 300.0;
const OFF_ROUTE: f32 = 1500.0;

pub(super) struct Live {
    route: Vec<[f32; 3]>,
    driver: Driver,
    director: Director,
    cover: Cover,
    read_at: Instant,
}

impl Live {
    pub fn new(s: &Session) -> Live {
        let route = s.plan.path.clone();
        Live {
            driver: Driver::new(&route),
            director: Director::new(&route, &s.blocking),
            route,
            cover: Cover::default(),
            read_at: Instant::now(),
        }
    }

    /// The guide's route again, when the take follows it and it has changed (a new goal, the route
    /// worked out again from where the player is).
    fn reread(&mut self, s: &Session) {
        if self.read_at.elapsed() < REREAD {
            return;
        }
        self.read_at = Instant::now();
        let Some(feed) = s.plan.feed.as_ref() else { return };
        let route = feed.lock().unwrap().0.clone();
        let end = |r: &[[f32; 3]]| r.last().copied();
        let moved = match (end(&route), end(&self.route)) {
            (Some(a), Some(b)) => (a[0] - b[0]).hypot(a[1] - b[1]) > MOVED,
            (a, b) => a.is_some() != b.is_some(),
        };
        if moved {
            self.driver = Driver::new(&route);
            self.director = Director::new(&route, &s.blocking);
            self.route = route;
        }
    }

    pub fn tick(&mut self, s: &mut Session, hero: [f32; 3], dt: f32) -> Step {
        self.reread(s);
        let body = s.body_yaw().unwrap_or(s.yaw.angle);
        let (along, ahead_yaw, share, off) = if self.route.len() >= 2 {
            let (step, off) = self.driver.track([hero[0], hero[1]]);
            (self.driver.along(), step.ahead_yaw, step.share, off)
        } else {
            (0.0, body, 0.0, f32::MAX)
        };
        let on_route = off < OFF_ROUTE;
        // the way the player goes: the route's, on it; their body's, off it
        let heading = if on_route { ahead_yaw } else { body };
        let state = if s.plan.lens == Lens::Director {
            let planned = on_route.then(|| {
                let (rig, shot) = self.director.rig(along);
                (rig, shot.label())
            });
            let cue = self.cover.cue(s, hero, heading, planned, true, dt);
            direct::apply(s, &cue, dt);
            State::Directing(share, cue.label)
        } else {
            let eye = [hero[0], hero[1], hero[2] + EYE];
            let end = self.route.last().copied().unwrap_or(eye);
            if let Some(t) = aim(s.plan.lens, eye, heading, end, s.orbit) {
                s.turn(t, dt);
            }
            State::Rolling(share)
        };
        let lift = s.pivot0.map_or(70.0, |p| p[2] as f32);
        let pivot = [hero[0], hero[1], hero[2] + lift];
        let a2 = if on_route {
            self.driver.ahead(ROOM_AHEAD_CM)
        } else {
            let h = heading.to_radians();
            [hero[0] + h.cos() * ROOM_AHEAD_CM, hero[1] + h.sin() * ROOM_AHEAD_CM]
        };
        s.ease([pivot, [a2[0], a2[1], pivot[2]]], dt);
        Step::Go(state)
    }
}
