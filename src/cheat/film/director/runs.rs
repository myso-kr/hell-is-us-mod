//! Shared by the walk's beats and the flight's: a reading turned into runs of one choice, and the
//! runs too short to read merged into their longer neighbour.

use super::read::Sense;

/// A stretch of the route (cm along it) with one choice, and which way the route turns where the
/// stretch begins (+1 left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Run<T> {
    pub from: f32,
    pub to: f32,
    pub what: T,
    pub turn: f32,
}

/// The runs of `call`'s choice along a reading of a route `length` cm long.
pub(super) fn runs<T: PartialEq + Copy>(senses: &[Sense], length: f32, call: impl Fn(&Sense) -> T) -> Vec<Run<T>> {
    let mut out: Vec<Run<T>> = Vec::new();
    for (i, s) in senses.iter().enumerate() {
        let what = call(s);
        let to = senses.get(i + 1).map_or(length, |n| n.at);
        match out.last_mut() {
            Some(r) if r.what == what => r.to = to,
            _ => out.push(Run { from: s.at, to, what, turn: s.turn.signum() }),
        }
    }
    out
}

/// Runs shorter than `min` (cm) merged into their longer neighbour, shortest first; runs `kept`
/// says so stay whatever their length. Neighbours left with one choice become one run.
pub(super) fn merge<T: PartialEq + Copy>(runs: &mut Vec<Run<T>>, min: f32, kept: impl Fn(T) -> bool) {
    let len = |r: &Run<T>| r.to - r.from;
    loop {
        let short = (0..runs.len())
            .filter(|&i| len(&runs[i]) < min && !kept(runs[i].what))
            .min_by(|&a, &b| len(&runs[a]).total_cmp(&len(&runs[b])));
        let Some(i) = short else { break };
        let before = i.checked_sub(1).map(|j| len(&runs[j]));
        let after = runs.get(i + 1).map(len);
        let into = match (before, after) {
            (Some(x), Some(y)) if y > x => i + 1,
            (Some(_), _) => i - 1,
            (None, Some(_)) => i + 1,
            (None, None) => break,
        };
        let gone = runs.remove(i);
        let j = if into > i { into - 1 } else { into };
        runs[j].from = runs[j].from.min(gone.from);
        runs[j].to = runs[j].to.max(gone.to);
        let mut k = 0;
        while k + 1 < runs.len() {
            if runs[k].what == runs[k + 1].what {
                runs[k].to = runs[k + 1].to;
                runs.remove(k + 1);
            } else {
                k += 1;
            }
        }
    }
}

/// How far into the next stretch's look the take is, `along` a stretch ending at `to`: 0 until
/// `anticipate` before its end, then eased to 1 at it.
pub(super) fn ahead_of(along: f32, to: f32, anticipate: f32) -> f32 {
    let k = (1.0 - (to - along).max(0.0) / anticipate).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}
