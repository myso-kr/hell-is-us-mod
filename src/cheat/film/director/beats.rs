//! Cutting a read route into beats and giving each a shot (.spec/FILMING-RESEARCH.md §4): the
//! start and the end their own, climbing and going down, turns, corridors, views, long open
//! stretches; then beats too short to read merged into a neighbour, and long ones varied.

use super::read::Sense;
use super::shot::Shot;

/// A stretch of the route (cm along it) and its shot, on a side of the way (+1 left, −1 right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub from: f32,
    pub to: f32,
    pub shot: Shot,
    pub side: f32,
}

/// How long the opening and the closing are (cm), the shortest a beat may be (cm, ~4 s at the
/// hero's 300–450 cm/s), how long one open stretch may run before it changes (cm), the shortest
/// route given beats at all (cm; under it, one follow), how much a rise or fall counts (cm over
/// 10 m), and how much a turn does (degrees between the 10 m before and after).
const OPENING: f32 = 1500.0;
const CLOSING: f32 = 1800.0;
const MIN_BEAT: f32 = 1400.0;
const LONG_BEAT: f32 = 2600.0;
const SHORT_ROUTE: f32 = 3000.0;
const CLIMB: f32 = 180.0;
const FALL: f32 = -250.0;
const TURN: f32 = 60.0;

/// The shot a point calls for (and, for a turn, which way it turns: +1 left).
fn call(s: &Sense, along: f32, length: f32) -> (Shot, f32) {
    let open = s.open_left && s.open_right;
    let shot = if along < OPENING {
        if open {
            Shot::CraneDown
        } else {
            Shot::PushIn
        }
    } else if length - along < CLOSING {
        Shot::Dronie
    } else if !s.open_left && !s.open_right {
        // a corridor first: its navmesh corners are no turns to swing round
        Shot::Steadicam
    } else if s.rise > CLIMB {
        Shot::LowRise
    } else if s.rise < FALL {
        Shot::HighFall
    } else if s.turn.abs() > TURN {
        Shot::Arc
    } else if s.view {
        Shot::CraneReveal
    } else if open {
        Shot::SideTrack
    } else {
        Shot::Follow
    };
    (shot, s.turn.signum())
}

