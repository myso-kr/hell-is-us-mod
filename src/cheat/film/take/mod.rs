//! A take: what it is (`Plan`), where it writes (`Wiring`), where it stands (`State`), and the
//! thread that rolls it (`roll`; the loop in `run`, the camera's settings in `knobs`).

mod carry;
mod cover;
mod direct;
mod drive;
mod fly;
mod input;
mod knobs;
mod run;
mod session;

use crate::player::PoseSource;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use super::Lens;

/// A take: the route (cm), the pace (the stick's length, 0–1), the camera, its distance (cm)
/// and field of view (degrees) (`None`, as they are), whether to walk back and forth until
/// stopped, the countdown (s), who moves what, a recording's pace, how shots change, and what
/// stands about the hero.
#[derive(Clone, Debug)]
pub struct Plan {
    pub path: Vec<[f32; 3]>,
    pub pace: f32,
    pub lens: Lens,
    pub distance: Option<f32>,
    pub fov: Option<f32>,
    pub repeat: bool,
    pub countdown: u32,
    pub mode: crate::film::Mode,
    /// A recording played: its pace along it (shares of the fastest), and where in the path it
    /// begins (cm): a walk goes at the pace it was walked.
    pub paces: Option<(f32, Vec<(f32, f32)>)>,
    /// How the director changes shots, and what stands about the hero for it to look at.
    pub cuts: crate::film::Cuts,
    pub subjects: Option<crate::film::SubjectFeed>,
}

/// What one tick of a take came to: on, standing so; or over.
pub(super) enum Step {
    Go(State),
    End(State),
}

/// Where a take writes and reads, found by the worker: the hero's `ControlInputVector`, the
/// controller's `ControlRotation`, the pose, the cameras' (exploration, combat, APC)
/// `DefaultDistanceFromPlayer` and `FieldOfView` (floats), the camera mode's pivot translation
/// (`PivotToViewTarget` + 0x20, three doubles) and the hero root's `RelativeRotation`.
/// Where in `CameraToPivotTranslationInterpolator` the camera's distance is held (its current,
/// start and target, twice), found on build 24045435.
pub const ZOOM_AT: [u64; 6] = [0x40, 0x68, 0x80, 0xb0, 0xd8, 0xf0];

/// Where the hero's root component keeps its place (its location and its world transform's
/// translation, doubles) and the movement component its velocity: written as a teleport is.
#[derive(Clone, Copy, Debug)]
pub struct HeroRoot {
    pub location: u64,
    pub world: u64,
    pub velocity: Option<u64>,
    /// The movement component's `MovementMode` (a byte): set falling once the hero is put back,
    /// so the game moves it there at once (a still hero is otherwise not redrawn until it moves).
    pub mode: Option<u64>,
    /// The hero's mesh's `RelativeScale3D` (doubles): shrunk to nothing while carried, so it is
    /// not seen should the camera glimpse it (its weapons and clothes are attached to it, and go
    /// with it); the game takes it up as the carried hero's place is updated.
    pub scale: Option<u64>,
}

pub struct Wiring {
    pub input: u64,
    pub rotation: u64,
    pub pose: PoseSource,
    pub distance: Vec<u64>,
    pub fov: Vec<u64>,
    pub pivot: Option<u64>,
    pub body: Option<u64>,
    /// The camera mode's `PenetrationBlendInTime` and `PenetrationBlendOutTime` (floats).
    pub blend: Option<(u64, u64)>,
    /// The camera mode's byte of `bValidateSafeLoc` (bit 0) and `bPreventCameraPenetration`
    /// (bit 1): cleared on a flight, which plans its own way round obstacles.
    pub safety: Option<u64>,
    /// The camera mode's `CameraToPivotTranslationInterpolator` doubles that hold the camera's
    /// distance behind the pivot (negative cm; `ZOOM_AT` into it, build 24045435): written for a
    /// cut, where the game would ease the distance.
    pub zoom: Option<[u64; 6]>,
    /// A flight's hand on the hero, to carry it behind the camera and bring it back after.
    pub hero_root: Option<HeroRoot>,
    /// What stands in the way (obstacles.rs), for the flight's path and the camera's room.
    pub scene: std::sync::Arc<crate::obstacles::Scene>,
}

/// Where a take is.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum State {
    #[default]
    Idle,
    /// Seconds to the start.
    Countdown(u32),
    /// Rolling: how much of the route is walked (0–1).
    Rolling(f32),
    /// Rolling, directed: how much is walked, and the shot playing (its name).
    Directing(f32, &'static str),
    Finished,
    Stuck,
    /// Stopped by the player's keyboard or mouse.
    Interrupted,
    Stopped,
    Failed(String),
}

impl State {
    pub fn text(&self) -> String {
        match self {
            State::Idle => tr!("FILM_STATE_IDLE").into(),
            State::Countdown(n) => trf!("FILM_STATE_COUNTDOWN", n = n),
            State::Rolling(k) => trf!("FILM_STATE_ROLLING", pct = (k * 100.0).round() as u32),
            State::Directing(k, shot) => trf!("FILM_STATE_DIRECTING", pct = (k * 100.0).round() as u32, shot = shot),
            State::Finished => tr!("FILM_STATE_FINISHED").into(),
            State::Stuck => tr!("FILM_STATE_STUCK").into(),
            State::Interrupted => tr!("FILM_STATE_INTERRUPTED").into(),
            State::Stopped => tr!("FILM_STATE_STOPPED").into(),
            State::Failed(e) => trf!("FILM_STATE_FAILED", e = e),
        }
    }

    pub fn rolling(&self) -> bool {
        matches!(self, State::Countdown(_) | State::Rolling(_) | State::Directing(..))
    }
}

/// Writes a second.
pub(super) const RATE_HZ: f64 = 250.0;
/// The countdown from the card, which leaves time to switch to the game; from the key, a
/// moment for the key to be let go of before input is watched.
pub const COUNTDOWN_S: u32 = 3;
pub const KEY_GRACE: Duration = Duration::from_millis(600);

/// A take running: stopping it ends the thread, which leaves the stick at rest.
pub struct Take {
    stop: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

impl Take {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Whether the take has ended and put everything back.
    pub fn ended(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

impl Drop for Take {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Roll a take on a thread of its own, the game opened for writing there: a countdown, then
/// the walk at `RATE_HZ` until the end, a stop, the player's input, or the hero stuck.
pub fn roll(plan: Plan, wiring: Wiring, state: Arc<Mutex<State>>, ended: Arc<Mutex<Option<Instant>>>) -> Take {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let done = Arc::new(AtomicBool::new(false));
    let done_flag = done.clone();
    *state.lock().unwrap() = State::Countdown(plan.countdown.max(1));
    std::thread::spawn(move || {
        let end = run::run(&plan, &wiring, &flag, &state);
        *state.lock().unwrap() = end;
        *ended.lock().unwrap() = Some(Instant::now());
        done_flag.store(true, Ordering::SeqCst);
    });
    Take { stop, done }
}
