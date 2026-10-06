//! A flight: the camera's pivot along the cleared path in the hero's frame, the camera aimed by the
//! lens or, directed, looked as an aerial film would (director/aerial.rs).

use super::session::Session;
use super::{State, Step};
use crate::film::avoid::ROOM_AHEAD_S;
use crate::film::gimbal::EYE;
use crate::film::{aim, clear_flight, AerialPlan, Flight, Lens};

pub(super) struct Fly {
    flight: Flight,
    aerial: Option<AerialPlan>,
}

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
        Ok(Fly { flight: Flight::new(&path), aerial })
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
        let ahead = self.flight.ahead(s.plan.pace, ROOM_AHEAD_S);
        // into the hero's frame: the body's yaw undone
        let (sn, c) = body.to_radians().sin_cos();
        let (dx, dy) = (fly.at[0] - hero[0], fly.at[1] - hero[1]);
        s.write3(pv, [(dx * c + dy * sn) as f64, (-dx * sn + dy * c) as f64, (fly.at[2] - hero[2]) as f64]);
        s.hold_checks_off();
        let state = match &self.aerial {
            Some(plan) => {
                let (p, y, f, what) = plan.cue(fly.share, fly.ahead_yaw);
                s.turn((p, y), dt);
                s.want(None, Some(f));
                State::Directing(fly.share, what.label())
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
