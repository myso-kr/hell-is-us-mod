//! A directed walk's tick: the director's cue put on the camera — the gimbal eased toward its
//! angles, the pivot moved in the hero's frame, the distance and field of view asked of the knobs.

use super::knobs::Knob;
use crate::film::{Axis, Cue};
use crate::game::process::Game;
use crate::mem::Memory;

/// Put `cue` on the camera this tick (`dt` s): `rotation` the controller's, `pivot` the camera
/// mode's translation, if found.
pub(super) fn apply(
    game: &Game,
    cue: &Cue,
    (pitch, yaw): (&mut Axis, &mut Axis),
    (rotation, pivot): (u64, Option<u64>),
    knobs: &mut [Knob],
    dt: f32,
) {
    let write3 = |at: u64, v: [f64; 3]| {
        let b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        game.write(at, &b)
    };
    let mut roll = [0u8; 8];
    let roll = if game.read(rotation + 16, &mut roll) { f64::from_le_bytes(roll) } else { 0.0 };
    let (p, y) = (pitch.toward(cue.pitch, dt), yaw.toward(cue.yaw, dt));
    write3(rotation, [p.rem_euclid(360.0) as f64, y as f64, roll]);
    if let Some(pv) = pivot {
        write3(pv, cue.pivot.map(|v| v as f64));
    }
    for k in knobs.iter_mut() {
        if k.distance {
            k.want = cue.distance;
        } else if k.fov {
            k.want = cue.fov;
        }
    }
}
