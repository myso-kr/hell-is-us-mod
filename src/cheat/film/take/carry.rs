//! A flight carries the hero with the camera: the game draws the land finely only near the hero,
//! so it goes along — just behind the camera and above it, straight back along the camera's look.
//! A point behind the lens along its own axis is never in the frame, whatever the camera looks at
//! and however wide its lens; so it is put there exactly each tick, not eased after (lagging, it
//! would swing into the frame as the camera turns). Its mesh is shrunk as well (session.rs).

use super::session::Session;
use crate::film::{above_floor, FEET, FEET_OVER_FLOOR, FLIGHT_DISTANCE, HERO_ABOVE, HERO_BEHIND};

#[derive(Default)]
pub(super) struct Carry;

impl Carry {
    /// The hero put behind the camera turning about `pivot` (the flight's point, at the camera's
    /// height), the camera looking as the gimbal holds it: `HERO_BEHIND` back from the lens along
    /// its look, `HERO_ABOVE` higher. The hero's root, as put.
    pub fn place(&self, s: &Session, hero: [f32; 3], pivot: [f32; 3]) -> [f32; 3] {
        if s.w.hero_root.is_none() {
            return hero;
        }
        let (p, y) = (s.pitch.angle.to_radians(), s.yaw.angle.to_radians());
        let look = [p.cos() * y.cos(), p.cos() * y.sin(), p.sin()];
        // the lens: the camera's distance back from the pivot along its look
        let back = FLIGHT_DISTANCE + HERO_BEHIND;
        let mut at = [pivot[0] - look[0] * back, pivot[1] - look[1] * back, pivot[2] - look[2] * back + HERO_ABOVE];
        // never into the ground or the water (it would drown): its feet above the floor
        if let Some(low) = above_floor(&s.blocking, [at[0], at[1], at[2] - FEET], FEET_OVER_FLOOR) {
            at[2] = at[2].max(low + FEET);
        }
        s.place_hero(at.map(|v| v as f64));
        at
    }
}
