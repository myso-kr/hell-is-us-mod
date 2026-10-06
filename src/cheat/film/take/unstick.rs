//! A walk caught on a wall or in a narrow place is not given up at once: first the hero backs off
//! to one side and the other (a step round what holds it), then it is lifted a little ahead along
//! the route; only when that fails too does the take stop.

/// How long a step aside lasts (s), how many are tried before a hop, how far a hop goes along the
/// route (cm) and how high above it it sets the hero down (cm), how many hops before giving up,
/// and how long the walk must go well for the count to start again (s).
const ASIDE_FOR: f32 = 0.8;
const ASIDES: u32 = 2;
pub(super) const HOP: f32 = 250.0;
pub(super) const HOP_ABOVE: f32 = 30.0;
const HOPS: u32 = 3;
const FORGIVE: f32 = 15.0;

/// What to do about being stuck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Way {
    /// Walk on as steered.
    On,
    /// The stick this way instead.
    Aside([f32; 2]),
    /// Lift the hero `HOP` along the route.
    Hop,
    /// Nothing more to try.
    GiveUp,
}

#[derive(Default)]
pub(super) struct Unstick {
    t: f32,
    asides: u32,
    hops: u32,
    /// The step aside under way: its stick, until when.
    aside: Option<([f32; 2], f32)>,
    /// When it was last stuck.
    last: f32,
}

impl Unstick {
    /// The way on, the walk steering `input` and `stuck` as it says.
    pub fn way(&mut self, input: [f32; 2], stuck: bool, dt: f32) -> Way {
        self.t += dt;
        if let Some((stick, until)) = self.aside {
            if self.t < until {
                return Way::Aside(stick);
            }
            self.aside = None;
        }
        if !stuck {
            return Way::On;
        }
        if self.t - self.last > FORGIVE {
            (self.asides, self.hops) = (0, 0);
        }
        self.last = self.t;
        if self.asides < ASIDES {
            // back off and to one side, then the other
            let side = if self.asides % 2 == 0 { 1.0 } else { -1.0 };
            let l = input[0].hypot(input[1]).max(1e-3);
            let (fx, fy) = (input[0] / l, input[1] / l);
            let (x, y) = (-fx * 0.5 - fy * side, -fy * 0.5 + fx * side);
            let n = x.hypot(y);
            self.asides += 1;
            self.aside = Some(([x / n * 0.8, y / n * 0.8], self.t + ASIDE_FOR));
            return Way::Aside([x / n * 0.8, y / n * 0.8]);
        }
        if self.hops < HOPS {
            self.hops += 1;
            self.asides = 0;
            return Way::Hop;
        }
        Way::GiveUp
    }
}

/// The route's height near (x, y): its nearest point's (cm).
pub(super) fn height_on(route: &[[f32; 3]], at: [f32; 2]) -> Option<f32> {
    route
        .iter()
        .min_by(|a, b| (a[0] - at[0]).hypot(a[1] - at[1]).total_cmp(&(b[0] - at[0]).hypot(b[1] - at[1])))
        .map(|p| p[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stuck_steps_aside_twice_then_hops_then_gives_up() {
        let mut u = Unstick::default();
        let mut seen = Vec::new();
        for _ in 0..2000 {
            let w = u.way([1.0, 0.0], true, 0.01);
            if seen.last().is_none_or(|l: &Way| std::mem::discriminant(l) != std::mem::discriminant(&w)) {
                seen.push(w);
            }
            if w == Way::GiveUp {
                break;
            }
        }
        let kinds: Vec<&str> = seen
            .iter()
            .map(|w| match w {
                Way::Aside(_) => "aside",
                Way::Hop => "hop",
                Way::GiveUp => "give up",
                Way::On => "on",
            })
            .collect();
        assert_eq!(kinds.first(), Some(&"aside"));
        assert!(kinds.contains(&"hop"));
        assert_eq!(kinds.last(), Some(&"give up"));
        // a step aside goes back and sideways, never on into the wall
        if let Some(Way::Aside(s)) = seen.first() {
            assert!(s[0] < 0.0 && s[1].abs() > 0.5, "{s:?}");
        }
    }

    #[test]
    fn a_walk_that_goes_well_is_forgiven() {
        let mut u = Unstick::default();
        for _ in 0..3 {
            u.way([1.0, 0.0], true, 0.01);
            for _ in 0..100 {
                u.way([1.0, 0.0], false, 0.01);
            }
        }
        // after a long good stretch, a step aside again rather than a hop
        for _ in 0..2000 {
            u.way([1.0, 0.0], false, 0.01);
        }
        assert!(matches!(u.way([1.0, 0.0], true, 0.01), Way::Aside(_)));
    }
}
