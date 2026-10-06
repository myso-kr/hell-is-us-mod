//! The camera's settings a take holds: distance and field of view (eased to what is asked and put
//! back after), and the game's own pull-in from walls, slowed.

use crate::film::avoid::{room_behind, ROOM_IN, ROOM_OUT};
use crate::mem::Memory;

/// The game's own pull-in, slowed for a take (s): in, out.
pub(super) const BLEND: (f32, f32) = (0.6, 1.2);

/// How fast the camera's distance and field of view ease to the ones asked for (share a second).
const ZOOM_EASE: f32 = 1.2;

/// A camera setting a take eases to the value asked for, and puts back after. A distance's
/// value is also kept within the room behind the camera.
pub(super) struct Knob {
    pub at: u64,
    pub was: f32,
    pub now: f32,
    pub want: f32,
    pub distance: bool,
    pub fov: bool,
}

/// The camera's settings eased one tick toward what is asked: a distance no further than the
/// room behind the camera from the pivots given (now and a moment ahead) — closing in faster
/// than backing out, as the room is eased itself — the rest straight to their values.
#[allow(clippy::too_many_arguments)]
pub(super) fn ease_knobs(
    knobs: &mut [Knob],
    room: &mut f32,
    b: &crate::obstacles::Blocking,
    pivots: [[f32; 3]; 2],
    back: [f32; 3],
    dt: f32,
    game: &crate::game::process::Game,
) {
    let want = knobs.iter().filter(|k| k.distance).map(|k| k.want).fold(0.0, f32::max);
    if want > 0.0 {
        let free = pivots.iter().map(|&p| room_behind(b, p, back, want)).fold(want, f32::min);
        if *room == f32::MAX {
            *room = free;
        }
        let rate = if free < *room { ROOM_IN } else { ROOM_OUT };
        *room += (free - *room) * (rate * dt).min(1.0);
    }
    for k in knobs.iter_mut() {
        let target = if k.distance { k.want.min(*room) } else { k.want };
        k.now += (target - k.now) * (ZOOM_EASE * dt).min(1.0);
        game.write(k.at, &k.now.to_le_bytes());
    }
}