/// The beats of a route `length` cm long, from its reading.
pub fn plan(senses: &[Sense], length: f32) -> Vec<Beat> {
    // The line's side (the 180° rule): the side more often open, kept for the whole take and
    // changed only by a move that carries the camera across (an arc, the long stretch's turn).
    let left = senses.iter().filter(|s| s.open_left).count();
    let right = senses.iter().filter(|s| s.open_right).count();
    let mut line = if left >= right { 1.0 } else { -1.0 };
    if length < SHORT_ROUTE {
        return vec![Beat { from: 0.0, to: length, shot: Shot::Follow, side: line }];
    }
    // runs of one shot (a turn's run: which way it turns, from its first point)
    let mut beats: Vec<(Beat, f32)> = Vec::new();
    for (i, s) in senses.iter().enumerate() {
        let (shot, turn) = call(s, s.at, length);
        let to = senses.get(i + 1).map_or(length, |n| n.at);
        match beats.last_mut() {
            Some((b, _)) if b.shot == shot => b.to = to,
            _ => beats.push((Beat { from: s.at, to, shot, side: 0.0 }, turn)),
        }
    }
    // too short to read: into the longer neighbour (the opening and closing kept)
    loop {
        let len = |b: &Beat| b.to - b.from;
        let short = (0..beats.len())
            .filter(|&i| len(&beats[i].0) < MIN_BEAT)
            .filter(|&i| !matches!(beats[i].0.shot, Shot::CraneDown | Shot::PushIn | Shot::Dronie))
            .min_by(|&a, &b| len(&beats[a].0).total_cmp(&len(&beats[b].0)));
        let Some(i) = short else { break };
        let before = i.checked_sub(1).map(|j| len(&beats[j].0));
        let after = beats.get(i + 1).map(|b| len(&b.0));
        let into = match (before, after) {
            (Some(x), Some(y)) if y > x => i + 1,
            (Some(_), _) => i - 1,
            (None, Some(_)) => i + 1,
            (None, None) => break,
        };
        let (gone, _) = beats.remove(i);
        let j = if into > i { into - 1 } else { into };
        beats[j].0.from = beats[j].0.from.min(gone.from);
        beats[j].0.to = beats[j].0.to.max(gone.to);
        // neighbours that now share a shot become one
        let mut k = 0;
        while k + 1 < beats.len() {
            if beats[k].0.shot == beats[k + 1].0.shot {
                beats[k].0.to = beats[k + 1].0.to;
                beats.remove(k + 1);
            } else {
                k += 1;
            }
        }
    }
    // the sides along the take; a long open stretch alongside, then behind, then on the other
    let mut out: Vec<Beat> = Vec::new();
    for (mut b, turn) in beats {
        if b.shot == Shot::Arc && turn != 0.0 {
            // round the outside of the turn: the camera crosses the line, and stays across
            line = -turn;
        }
        b.side = line;
        if b.shot != Shot::SideTrack || b.to - b.from < LONG_BEAT * 1.5 {
            out.push(b);
            continue;
        }
        let parts = ((b.to - b.from) / LONG_BEAT).round().max(2.0) as usize;
        let len = (b.to - b.from) / parts as f32;
        for p in 0..parts {
            let shot = match p % 3 {
                0 => Shot::SideTrack,
                1 => Shot::Follow,
                _ => {
                    // behind, then across: a continuous move takes the camera over the line
                    line = -line;
                    Shot::SideTrack
                }
            };
            out.push(Beat { from: b.from + len * p as f32, to: b.from + len * (p + 1) as f32, shot, side: line });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(length: f32, f: impl Fn(f32) -> Sense) -> Vec<Sense> {
        (0..=(length / 200.0) as usize).map(|i| Sense { at: i as f32 * 200.0, ..f(i as f32 * 200.0) }).collect()
    }

    fn open() -> Sense {
        Sense { open_left: true, open_right: true, ..Default::default() }
    }

    #[test]
    fn an_open_walk_opens_with_a_crane_and_closes_with_a_dronie() {
        let b = plan(&line(8000.0, |_| open()), 8000.0);
        assert_eq!(b.first().unwrap().shot, Shot::CraneDown);
        assert_eq!(b.last().unwrap().shot, Shot::Dronie);
        assert!(b.iter().any(|x| x.shot == Shot::SideTrack));
        for w in b.windows(2) {
            assert_ne!(w[0].shot, w[1].shot, "the same shot twice running");
            assert!((w[0].to - w[1].from).abs() < 1e-3, "a gap or overlap");
        }
        for x in &b[1..b.len() - 1] {
            assert!(x.to - x.from >= MIN_BEAT - 1e-3, "{x:?} too short");
        }
    }

    #[test]
    fn climbing_and_corridors_have_their_shots() {
        let b = plan(
            &line(8000.0, |at| {
                if (2000.0..3800.0).contains(&at) {
                    Sense { rise: 300.0, ..open() }
                } else if (4200.0..6400.0).contains(&at) {
                    Sense::default()
                } else {
                    open()
                }
            }),
            8000.0,
        );
        assert!(b.iter().any(|x| x.shot == Shot::LowRise), "{b:?}");
        assert!(b.iter().any(|x| x.shot == Shot::Steadicam), "{b:?}");
    }

    #[test]
    fn a_short_route_is_one_follow_and_the_line_holds() {
        let b = plan(&line(2000.0, |_| open()), 2000.0);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].shot, Shot::Follow);
        // no beat changes side but an arc or the long stretch's turn
        let b = plan(&line(20000.0, |_| open()), 20000.0);
        for w in b.windows(2) {
            if w[0].side != w[1].side {
                assert!(w[1].shot == Shot::Arc || (w[0].shot == Shot::Follow && w[1].shot == Shot::SideTrack), "{w:?}");
            }
        }
    }

    #[test]
    fn a_blip_is_merged_away() {
        let b = plan(
            &line(6000.0, |at| if (3000.0..3300.0).contains(&at) { Sense { turn: 80.0, ..open() } } else { open() }),
            6000.0,
        );
        assert!(b.iter().all(|x| x.shot != Shot::Arc), "a 3 m turn is no beat: {b:?}");
    }
}
