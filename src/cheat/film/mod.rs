//! Filming mode (.spec/ROADMAP.md §4.1): the hero walked along a route by the stick's input,
//! the camera turned the way a drone's gimbal would, for recording.
//!
//! Probed in play: `Pawn.ControlInputVector` written at 250 Hz has the game walk the hero
//! itself — its collisions, stairs, slopes, animation, and the body turned to the way — where
//! written positions slide it; `ControlRotation` turns the camera with the rig's own smoothing.
//! The game consumes the input every frame, so when the writes stop the hero stops.
//!
//! The camera follows drone practice (DJI's): every move eased in and out (Cine), the gimbal a
//! critically damped spring under a turn-rate cap, and four modes — left alone, ActiveTrack
//! (looking the way ahead), Spotlight (on the destination while walking) and Circle (turning
//! about the hero at a steady rate). The camera's distance and field of view are eased to the
//! ones asked for and put back after, through the exploration camera's own settings, which the
//! game reads every frame and interpolates toward itself (probed with the game in focus:
//! 484 → 832 cm without a tremor, 70° → 55°). Holding the camera mode's interpolator instead
//! fought the game's own target each frame and shook. The exploration, combat and APC cameras
//! are all set, so a fight on the way keeps the shot.
//!
//! A flight moves the camera alone, the hero left where it stands: the camera mode's
//! `PivotToViewTarget` (an FTransform, its translation 70 cm up at rest) is where the camera
//! turns about, in the hero's frame (its body's yaw), and the game follows it smoothly
//! (probed: 500 cm ahead moved the camera 480 cm the way the hero faced). The path is flown
//! straight between the points, its corners rounded, lifted above the floor.
//!
//! Obstacles, after DJI's APAS: a drone set to film bypasses rather than brakes, its avoidance
//! planned ahead so the path stays smooth. Here: a flight's path is checked before it starts —
//! over low obstacles, round tall ones — and its heights and turns spread out so each begins
//! well before the obstacle. The camera's room behind it is checked every tick, now and a moment
//! ahead, and its distance eased in before the game's own pull-in (a snap in 0.15 s, out in
//! 0.25 s) would have to act, and back out slowly after; that pull-in, if it still acts, is
//! slowed for the take.
//!
//! The parts: `lens` (the camera's modes), `setup` (the card's settings, kept), `route` (a take's
//! way from the setup), `walk` (steering the hero along it), `flight` (the camera's own path),
//! `avoid` (obstacles: a flight's path cleared, the camera's room), `gimbal` (turning the
//! camera), `director` (choosing each beat's shot), `mode` (who moves what), `take` (a take rolling
//! on its own thread).

mod avoid;
mod director;
mod flight;
mod gimbal;
mod lens;
mod mode;
mod record;
mod route;
mod setup;
mod source;
mod subjects;
mod take;
mod walk;

pub use avoid::{clear_flight, headroom, room_behind};
pub use director::{
    judge, openness, Aerial, AerialPlan, Angle, Beat, Coverage, Ctx, Cue, Cuts, Director, Glance, GlanceLook, Motion,
    Rig, Shot, Sight, Situation,
};
pub use flight::{rounded, Flight, Fly, FLIGHT_DISTANCE};
pub use gimbal::{aim, wrap, Axis, ORBIT_DPS};
pub use lens::Lens;
pub use mode::Mode;
pub use record::{pace_at, Recorder, Recording};
pub use source::Source;
pub use subjects::{about, Subject, SubjectFeed, SubjectKind};

/// The guide's route as the overlay draws it (Unreal cm, and its colour): a live take follows it
/// as it changes.
pub type RouteFeed = std::sync::Arc<std::sync::Mutex<(Vec<[f32; 3]>, [u8; 3])>>;
pub use route::{flight_path, path, recording_starts};
pub use setup::{Setup, DISTANCE, FEET, FOV, KEY, NEAR};
pub use take::{roll, Plan, State, Take, Wiring, COUNTDOWN_S, KEY_GRACE, ZOOM_AT};
pub use walk::{Driver, Steer};
