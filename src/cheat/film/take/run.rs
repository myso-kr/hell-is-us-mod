//! The take's loop: a countdown, then at `RATE_HZ` the stick toward the route (a walk) or the
//! pivot along the path (a flight), the camera aimed, its settings eased; until the end, a stop,
//! the player's input, or the hero stuck.

use super::direct;
use super::knobs::{ease_knobs, Knob, BLEND};
use super::{last_input, Plan, State, Wiring, KEY_GRACE, RATE_HZ};
use crate::film::avoid::{ROOM_AHEAD_CM, ROOM_AHEAD_S};
use crate::film::gimbal::{back_of, EYE};
use crate::film::{aim, clear_flight, wrap, Axis, Director, Driver, Flight, Lens, FLIGHT_DISTANCE, ORBIT_DPS};
use crate::mem::Memory;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What a camera setting is, for which a take holds it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Distance,
    Fov,
    Other,
}

pub(super) fn run(plan: &Plan, w: &Wiring, stop: &AtomicBool, state: &Mutex<State>) -> State {
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
    // the director chooses the shots of a walk (a flight looks the way it goes instead)
    let directed = plan.lens == Lens::Director && !plan.flight;
    // a flight keeps the camera close to its pivot unless asked otherwise; the distance is held
    // either way, kept within the room behind the camera (the game's own when none is asked)
    let distance = plan.distance.or(plan.flight.then_some(FLIGHT_DISTANCE));
    let mut knobs: Vec<Knob> = w
        .distance
        .iter()
        .map(|&at| (at, distance, Kind::Distance))
        .chain(w.fov.iter().map(|&at| (at, plan.fov, Kind::Fov)))
        .chain(w.blend.iter().flat_map(|&(i, o)| [(i, Some(BLEND.0), Kind::Other), (o, Some(BLEND.1), Kind::Other)]))
        .filter_map(|(at, want, kind)| {
            let was = read_f(at)?;
            // the distance is always held; the field of view too when the director moves it
            let held = kind == Kind::Distance || (kind == Kind::Fov && directed);
            let want = want.or(held.then_some(was))?;
            Some(Knob { at, was, now: was, want, distance: kind == Kind::Distance, fov: kind == Kind::Fov })
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
    let mut director = directed.then(|| Director::new(&route, &blocking));
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
                *f = f.back();
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
            // the hero: there it looks the way ahead instead.
            let lens = if matches!(plan.lens, Lens::Orbit | Lens::Director) { Lens::Follow } else { plan.lens };
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
            director = directed.then(|| Director::new(&route, &blocking));
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
        let cue = director.as_mut().map(|d| d.cue(driver.along(), s.ahead_yaw, hero, &blocking, dt));
        if let Some(cue) = &cue {
            direct::apply(&game, cue, (&mut pitch, &mut yaw), (w.rotation, w.pivot), &mut knobs, dt);
        } else if let Some((tp, ty)) = aim(plan.lens, eye, s.ahead_yaw, end3, orbit) {
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
        *state.lock().unwrap() = match cue {
            Some(c) => State::Directing(s.share, c.shot),
            None => State::Rolling(s.share),
        };
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
