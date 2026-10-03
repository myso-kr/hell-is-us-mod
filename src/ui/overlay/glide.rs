//! The hero's pose between readings. The worker reads the game ten times a second
//! (`ui::STEP`); drawn as read, the maps step at that rate. Each new reading starts a
//! glide from where the hero is shown now to where they were read, over the time the
//! readings take to come: the maps move smoothly, a reading behind at most.

use std::time::{Duration, Instant};

/// A pose: position (cm) and yaw (degrees).
pub type Pose = ([f32; 3], f32);

/// Farther than this between readings (cm) is a jump (a door, a fast travel, a load):
/// shown at once rather than glided across.
const JUMP: f32 = 3000.0;
/// The glide's length is the time between the last two readings, kept within these.
const SHORTEST: Duration = Duration::from_millis(60);
const LONGEST: Duration = Duration::from_millis(200);

#[derive(Default)]
pub struct Glide {
    /// Where the glide starts and ends.
    from: Option<Pose>,
    to: Option<Pose>,
    /// When the last reading came, and how long the glide takes.
    at: Option<Instant>,
    span: Duration,
}

impl Glide {
    /// Takes the latest reading (the same one again until a new one comes) and gives
    /// the pose to draw now.
    pub fn see(&mut self, read: Option<Pose>, now: Instant) -> Option<Pose> {
        let Some(read) = read else {
            *self = Self::default();
            return None;
        };
        if self.to != Some(read) {
            let shown = self.shown(now);
            let far = shown.is_none_or(|(p, _)| dist(p, read.0) > JUMP);
            self.span = self.at.map_or(LONGEST, |at| now.duration_since(at)).clamp(SHORTEST, LONGEST);
            self.from = if far { Some(read) } else { shown };
            self.to = Some(read);
            self.at = Some(now);
        }
        self.shown(now)
    }

    fn shown(&self, now: Instant) -> Option<Pose> {
        let (from, to, at) = (self.from?, self.to?, self.at?);
        let t = (now.duration_since(at).as_secs_f32() / self.span.as_secs_f32().max(1e-3)).min(1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        let p = [lerp(from.0[0], to.0[0]), lerp(from.0[1], to.0[1]), lerp(from.0[2], to.0[2])];
        // The short way round.
        let turn = (to.1 - from.1 + 540.0).rem_euclid(360.0) - 180.0;
        Some((p, (from.1 + turn * t).rem_euclid(360.0)))
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glides_between_readings_and_jumps_far_ones() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut g = Glide::default();
        assert_eq!(g.see(Some(([0.0; 3], 0.0)), ms(0)), Some(([0.0; 3], 0.0)));
        // A reading 100 ms later, 100 cm on: halfway there 50 ms after it.
        g.see(Some(([100.0, 0.0, 0.0], 0.0)), ms(100));
        let (p, _) = g.see(Some(([100.0, 0.0, 0.0], 0.0)), ms(150)).unwrap();
        assert!((p[0] - 50.0).abs() < 1.0, "{p:?}");
        let (p, _) = g.see(Some(([100.0, 0.0, 0.0], 0.0)), ms(400)).unwrap();
        assert_eq!(p[0], 100.0);
        // Turning from 350 to 10 degrees goes through 0, not 180.
        g.see(Some(([100.0, 0.0, 0.0], 350.0)), ms(500));
        g.see(Some(([100.0, 0.0, 0.0], 350.0)), ms(700));
        g.see(Some(([100.0, 0.0, 0.0], 10.0)), ms(800));
        let (_, yaw) = g.see(Some(([100.0, 0.0, 0.0], 10.0)), ms(850)).unwrap();
        assert!((yaw - 355.0).abs() < 1.0, "a quarter of the way from 350 to 10: {yaw}");
        // A jump is shown at once.
        let (p, _) = g.see(Some(([90_000.0, 0.0, 0.0], 0.0)), ms(900)).unwrap();
        assert_eq!(p[0], 90_000.0);
    }
}
