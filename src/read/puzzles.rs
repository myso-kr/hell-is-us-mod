//! The puzzles near the hero and their answers, read from the game as it holds them
//! (.spec/GUIDE.md §27, F6):
//! - a dial puzzle (`DialPuzzleActionComponent.Dials` → each `DialComponent`'s
//!   `DialState` of `NbDialState`, and `DialSolution`) — the vault doors among them;
//! - a keypad or a computer (`KeypadRuneComponent.Rune.ExpectedCode`);
//! - an item placement (`ItemPlacementActionComponent.Solution`: the item, or a
//!   condition listing the items).
//!
//! The components are found by the quest pass as it walks every object (quests.rs);
//! this reads the ones within reach.

use crate::mem::{self, Memory};
use crate::names::{Names, CLASS, OUTER};

/// Which kind of puzzle a component is, by its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dial,
    Keypad,
    Placement,
}

impl Kind {
    pub fn of(m: &dyn Memory, n: &Names, comp: u64) -> Option<Kind> {
        let class = mem::read_u64(m, comp + CLASS)?;
        match n.object(m, class)?.as_str() {
            "DialPuzzleActionComponent" => Some(Kind::Dial),
            "KeypadRuneComponent" => Some(Kind::Keypad),
            _ if n.is_a(m, comp, "ItemPlacementActionComponent") => Some(Kind::Placement),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Dial => tr!("다이얼 퍼즐"),
            Kind::Keypad => tr!("키패드·암호"),
            Kind::Placement => tr!("물건 놓기 퍼즐"),
        }
    }
}

/// One dial: where it points, where it should, of how many places.
#[derive(Clone, Debug, PartialEq)]
pub struct Dial {
    pub now: u8,
    pub want: u8,
    pub places: u8,
}

impl Dial {
    /// Presses to get there, the way the dial turns.
    pub fn turns(&self) -> u8 {
        if self.places == 0 {
            return 0;
        }
        (self.want + self.places - self.now % self.places) % self.places
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Dials(Vec<Dial>),
    Code(String),
    /// The items it takes, by their data asset names.
    Items(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Puzzle {
    /// The component: what the panel keeps a revealed answer by.
    pub id: u64,
    pub kind: Kind,
    /// The actor's class, for a name.
    pub class: String,
    pub at: [f32; 3],
    pub answer: Answer,
    pub solved: bool,
}

/// Where an actor stands: its root component's RelativeLocation (doubles).
fn location(m: &dyn Memory, n: &Names, actor: u64) -> Option<[f32; 3]> {
    let root = mem::read_u64(m, actor + n.field(m, actor, "RootComponent")?.offset as u64).filter(|&p| mem::plausible(p))?;
    let at = root + n.field(m, root, "RelativeLocation")?.offset as u64;
    let mut b = [0u8; 24];
    m.read(at, &mut b).then_some(())?;
    let d = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()) as f32;
    let p = [d(0), d(1), d(2)];
    p.iter().all(|v| v.is_finite()).then_some(p)
}

fn byte(m: &dyn Memory, n: &Names, obj: u64, name: &str) -> Option<u8> {
    let f = n.field(m, obj, name)?;
    let mut b = [0u8; 1];
    m.read(obj + f.offset as u64, &mut b).then_some(b[0])
}

/// Whether an interaction component has been used (`bHasBeenActivated`).
fn activated(m: &dyn Memory, n: &Names, comp: u64) -> bool {
    byte(m, n, comp, "bHasBeenActivated").is_some_and(|b| b & 1 != 0)
}

/// The actor's component in its property `name`, if it holds one.
fn component(m: &dyn Memory, n: &Names, actor: u64, name: &str) -> Option<u64> {
    mem::read_u64(m, actor + n.field(m, actor, name)?.offset as u64).filter(|&p| mem::plausible(p))
}

fn objects(m: &dyn Memory, at: u64) -> Vec<u64> {
    let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return Vec::new() };
    if !mem::plausible(data) || num > 64 {
        return Vec::new();
    }
    (0..num as u64).filter_map(|i| mem::read_u64(m, data + i * 8).filter(|&p| mem::plausible(p))).collect()
}

/// A puzzle component's puzzle, when it can be read.
pub fn read(m: &dyn Memory, n: &Names, comp: u64, kind: Kind) -> Option<Puzzle> {
    let actor = mem::read_u64(m, comp + OUTER).filter(|&p| mem::plausible(p))?;
    let class = mem::read_u64(m, actor + CLASS).and_then(|c| n.object(m, c)).unwrap_or_default();
    let at = location(m, n, actor)?;
    let (answer, solved) = match kind {
        Kind::Dial => {
            let dials: Vec<Dial> = objects(m, comp + n.field(m, comp, "Dials")?.offset as u64)
                .into_iter()
                .filter_map(|d| {
                    Some(Dial {
                        now: byte(m, n, d, "DialState")?,
                        want: byte(m, n, d, "DialSolution")?,
                        places: byte(m, n, d, "NbDialState")?,
                    })
                })
                .collect();
            if dials.is_empty() {
                return None;
            }
            let locked = byte(m, n, comp, "bIsDialsLocked").is_some_and(|b| b & 1 != 0);
            let solved = locked || dials.iter().all(|d| d.turns() == 0);
            (Answer::Dials(dials), solved)
        }
        Kind::Keypad => {
            let (at, _) = n.path(m, comp, &["Rune", "ExpectedCode"])?;
            let code = crate::quests::fstring(m, at)?;
            // The keypad's (or the computer's) action says whether it was opened.
            let action = component(m, n, actor, "KeypadAction").or_else(|| component(m, n, actor, "ComputerAccessAction"));
            (Answer::Code(code), action.is_some_and(|a| activated(m, n, a)))
        }
        Kind::Placement => {
            let solution = mem::read_u64(m, comp + n.field(m, comp, "Solution")?.offset as u64).filter(|&p| mem::plausible(p))?;
            let items = if n.is_a(m, solution, "ItemData") {
                vec![solution]
            } else {
                n.field(m, solution, "Solution").map(|f| objects(m, solution + f.offset as u64)).unwrap_or_default()
            };
            let names: Vec<String> = items.into_iter().filter_map(|i| n.object(m, i)).collect();
            if names.is_empty() {
                return None;
            }
            (Answer::Items(names), activated(m, n, comp))
        }
    };
    Some(Puzzle { id: comp, kind, class, at, answer, solved })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dial_turns_forward_round_its_places() {
        assert_eq!(Dial { now: 1, want: 3, places: 8 }.turns(), 2);
        assert_eq!(Dial { now: 6, want: 1, places: 8 }.turns(), 3);
        assert_eq!(Dial { now: 4, want: 4, places: 8 }.turns(), 0);
    }
}
