//! Turning the camera: a damped spring per axis under a turn-rate cap (Cine), where each lens
//! aims, and which way is behind the view.

use super::Lens;

/// The gimbal: its spring (rad/s), its turn-rate cap (°/s), the camera's pitch on the way.
const GIMBAL_OMEGA: f32 = 2.2;
const GIMBAL_MAX_DPS: f32 = 70.0;
const FOLLOW_PITCH: f32 = -8.0;
pub const ORBIT_DPS: f32 = 18.0;
/// Where the camera is taken to look from for Spotlight: the hero's head (cm above the feet).
pub(super) const EYE: f32 = 160.0;

/// Degrees into (−180, 180].
pub fn wrap(d: f32) -> f32 {
    let r = (d + 180.0).rem_euclid(360.0) - 180.0;
    if r == -180.0 {
        180.0
    } else {
        r
    }
}

/// A gimbal axis: a critically damped spring toward its target, its speed capped (Cine).
#[derive(Clone, Copy, Debug, Default)]
pub struct Axis {
    pub angle: f32,
    speed: f32,
}

impl Axis {
    pub fn new(angle: f32) -> Axis {
        Axis { angle, speed: 0.0 }
    }

    pub fn toward(&mut self, target: f32, dt: f32) -> f32 {
        let x = wrap(target - self.angle);
        let w = GIMBAL_OMEGA;
        self.speed += (w * w * x - 2.0 * w * self.speed) * dt;
        self.speed = self.speed.clamp(-GIMBAL_MAX_DPS, GIMBAL_MAX_DPS);
        self.angle = wrap(self.angle + self.speed * dt);
        self.angle
    }
}

/// Where the gimbal aims for `lens` from `eye`: (pitch, yaw), or `None` to leave the camera be.
/// Spotlight looks at `look_at`: the route's end on a walk, the hero on a flight.
pub fn aim(lens: Lens, eye: [f32; 3], ahead_yaw: f32, look_at: [f32; 3], orbit: f32) -> Option<(f32, f32)> {
    match lens {
        Lens::Free => None,
        Lens::Follow => Some((FOLLOW_PITCH, ahead_yaw)),
        Lens::Spotlight => {
            let (dx, dy, dz) = (look_at[0] - eye[0], look_at[1] - eye[1], look_at[2] - eye[2]);
            let flat = dx.hypot(dy);
            if flat < 100.0 {
                return Some((FOLLOW_PITCH, ahead_yaw));
            }
            Some((dz.atan2(flat).to_degrees().clamp(-45.0, 30.0), dy.atan2(dx).to_degrees()))
        }
        Lens::Orbit => Some((FOLLOW_PITCH, orbit)),
    }
}

/// Behind the camera's view, as (pitch, yaw) in degrees.
pub(super) fn back_of(pitch: f32, yaw: f32) -> [f32; 3] {
    let (p, y) = (pitch.to_radians(), yaw.to_radians());
    [-p.cos() * y.cos(), -p.cos() * y.sin(), -p.sin()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gimbal_turns_smoothly_the_short_way() {
        let mut a = Axis::new(170.0);
        let mut last = a.angle;
        for _ in 0..500 {
            let now = a.toward(-170.0, 0.01);
            // across ±180, not the long way round, and never faster than the cap
            assert!(wrap(now - last).abs() <= GIMBAL_MAX_DPS * 0.01 + 1e-3);
            last = now;
        }
        assert!(wrap(a.angle + 170.0).abs() < 1.0, "settled at {}", a.angle);
    }

    #[test]
    fn spotlight_looks_at_the_end() {
        let (p, y) = aim(Lens::Spotlight, [0.0, 0.0, 160.0], 0.0, [0.0, 1000.0, 160.0], 0.0).unwrap();
        assert!((y - 90.0).abs() < 0.01 && p.abs() < 0.01);
        assert_eq!(aim(Lens::Free, [0.0; 3], 0.0, [0.0; 3], 0.0), None);
    }
}
