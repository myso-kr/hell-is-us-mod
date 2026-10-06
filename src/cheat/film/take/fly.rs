//! A flight: the camera's pivot along the cleared path in the hero's frame, the camera aimed by the
//! lens or, directed, looked as an aerial film would (director/aerial.rs).

use super::session::Session;
use super::{State, Step};
use crate::film::avoid::ROOM_AHEAD_S;
use crate::film::gimbal::EYE;
use crate::film::FLIGHT_DISTANCE;
use crate::film::{aim, clear_flight, Aerial, AerialPlan, Flight, Glance, Lens};

pub(super) struct Fly {
    flight: Flight,
    aerial: Option<AerialPlan>,
    glance: Glance,
    /// The camera's height over the path now (cm), eased toward the look's, and the most there is
    /// room for above, looked at now and then.
    lift: f32,
    room: (f32, f32),
}

/// How fast the height changes at most (cm/s), how often the room above is looked at (s).
const LIFT_SPEED: f32 = 350.0;
const ROOM_EVERY: f32 = 0.1;

impl Fly {
    /// From where the camera turns about now (the pivot, in the hero's frame) along the plan's path.
    pub fn new(s: &Session) -> Result<Fly, State> {
        let (Some(p0), Some(body), Some(hp)) = (s.pivot0, s.body_yaw(), s.hero()) else {
            return Err(State::Failed(trf!("NO_PROPERTY", name = "PivotToViewTarget")));
        };
        let (sn, c) = body.to_radians().sin_cos();
        let (lx, ly) = (p0[0] as f32, p0[1] as f32);
        let start = [hp[0] + lx * c - ly * sn, hp[1] + lx * sn + ly * c, hp[2] + p0[2] as f32];
        let mut path = vec![start];
        path.extend(s.plan.path.iter().copied());
        let path = clear_flight(&path, &s.blocking);
        let aerial = (s.plan.lens == Lens::Director).then(|| AerialPlan::new(&path, &s.blocking));
        Ok(Fly { flight: Flight::new(&path), aerial, glance: Glance::default(), lift: 0.0, room: (f32::MAX, 0.0) })
    }

    pub fn tick(&mut self, s: &mut Session, hero: [f32; 3], dt: f32) -> Step {
        let (Some(pv), Some(body)) = (s.w.pivot, s.body_yaw()) else {
            return Step::End(State::Failed(trf!("NO_PROPERTY", name = "PivotToViewTarget")));
        };
        let fly = self.flight.step(s.plan.pace, dt);
        if fly.done && s.plan.repeat {
            self.flight = self.flight.back();
            return Step::Go(State::Rolling(0.0));
        }
        if fly.done {
            return Step::End(State::Finished);
        }
        let mut ahead = self.flight.ahead(s.plan.pace, ROOM_AHEAD_S);
        // directed, the look's own height over the path: eased, kept under what is above
        let look = self.aerial.as_ref().map(|plan| plan.cue(fly.share, fly.ahead_yaw));
        let want = look.map_or(0.0, |(l, _)| l.lift);
        self.room.1 += dt;
        if self.room.1 >= ROOM_EVERY {
            self.room = (crate::film::headroom(&s.blocking, fly.at, want.max(self.lift)), 0.0);
        }
        let target = want.min(self.room.0);
        self.lift += ((target - self.lift) * (2.0 * dt).min(1.0)).clamp(-LIFT_SPEED * dt, LIFT_SPEED * dt);
        let mut fly = fly;
        fly.at[2] += self.lift;
        ahead[2] += self.lift;
        // into the hero's frame: the body's yaw undone
        let (sn, c) = body.to_radians().sin_cos();
        let (dx, dy) = (fly.at[0] - hero[0], fly.at[1] - hero[1]);
        s.write3(pv, [(dx * c + dy * sn) as f64, (-dx * sn + dy * c) as f64, (fly.at[2] - hero[2]) as f64]);
        s.hold_checks_off();
        let state = match &self.aerial {
            Some(_) => {
                let (l, what) = look.unwrap();
                let (p, y, f) = (l.pitch, l.yaw, l.fov);
                // something passed near is looked at for a moment; soaring, the eagle hunts
                let subjects = s.plan.subjects.as_ref().map(|f| f.lock().unwrap().clone()).unwrap_or_default();
                let hunt = what == Aerial::Eagle;
                match self.glance.look(fly.at, &subjects, dt, s.plan.cuts, hunt) {
                    Some(g) if g.cut => s.cut((g.pitch, g.yaw), FLIGHT_DISTANCE, g.fov.unwrap_or(f)),
                    Some(g) => {
                        s.turn((g.pitch, g.yaw), dt);
                        s.want(None, Some(g.fov.unwrap_or(f)));
                    }
                    None => {
                        s.turn((p, y), dt);
                        s.want(None, Some(f));
                    }
                }
                let label = match (self.glance.hunting(), self.glance.on()) {
                    (true, _) => tr!("AERIAL_HUNT"),
                    (_, true) => tr!("COVER_GLANCE"),
                    _ => what.label(),
                };
                State::Directing(fly.share, label)
            }
            None => {
                // Circling turns the camera about its pivot, which on a flight is on the path, not
                // the hero: there it looks the way ahead instead.
                let lens = if s.plan.lens == Lens::Orbit { Lens::Follow } else { s.plan.lens };
                let head = [hero[0], hero[1], hero[2] + EYE - 90.0];
                if let Some(t) = aim(lens, fly.at, fly.ahead_yaw, head, s.orbit) {
                    s.turn(t, dt);
                }
                State::Rolling(fly.share)
            }
        };
        s.ease([fly.at, ahead], dt);
        Step::Go(state)
    }
}
