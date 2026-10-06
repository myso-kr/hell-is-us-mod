//! The camera's own path on a flight: straight between the points, corners rounded, eased in and
//! out.

/// A flight: how high above the floor points it goes (cm), how fast at full pace (cm/s), how
/// long it eases in (s) and over how far it eases out (cm), and the camera's distance behind
/// the pivot when none is asked for (cm), so the camera is on the path.
pub(super) const FLIGHT_LIFT: f32 = 180.0;
const FLIGHT_SPEED: f32 = 450.0;
const FLIGHT_EASE_IN: f32 = 2.0;
const FLIGHT_EASE_OUT: f32 = 600.0;
pub const FLIGHT_DISTANCE: f32 = 150.0;
/// How far behind the camera's lens, along its look, the hero is carried, and how much higher
/// (cm): out of the frame whatever it looks at, near enough that the land it sees is drawn finely.
pub const HERO_BEHIND: f32 = 300.0;
pub const HERO_ABOVE: f32 = 100.0;

/// Corners rounded: Chaikin's corner cutting, `rounds` times, the ends kept.
pub fn rounded(path: &[[f32; 3]], rounds: usize) -> Vec<[f32; 3]> {
    let mut p = path.to_vec();
    for _ in 0..rounds {
        if p.len() < 3 {
            break;
        }
        let mut q = vec![p[0]];
        for w in p.windows(2) {
            let lerp = |k: f32| [0, 1, 2].map(|i| w[0][i] + (w[1][i] - w[0][i]) * k);
            q.push(lerp(0.25));
            q.push(lerp(0.75));
        }
        q.push(*p.last().unwrap());
        p = q;
    }
    p
}

/// The camera along a flight: where it is each moment, eased in and out.
pub struct Flight {
    path: Vec<[f32; 3]>,
    at_len: Vec<f32>,
    s: f32,
    t: f32,
}

/// One moment of a flight: where the camera turns about, the way ahead (yaw, degrees), the
/// share flown, whether it is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fly {
    pub at: [f32; 3],
    pub ahead_yaw: f32,
    pub share: f32,
    pub done: bool,
}

impl Flight {
    pub fn new(path: &[[f32; 3]]) -> Flight {
        let path = rounded(path, 3);
        let mut at_len = vec![0.0];
        for w in path.windows(2) {
            let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2)).sqrt();
            at_len.push(at_len.last().unwrap() + d);
        }
        Flight { path, at_len, s: 0.0, t: 0.0 }
    }

    /// The same flight the other way, from its end.
    pub fn back(&self) -> Flight {
        let mut p = self.path.clone();
        p.reverse();
        Flight::new(&p)
    }

    pub fn length(&self) -> f32 {
        *self.at_len.last().unwrap_or(&0.0)
    }

    fn point(&self, s: f32) -> [f32; 3] {
        if self.path.len() < 2 {
            return self.path.first().copied().unwrap_or([0.0; 3]);
        }
        let s = s.clamp(0.0, self.length());
        let i = self.at_len.partition_point(|&l| l <= s).clamp(1, self.path.len() - 1);
        let seg = self.at_len[i] - self.at_len[i - 1];
        let k = if seg > 0.0 { (s - self.at_len[i - 1]) / seg } else { 0.0 };
        [0, 1, 2].map(|j| self.path[i - 1][j] + (self.path[i][j] - self.path[i - 1][j]) * k)
    }

    /// Where the flight will be in `secs` at full pace.
    pub fn ahead(&self, pace: f32, secs: f32) -> [f32; 3] {
        self.point(self.s + pace.clamp(0.05, 1.0) * FLIGHT_SPEED * secs)
    }

    pub fn step(&mut self, pace: f32, dt: f32) -> Fly {
        self.t += dt;
        let left = self.length() - self.s;
        let ease_in = {
            let k = (self.t / FLIGHT_EASE_IN).clamp(0.0, 1.0);
            k * k * (3.0 - 2.0 * k)
        };
        let ease_out = (left / FLIGHT_EASE_OUT).clamp(0.0, 1.0).sqrt().max(0.04);
        self.s = (self.s + pace.clamp(0.05, 1.0) * FLIGHT_SPEED * ease_in.max(0.05) * ease_out * dt).min(self.length());
        let at = self.point(self.s);
        let ahead = self.point(self.s + 300.0);
        let ahead_yaw = if (ahead[0] - at[0]).hypot(ahead[1] - at[1]) > 1.0 {
            (ahead[1] - at[1]).atan2(ahead[0] - at[0]).to_degrees()
        } else {
            let back = self.point(self.s - 300.0);
            (at[1] - back[1]).atan2(at[0] - back[0]).to_degrees()
        };
        let share = if self.length() > 0.0 { self.s / self.length() } else { 1.0 };
        Fly { at, ahead_yaw, share, done: self.length() - self.s < 2.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flight_rounds_its_corners_and_lands_at_the_end() {
        let path = [[0.0, 0.0, 200.0], [1000.0, 0.0, 200.0], [1000.0, 1000.0, 400.0]];
        let r = rounded(&path, 3);
        assert_eq!(r[0], path[0]);
        assert_eq!(*r.last().unwrap(), path[2]);
        // the corner is cut: no point of the rounded path is at it
        assert!(r.iter().all(|p| (p[0] - 1000.0).hypot(p[1]) > 50.0));
        let mut f = Flight::new(&path);
        let (mut last, mut n) = (f.step(1.0, 0.02), 0);
        let mut fastest: f32 = 0.0;
        while !last.done {
            let next = f.step(1.0, 0.02);
            let d = ((next.at[0] - last.at[0]).powi(2) + (next.at[1] - last.at[1]).powi(2)).sqrt();
            fastest = fastest.max(d / 0.02);
            last = next;
            n += 1;
            assert!(n < 10_000);
        }
        assert!(
            (last.at[0] - 1000.0).abs() < 3.0 && (last.at[1] - 1000.0).abs() < 3.0 && (last.at[2] - 400.0).abs() < 3.0
        );
        assert!(fastest <= FLIGHT_SPEED + 1.0, "{fastest}");
    }
}
