//! The take's loop: a countdown, then at `RATE_HZ` one tick of the mode's own (`drive`, `fly`) on the shared session; until the mode ends it, a stop, the hero changed, or — when the
//! mod moves the hero or the camera alone — the player's input. Then everything put back.

use super::drive::Drive;
use super::fly::Fly;
use super::input::{Touch, Watch};
use super::session::Session;
use super::{Plan, State, Step, Wiring, KEY_GRACE, RATE_HZ};
use crate::film::Mode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The mode a take rolls in, with its own state.
enum Runner {
    Drive(Drive),
    Fly(Fly),
}

pub(super) fn run(plan: &Plan, w: &Wiring, stop: &AtomicBool, state: &Mutex<State>) -> State {
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
    let mut s = match Session::open(plan, w) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let mut runner = match plan.mode {
        Mode::Walk => Runner::Drive(Drive::new(&s)),
        Mode::Flight => match Fly::new(&s) {
            Ok(f) => Runner::Fly(f),
            Err(e) => return e,
        },
    };
    let mut touch = Watch::new();
    let tick = Duration::from_secs_f64(1.0 / RATE_HZ);
    let mut next = Instant::now();
    let mut last = Instant::now();
    let outcome = loop {
        if stop.load(Ordering::SeqCst) {
            break State::Stopped;
        }
        let dt = last.elapsed().as_secs_f32().min(0.05);
        {
            match touch.touch(s.drift(), dt) {
                Some(Touch::Key(vk)) => {
                    crate::logfile::line(&format!("film: stopped by the player: key 0x{vk:02x}"));
                    break State::Interrupted;
                }
                Some(Touch::Turned(d)) => {
                    crate::logfile::line(&format!("film: stopped by the player: the camera turned {d:.1}°"));
                    break State::Interrupted;
                }
                None => {}
            }
        }
        let Some(hero) = s.hero() else { break State::Failed(tr!("THE_HERO_CHANGED").into()) };
        last = Instant::now();
        s.tick(dt);
        let step = match &mut runner {
            Runner::Drive(d) => d.tick(&mut s, hero, dt),
            Runner::Fly(f) => f.tick(&mut s, hero, dt),
        };
        match step {
            Step::Go(now) => *state.lock().unwrap() = now,
            Step::End(end) => break end,
        }
        next += tick;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    };
    s.restore();
    outcome
}
