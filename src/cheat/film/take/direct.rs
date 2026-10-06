//! A directed tick: the director's cue put on the camera — cut to at once when the cue says so,
//! else the gimbal eased toward its angles — the pivot moved in the hero's frame, the distance and
//! field of view asked of the session's knobs.

use super::session::Session;
use crate::film::Cue;

pub(super) fn apply(s: &mut Session, cue: &Cue, dt: f32) {
    if cue.cut {
        s.cut((cue.pitch, cue.yaw), cue.distance, cue.fov);
    } else {
        s.turn((cue.pitch, cue.yaw), dt);
        s.want(Some(cue.distance), Some(cue.fov));
    }
    if let Some(pv) = s.w.pivot {
        s.write3(pv, cue.pivot.map(|v| v as f64));
    }
}
