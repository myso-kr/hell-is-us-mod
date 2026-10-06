//! A long stretch of one kind of land cut into shots of a few seconds, each a different look (a
//! kilometre of open ground is not one glide). Not to a beat: the shots run long and short, the
//! looks that suit the land come in no set order, never one of the last two again, the side
//! tracked changed each time. Drawn from where they are, so the same way gives the same plan.

use super::look::Aerial;
use crate::film::director::runs::Run;

/// How long a stretch may run before it is cut (cm, ~13 s at a flight's 450 cm/s), and the shortest
/// and longest shots it is cut into (cm, ~7–13 s): long enough to watch, not to tire of.
const LONGEST: f32 = 6000.0;
const SHORTEST_PIECE: f32 = 3000.0;
const LONGEST_PIECE: f32 = 6000.0;
/// How long the eagle soars at least (cm, ~18 s).
const EAGLE: f32 = 8000.0;

/// What a stretch of each kind may turn into, the likelier first (listed twice: likelier still).
fn turns(a: Aerial) -> &'static [Aerial] {
    match a {
        Aerial::Glide => &[
            Aerial::Glide,
            Aerial::Glide,
            Aerial::Track,
            Aerial::Track,
            Aerial::Ascend,
            Aerial::Skim,
            Aerial::TiltUp,
            Aerial::Descend,
            Aerial::Overhead,
            Aerial::FlyBy,
            Aerial::Vertigo,
        ],
        // high open ground: the eagle's
        Aerial::Overhead => {
            &[Aerial::Eagle, Aerial::Eagle, Aerial::Overhead, Aerial::TiltUp, Aerial::Descend, Aerial::Track]
        }
        Aerial::Rise => &[Aerial::Rise, Aerial::Track, Aerial::Ascend, Aerial::Vertigo],
        Aerial::Dive => &[Aerial::Dive, Aerial::Skim, Aerial::Descend],
        _ => &[],
    }
}

/// A number in 0–1 drawn from a place along the way and a salt.
fn draw(at: f32, salt: u32) -> f32 {
    let mut x = (at as u32).wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_535.0
}

/// The long stretches cut and varied.
pub(super) fn vary(stretches: Vec<Run<Aerial>>) -> Vec<Run<Aerial>> {
    let mut out: Vec<Run<Aerial>> = Vec::new();
    let mut side = if draw(0.0, 7) < 0.5 { 1.0 } else { -1.0 };
    for r in stretches {
        let list = turns(r.what);
        if r.to - r.from <= LONGEST || list.is_empty() {
            out.push(r);
            continue;
        }
        let mut from = r.from;
        while from < r.to {
            let mut to = from + SHORTEST_PIECE + (LONGEST_PIECE - SHORTEST_PIECE) * draw(from, 1);
            let pending = {
                let recent: Vec<Aerial> = out.iter().rev().take(2).map(|l| l.what).collect();
                let fresh: Vec<Aerial> = list.iter().copied().filter(|a| !recent.contains(a)).collect();
                let pool = if fresh.is_empty() { list.to_vec() } else { fresh };
                pool[((draw(from, 2) * pool.len() as f32) as usize).min(pool.len() - 1)]
            };
            // the eagle soars long (it climbs, then looks about), the rest as drawn
            if pending == Aerial::Eagle {
                to = to.max(from + EAGLE);
            }
            // nothing too short left over at the end
            if r.to - to < SHORTEST_PIECE {
                to = r.to;
            }
            let what = pending;
            let turn = if matches!(what, Aerial::Track | Aerial::FlyBy | Aerial::Eagle) {
                side = -side;
                side
            } else {
                r.turn
            };
            out.push(Run { from, to, what, turn });
            from = to;
        }
    }
    out
}
