//! The filming key in the game (F8 by default): alone, a take starts or stops (the worker's); with
//! Ctrl, a point where the hero stands is added to the take's points, or the one it stands by taken
//! away (as the marker key); with Ctrl and Shift, the player's own way is recorded until pressed
//! again, then kept as a recording. What it did is said in the banner.

use super::Shared;
use crate::film::{Recorder, Setup, Source, FEET, NEAR};
use std::sync::atomic::Ordering;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};

fn held(key: u16) -> bool {
    // SAFETY: a plain state query.
    unsafe { GetAsyncKeyState(key as i32) as u16 & 0x8000 != 0 }
}

/// The key was pressed in the game, the hero's root at `hero`: what to say, if anything.
pub fn pressed(shared: &Shared, hero: Option<[f32; 3]>) -> Option<String> {
    match (held(VK_CONTROL), held(VK_SHIFT)) {
        (false, _) => {
            shared.film_key.store(true, Ordering::SeqCst);
            None
        }
        (true, true) => Some(record(shared)),
        (true, false) => hero.map(|h| point(shared, [h[0], h[1], h[2] - FEET])),
    }
}

/// Ctrl: a point where the feet are, or the one they stand by taken away.
fn point(shared: &Shared, at: [f32; 3]) -> String {
    let mut setup = shared.film_setup.lock().unwrap();
    let near = setup
        .points
        .iter()
        .position(|q| ((q[0] - at[0]).powi(2) + (q[1] - at[1]).powi(2) + (q[2] - at[2]).powi(2)).sqrt() < NEAR);
    let said = match near {
        Some(i) => {
            setup.points.remove(i);
            trf!("FILM_POINT_REMOVED", k = i + 1, n = setup.points.len())
        }
        None => {
            setup.points.push(at);
            setup.source = Source::Points;
            trf!("FILM_POINT_ADDED", n = setup.points.len())
        }
    };
    setup.save();
    said
}

/// Ctrl+Shift: recording begins; pressed again, it ends and is kept (and chosen as the way).
fn record(shared: &Shared) -> String {
    let mut rec = shared.film_recorder.lock().unwrap();
    match rec.take() {
        None => {
            *rec = Some(Recorder::default());
            tr!("FILM_RECORDING").to_string()
        }
        Some(r) if r.len() < 2 => tr!("FILM_RECORDING_EMPTY").to_string(),
        Some(r) => {
            let mut setup = shared.film_setup.lock().unwrap();
            let name = trf!("FILM_RECORDING_NAME", n = setup.recordings.len() + 1);
            let kept = r.finish(name.clone());
            let (cm, s) = kept.length();
            setup.recordings.push(kept);
            setup.recording = setup.recordings.len() - 1;
            setup.source = Source::Recording;
            setup.save();
            trf!("FILM_RECORDING_KEPT", name = name, m = (cm / 100.0).round() as u32, s = s.round() as u32)
        }
    }
}

/// Each frame: the recording, if one is under way, takes where the feet are.
pub fn follow(shared: &Shared, hero: Option<[f32; 3]>) {
    if let (Some(r), Some(h)) = (shared.film_recorder.lock().unwrap().as_mut(), hero) {
        r.see([h[0], h[1], h[2] - FEET]);
    }
}

/// What stands about the hero, for a take's director: from the last scan, a few times a second.
pub fn subjects(shared: &Shared, hero: Option<[f32; 3]>) {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if last.is_some_and(|t| t.elapsed() < Duration::from_millis(250)) {
        return;
    }
    *last = Some(Instant::now());
    let Some(hero) = hero else { return };
    let about = match shared.snap.lock().unwrap().as_ref() {
        Some(s) => crate::film::about(&s.things, hero),
        None => Vec::new(),
    };
    *shared.film_subjects.lock().unwrap() = about;
}

/// The setup's points, for the maps (what they draw for the take).
pub fn drawn(setup: &Setup) -> Vec<[f32; 3]> {
    match setup.source {
        Source::Recording => setup.recordings.get(setup.recording).map(|r| r.points.clone()).unwrap_or_default(),
        Source::Tour => setup.tour.clone(),
        _ => setup.points.clone(),
    }
}
