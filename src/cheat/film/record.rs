//! Recording the player's own way (Ctrl+Shift+the filming key): where the hero's feet went, a
//! point every `STEP`, with when; kept by name, and played back — walked again at the pace it was
//! walked, or flown.

use std::time::Instant;

/// How far apart the points are taken (cm), the most kept (a long walk: 10 km), and the window the
/// pace is smoothed over (cm).
const STEP: f32 = 50.0;
const MOST: usize = 20_000;
const PACE_WINDOW: f32 = 300.0;
/// The slowest share of the pace a played recording walks at (a stop is not stood out).
const SLOWEST: f32 = 0.2;

/// A recording: its name, the points (cm, the feet), and when each was reached (s from the start).
#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    pub name: String,
    pub points: Vec<[f32; 3]>,
    pub times: Vec<f32>,
}

impl Recording {
    /// How long the way is (cm) and how long it took (s).
    pub fn length(&self) -> (f32, f32) {
        let cm = self.points.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
        (cm, self.times.last().copied().unwrap_or(0.0))
    }

    /// The pace along the way as shares of the fastest (0–1), at each point's distance along it
    /// (cm), smoothed over `PACE_WINDOW`: what a walk played from it goes at.
    pub fn paces(&self) -> Vec<(f32, f32)> {
        let n = self.points.len();
        if n < 2 {
            return Vec::new();
        }
        let mut along = vec![0.0f32];
        for w in self.points.windows(2) {
            along.push(along.last().unwrap() + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
        }
        let speed: Vec<f32> = (0..n)
            .map(|i| {
                let (mut a, mut b) = (i, i);
                while a > 0 && along[i] - along[a] < PACE_WINDOW / 2.0 {
                    a -= 1;
                }
                while b + 1 < n && along[b] - along[i] < PACE_WINDOW / 2.0 {
                    b += 1;
                }
                let dt = self.times[b] - self.times[a];
                if dt > 0.0 {
                    (along[b] - along[a]) / dt
                } else {
                    0.0
                }
            })
            .collect();
        let top = speed.iter().copied().fold(1.0f32, f32::max);
        along.into_iter().zip(speed).map(|(s, v)| (s, (v / top).clamp(SLOWEST, 1.0))).collect()
    }
}

/// The share of the pace `along` cm into a played recording (its paces offset by `start`, where
/// the recording begins in the take's path): the nearest taken before it.
pub fn pace_at(paces: &[(f32, f32)], start: f32, along: f32) -> f32 {
    let s = along - start;
    match paces.partition_point(|&(a, _)| a <= s) {
        0 => paces.first().map_or(1.0, |p| p.1),
        i => paces[i - 1].1,
    }
}

/// A recording under way.
pub struct Recorder {
    points: Vec<[f32; 3]>,
    times: Vec<f32>,
    start: Instant,
}

impl Default for Recorder {
    fn default() -> Recorder {
        Recorder { points: Vec::new(), times: Vec::new(), start: Instant::now() }
    }
}

impl Recorder {
    /// Where the feet are now: kept when `STEP` from the last point kept.
    pub fn see(&mut self, feet: [f32; 3]) {
        if self.points.len() >= MOST {
            return;
        }
        let far = self.points.last().is_none_or(|q| ((q[0] - feet[0]).hypot(q[1] - feet[1])) >= STEP);
        if far {
            self.points.push(feet);
            self.times.push(self.start.elapsed().as_secs_f32());
        }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn finish(self, name: String) -> Recording {
        Recording { name, points: self.points, times: self.times }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_keeps_its_pace() {
        // 10 m in 10 s, then 10 m in 2 s
        let mut points = Vec::new();
        let mut times = Vec::new();
        for i in 0..=40 {
            let x = i as f32 * 50.0;
            points.push([x, 0.0, 0.0]);
            times.push(if x <= 1000.0 { x / 100.0 } else { 10.0 + (x - 1000.0) / 500.0 });
        }
        let r = Recording { name: "r".into(), points, times };
        let (cm, s) = r.length();
        assert!((cm - 2000.0).abs() < 1.0 && (s - 12.0).abs() < 0.01);
        let p = r.paces();
        let slow = pace_at(&p, 0.0, 400.0);
        let fast = pace_at(&p, 0.0, 1700.0);
        assert!(fast > 0.95 && slow < 0.3, "slow {slow}, fast {fast}");
        // offset: the recording starting 500 cm into the take
        assert_eq!(pace_at(&p, 500.0, 900.0), slow);
    }

    #[test]
    fn a_recorder_takes_a_point_every_step() {
        let mut r = Recorder::default();
        for i in 0..100 {
            r.see([i as f32 * 10.0, 0.0, 0.0]);
        }
        assert_eq!(r.len(), 20);
    }
}
