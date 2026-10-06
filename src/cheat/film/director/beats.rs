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

/// How long the opening and the closing are (cm), the shortest a beat may be (cm, ~4 s at a
/// walk), how long one open stretch may run before it changes (cm), how much a rise or fall
/// counts (cm over 10 m), and how much a turn does (degrees over 6 m).
const OPENING: f32 = 700.0;
const CLOSING: f32 = 800.0;
const MIN_BEAT: f32 = 800.0;
const LONG_BEAT: f32 = 2600.0;
const CLIMB: f32 = 180.0;
const FALL: f32 = -250.0;
const TURN: f32 = 45.0;

/// The shot a point calls for, and the side: the outside of a turn (so the camera swings round
/// it), else the open side.
fn call(s: &Sense, along: f32, length: f32) -> (Shot, f32) {
    let open = s.open_left && s.open_right;
    let side = if s.turn.abs() > 10.0 {
        -s.turn.signum()
    } else if s.open_left || !s.open_right {
        1.0
    } else {
        -1.0
    };
    let shot = if along < OPENING {
        if open {
            Shot::CraneDown
        } else {
            Shot::PushIn
        }
    } else if length - along < CLOSING {
        Shot::Dronie
    } else if s.rise > CLIMB {
        Shot::LowRise
    } else if s.rise < FALL {
        Shot::HighFall
    } else if s.turn.abs() > TURN {
        Shot::Arc
    } else if !s.open_left && !s.open_right {
        Shot::Steadicam
    } else if s.view {
        Shot::CraneReveal
    } else if open {
        Shot::SideTrack
    } else {
        Shot::Steadicam
    };
    (shot, side)
}

/// The beats of a route `length` cm long, from its reading.
pub fn plan(senses: &[Sense], length: f32) -> Vec<Beat> {
    // runs of one shot
    let mut beats: Vec<Beat> = Vec::new();
    for (i, s) in senses.iter().enumerate() {
        let (shot, side) = call(s, s.at, length);
        let to = senses.get(i + 1).map_or(length, |n| n.at);
        match beats.last_mut() {
            Some(b) if b.shot == shot => b.to = to,
            _ => beats.push(Beat { from: s.at, to, shot, side }),
        }
    }
    // too short to read: into the longer neighbour (the opening and closing kept)
    loop {
        let short = (0..beats.len())
            .filter(|&i| beats[i].to - beats[i].from < MIN_BEAT)
            .filter(|&i| !matches!(beats[i].shot, Shot::CraneDown | Shot::PushIn | Shot::Dronie))
            .min_by(|&a, &b| (beats[a].to - beats[a].from).total_cmp(&(beats[b].to - beats[b].from)));
        let Some(i) = short else { break };
        let before = i.checked_sub(1).map(|j| beats[j].to - beats[j].from);
        let after = beats.get(i + 1).map(|b| b.to - b.from);
        let into = match (before, after) {
            (Some(x), Some(y)) if y > x => i + 1,
            (Some(_), _) => i - 1,
            (None, Some(_)) => i + 1,
            (None, None) => break,
        };
        let gone = beats.remove(i);
        let j = if into > i { into - 1 } else { into };
        beats[j].from = beats[j].from.min(gone.from);
        beats[j].to = beats[j].to.max(gone.to);
        // neighbours that now share a shot become one
        let mut k = 0;
        while k + 1 < beats.len() {
            if beats[k].shot == beats[k + 1].shot {
                beats[k].to = beats[k + 1].to;
                beats.remove(k + 1);
            } else {
                k += 1;
            }
        }
    }
    // a long open stretch alternates: alongside, then behind, then alongside on the other side
    let mut out: Vec<Beat> = Vec::new();
    for b in beats {
        if b.shot != Shot::SideTrack || b.to - b.from < LONG_BEAT * 1.5 {
            out.push(b);
            continue;
        }
        let parts = ((b.to - b.from) / LONG_BEAT).round().max(2.0) as usize;
        let len = (b.to - b.from) / parts as f32;
        for p in 0..parts {
            let (shot, side) = match p % 3 {
                0 => (Shot::SideTrack, b.side),
                1 => (Shot::Steadicam, b.side),
                _ => (Shot::SideTrack, -b.side),
            };
            out.push(Beat { from: b.from + len * p as f32, to: b.from + len * (p + 1) as f32, shot, side });
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
            &line(6000.0, |at| {
                if (2000.0..3200.0).contains(&at) {
                    Sense { rise: 300.0, ..open() }
                } else if at > 3600.0 {
                    Sense::default()
                } else {
                    open()
                }
            }),
            6000.0,
        );
        assert!(b.iter().any(|x| x.shot == Shot::LowRise), "{b:?}");
        assert!(b.iter().any(|x| x.shot == Shot::Steadicam), "{b:?}");
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
