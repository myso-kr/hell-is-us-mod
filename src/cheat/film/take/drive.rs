//! A walk the mod drives: the stick toward the route, the camera aimed by the lens or cued by the
//! director; at the end, a directed take lets the closing shot play out before it stops.

use super::session::Session;
use super::{direct, State, Step};
use crate::film::avoid::ROOM_AHEAD_CM;
use crate::film::gimbal::EYE;
use crate::film::{aim, Director, Driver, Lens};
use std::time::{Duration, Instant};

/// How long a directed take goes on at the end, the hero still, for its closing shot (s).
const CLOSING_HOLD: Duration = Duration::from_secs(4);

pub(super) struct Drive {
    route: Vec<[f32; 3]>,
    driver: Driver,
    director: Option<Director>,
    end: [f32; 3],
    /// When the hero arrived, for the closing shot to play out.
    arrived: Option<Instant>,
}

impl Drive {
    pub fn new(s: &Session) -> Drive {
        Drive::new_on(s, &s.plan.path)
    }

    pub fn tick(&mut self, s: &mut Session, hero: [f32; 3], dt: f32) -> Step {
        let step = if self.arrived.is_some() {
            // arrived: the stick at rest, the closing shot playing out
            crate::film::Steer {
                input: [0.0, 0.0],
                ahead_yaw: s.yaw.angle - 180.0,
                share: 1.0,
                done: false,
                stuck: false,
            }
        } else {
            // a recording walked at its own pace, as a share of the card's
            let pace = match &s.plan.paces {
                Some((start, paces)) => s.plan.pace * crate::film::pace_at(paces, *start, self.driver.along()),
                None => s.plan.pace,
            };
            self.driver.step([hero[0], hero[1]], pace, dt)
        };
        if step.done && s.plan.repeat {
            // back the way it came, the camera carried on
            let back: Vec<[f32; 3]> = self.route.iter().rev().copied().collect();
            *self = Drive::new_on(s, &back);
            return Step::Go(State::Rolling(0.0));
        }
        if step.done {
            if self.director.is_none() {
                return Step::End(State::Finished);
            }
            self.arrived = Some(Instant::now());
            s.write3(s.w.input, [0.0, 0.0, 0.0]);
        }
        if self.arrived.is_some_and(|t| t.elapsed() > CLOSING_HOLD) {
            return Step::End(State::Finished);
        }
        if step.stuck {
            return Step::End(State::Stuck);
        }
        if self.arrived.is_none() && !s.write3(s.w.input, [step.input[0] as f64, step.input[1] as f64, 0.0]) {
            return Step::End(State::Failed(tr!("COULD_NOT_WRITE_THE_HEROS_POSITION").into()));
        }
        let along = if self.arrived.is_some() { self.driver.length() } else { self.driver.along() };
        let heading = if self.arrived.is_some() { s.body_yaw().unwrap_or(step.ahead_yaw) } else { step.ahead_yaw };
        let cue = self.director.as_mut().map(|d| d.cue(along, heading, hero, &s.blocking, dt));
        match &cue {
            Some(cue) => direct::apply(s, cue, dt),
            None => {
                let eye = [hero[0], hero[1], hero[2] + EYE];
                if let Some(t) = aim(s.plan.lens, eye, step.ahead_yaw, self.end, s.orbit) {
                    s.turn(t, dt);
                }
            }
        }
        // the camera's room from the pivot (the hero, raised as the mode has it), now and ahead
        let lift = s.pivot0.map_or(70.0, |p| p[2] as f32);
        let pivot = [hero[0], hero[1], hero[2] + lift];
        let a2 = self.driver.ahead(ROOM_AHEAD_CM);
        s.ease([pivot, [a2[0], a2[1], pivot[2]]], dt);
        Step::Go(match cue {
            Some(c) => State::Directing(step.share, c.shot.label()),
            None => State::Rolling(step.share),
        })
    }

    /// A drive along `route` (the way back, on a repeat).
    fn new_on(s: &Session, route: &[[f32; 3]]) -> Drive {
        Drive {
            route: route.to_vec(),
            driver: Driver::new(route),
            director: (s.plan.lens == Lens::Director).then(|| Director::new(route, &s.blocking)),
            end: *route.last().unwrap_or(&[0.0; 3]),
            arrived: None,
        }
    }
}
