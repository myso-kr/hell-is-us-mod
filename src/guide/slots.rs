//! Choice puzzles: items set into slots side by side, one of each set right and the rest
//! decoys. The Watcher's Nest takes four ceramic flowers in 19 grooves (one right in each of
//! four sets, and a pair by the armour rack that is never right); the Eye of God takes four
//! orbs at eight statues (four keepers' statues right, four not).
//!
//! The survey marks each slot with what it counts as right (`survey::Choice`, read from the
//! slot class's `Item`: the placement's own `Solution` is only what it accepts). This groups
//! the slots into puzzles and sets by where they stand, and tells each set's state from
//! what its slots hold, read live when they are loaded and remembered after.

use crate::puzzles::Puzzle;
use crate::survey::{Choice, Placed};
use std::collections::HashMap;

/// Slots this close (cm, any way) belong to one puzzle: the Eye of God's statues stand
/// 30 to 140 m apart in one hall.
const PUZZLE_REACH: f32 = 25_000.0;
/// And one set: slots on one wall, side by side (the grooves are 2 to 6 m apart), at one
/// height (a floor up is another set).
const SET_REACH: f32 = 1_000.0;
const SET_HEIGHT: f32 = 150.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// The actor's place, on the floor below the groove.
    pub at: [f32; 3],
    /// The groove: where what is put in shows, as read live, or until then `GROOVE` above `at`.
    pub groove: [f32; 3],
    pub choice: Choice,
    /// What it holds: `None` not seen loaded yet this run, `Some(None)` empty.
    pub holds: Option<Option<String>>,
}

/// How far above its actor's place a choice slot's groove is (cm): the base blueprint's
/// (Base_1SlotPlacementPuzzleCheck), measured at the Watcher's Nest's grooves, 155 up and
/// under 30 to the side.
const GROOVE: f32 = 155.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    /// A stable id: what the panel keeps the hints asked for by.
    pub id: u64,
    pub slots: Vec<Slot>,
}

/// Where a set stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// The right item in the right slot, and nothing in the decoys.
    Done,
    /// Something in a decoy, or the wrong item in the right slot: to take out again.
    Wrong,
    /// Nothing placed yet (or not all of it).
    Open,
    /// Not seen loaded this run: what it holds is not known.
    Unseen,
    /// No slot of it is right: nothing goes here.
    Nothing,
}

impl Set {
    /// The right slot, if the set has one.
    pub fn answer(&self) -> Option<&Slot> {
        self.slots.iter().find(|s| matches!(s.choice, Choice::Right(_)))
    }

    pub fn state(&self) -> State {
        let wrong = self.slots.iter().any(|s| match (&s.choice, &s.holds) {
            (Choice::Decoy, Some(Some(_))) => true,
            (Choice::Right(want), Some(Some(have))) => want != have,
            _ => false,
        });
        if wrong {
            return State::Wrong;
        }
        let Some(right) = self.answer() else { return State::Nothing };
        match &right.holds {
            None => State::Unseen,
            Some(Some(_)) => State::Done,
            Some(None) => State::Open,
        }
    }

    /// Where the right slot is among the set's, counted from the left as one faces `yaw`
    /// (degrees, Unreal's: 0 east along X, 90 along Y): (n from 1, of how many). Only the
    /// sideways order counts; at a wall, facing it, that is the groove.
    pub fn from_left(&self, yaw: f32) -> Option<(usize, usize)> {
        let right = self.answer()?;
        let (s, c) = yaw.to_radians().sin_cos();
        // Unreal's right of a yaw: (-sin, cos).
        let side = |p: [f32; 3]| -s * p[0] + c * p[1];
        let n = self.slots.iter().filter(|x| side(x.at) < side(right.at)).count();
        Some((n + 1, self.slots.len()))
    }

    /// Where it is: the middle of its slots.
    pub fn at(&self) -> [f32; 3] {
        let n = self.slots.len().max(1) as f32;
        let sum = self.slots.iter().fold([0.0; 3], |a, s| [a[0] + s.at[0], a[1] + s.at[1], a[2] + s.at[2]]);
        sum.map(|v| v / n)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotPuzzle {
    pub world: String,
    /// The items its right slots take, each once, in the order first met.
    pub items: Vec<String>,
    pub sets: Vec<Set>,
}

impl SlotPuzzle {
    /// (sets done, sets with an answer).
    pub fn progress(&self) -> (usize, usize) {
        let sets = self.sets.iter().filter(|s| s.answer().is_some());
        let (mut done, mut all) = (0, 0);
        for s in sets {
            all += 1;
            done += (s.state() == State::Done) as usize;
        }
        (done, all)
    }

    /// The game's clue to it, when the table below knows the puzzle.
    pub fn clue(&self) -> Option<&'static Clue> {
        CLUES.iter().find(|c| self.items.iter().any(|i| c.items.contains(&i.as_str())))
    }
}

/// What each slot was last seen to hold, by where it stands (kept through the run: a set
/// left behind keeps its state).
#[derive(Default)]
pub struct Seen(HashMap<[i32; 3], Sight>);

/// A slot as last seen: what it held, and where its groove is.
type Sight = (Option<String>, Option<[f32; 3]>);

fn key(at: [f32; 3]) -> [i32; 3] {
    at.map(|v| (v / 50.0).round() as i32)
}

impl Seen {
    /// Takes in what the loaded slots hold now: each live placement is the slot it stands
    /// on (the same actor, within 1 m; the grooves are 2 m apart and more).
    pub fn see(&mut self, slots: &[(Placed, bool)], live: &[Puzzle]) {
        let slots: Vec<&Placed> = slots.iter().map(|(p, _)| p).filter(|p| p.choice.is_some()).collect();
        for l in live.iter().filter(|l| l.kind == crate::puzzles::Kind::Placement) {
            let off = |p: &&Placed| (l.at[0] - p.at[0]).hypot(l.at[1] - p.at[1]);
            if let Some(p) = slots.iter().filter(|p| off(p) < 100.0).min_by(|a, b| off(a).total_cmp(&off(b))) {
                self.0.insert(key(p.at), (l.placed.iter().flatten().next().cloned(), l.slot_at.first().copied()));
            }
        }
    }
}

/// The choice puzzles in the catalogue, grouped, with what their slots were seen to hold.
pub fn group(catalogue: &[(Placed, bool)], seen: &Seen) -> Vec<SlotPuzzle> {
    let slots: Vec<&Placed> = catalogue.iter().map(|(p, _)| p).filter(|p| p.choice.is_some()).collect();
    let flat = |a: &Placed, b: &Placed| (a.at[0] - b.at[0]).hypot(a.at[1] - b.at[1]);
    let puzzles =
        clusters(slots.len(), |i, j| slots[i].world == slots[j].world && flat(slots[i], slots[j]) < PUZZLE_REACH);
    let mut out: Vec<SlotPuzzle> = puzzles
        .into_iter()
        .map(|members| {
            let near = |i: usize, j: usize| {
                let (a, b) = (slots[members[i]], slots[members[j]]);
                flat(a, b) < SET_REACH && (a.at[2] - b.at[2]).abs() < SET_HEIGHT
            };
            let mut items = Vec::new();
            let mut sets: Vec<Set> = clusters(members.len(), near)
                .into_iter()
                .map(|set| {
                    let slots: Vec<Slot> = set
                        .into_iter()
                        .map(|i| {
                            let p = slots[members[i]];
                            let choice = p.choice.clone().unwrap();
                            if let Choice::Right(item) = &choice {
                                if !items.contains(item) {
                                    items.push(item.clone());
                                }
                            }
                            let seen = seen.0.get(&key(p.at));
                            Slot {
                                at: p.at,
                                groove: seen.and_then(|s| s.1).unwrap_or([p.at[0], p.at[1], p.at[2] + GROOVE]),
                                choice,
                                holds: seen.map(|s| s.0.clone()),
                            }
                        })
                        .collect();
                    let mut set = Set { id: 0, slots };
                    set.id = id(&slots_of(&set));
                    set
                })
                .collect();
            // Biggest first, then west to east: the same order every time.
            sets.sort_by(|a, b| b.slots.len().cmp(&a.slots.len()).then(a.at()[0].total_cmp(&b.at()[0])));
            SlotPuzzle { world: slots[members[0]].world.clone(), items, sets }
        })
        .collect();
    out.sort_by(|a, b| a.world.cmp(&b.world));
    out
}

fn slots_of(s: &Set) -> Vec<[i32; 3]> {
    s.slots.iter().map(|x| key(x.at)).collect()
}

fn id(keys: &[[i32; 3]]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    keys.hash(&mut h);
    h.finish()
}

/// Single-linkage clusters of `0..n` by `near`, each in index order.
fn clusters(n: usize, near: impl Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    let mut of: Vec<usize> = (0..n).collect();
    fn root(of: &mut [usize], mut i: usize) -> usize {
        while of[i] != i {
            of[i] = of[of[i]];
            i = of[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            if near(i, j) {
                let (a, b) = (root(&mut of, i), root(&mut of, j));
                of[a.max(b)] = a.min(b);
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut at: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        let r = root(&mut of, i);
        let g = *at.entry(r).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[g].push(i);
    }
    groups
}

/// The game's own clue to a choice puzzle, as text references (`Namespace/Key`, for
/// `i18n::game_text`): the note or scroll that poses it, and for each set the text that
/// settles it. Sets are told apart by how many slots they have and whether one is right
/// (the Watcher's Nest's: 8 under the wooden shields, 3 under the metal ones, 2 by the old
/// banner, 4 in the alcoves, and 2 by the armour rack that hold no answer). A set the table
/// does not name is settled by the right item's own description (the Eye of God's orbs:
/// each tells of the feeling its keeper keeps).
pub struct Clue {
    pub items: &'static [&'static str],
    pub title: &'static str,
    pub text: &'static str,
    pub sets: &'static [(usize, bool, &'static str, &'static str)],
    /// The mod's words for what to do once all is in place, when the game does not say.
    pub after: Option<&'static str>,
}

pub const CLUES: &[Clue] = &[
    Clue {
        items: &["Quest02_PholguardFlowerSymbol_Item_DA"],
        title: "Quest02_Items/VitalisHideoutNoteQuest02_Name",
        text: "Quest02_Items/VitalisHideoutNoteQuest02_TextUpdate",
        // Players pull the lever before the last flower, or miss the rumble that says
        // all four are in, and take the lever for broken.
        after: Some("SLOT_AFTER_FLOWERS"),
        sets: &[
            (
                8,
                true,
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File01_Title",
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File01_Content",
            ),
            (
                3,
                true,
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File02_Title",
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File02_Content",
            ),
            (
                2,
                true,
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File04_Title",
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File04_Content",
            ),
            (
                4,
                true,
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File05_Title",
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File05_Content",
            ),
            (
                2,
                false,
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File04_Title",
                "UI_Computer_ST/WatchersNest_VitalisResearchFlowerGrouping_File04_Content",
            ),
        ],
    },
    Clue {
        items: &[
            "Quest06_OrbOfEcstasy_Item_DA",
            "Quest06_OrbOfGrief_Item_DA",
            "Quest06_OrbOfRage_Item_DA",
            "Quest06_OrbOfTerror_Item_DA",
        ],
        title: "Quest01_Items/FailsafeAntichamberScrollQuest06_Name",
        text: "Quest01_Items/FailsafeAntichamberScrollQuest06_Text",
        after: None,
        sets: &[],
    },
];

impl Clue {
    /// The research file that settles a set: by its size and whether it has an answer.
    pub fn for_set(&self, set: &Set) -> Option<(&'static str, &'static str)> {
        let right = set.answer().is_some();
        self.sets.iter().find(|(n, r, _, _)| *n == set.slots.len() && *r == right).map(|(_, _, t, c)| (*t, *c))
    }
}

/// A stable id for a puzzle's riddle, for the panel to keep it shown by.
pub fn riddle_id(p: &SlotPuzzle) -> u64 {
    let keys: Vec<[i32; 3]> = p.sets.iter().flat_map(|s| s.slots.iter().map(|x| key(x.at))).collect();
    id(&keys) ^ 0x5EED
}

/// A game text to show as plain text: its inline images (`<img id="Brave" …/>`) dropped.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        match rest[i..].find('>') {
            Some(j) => rest = &rest[i + j + 1..],
            None => {
                rest = &rest[i..];
                break;
            }
        }
    }
    out.push_str(rest);
    out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").replace("\n\n\n", "\n\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzles::{Answer, Kind};

    const FLOWER: &str = "Quest02_PholguardFlowerSymbol_Item_DA";

    fn slot(x: f32, y: f32, z: f32, right: bool) -> (Placed, bool) {
        let choice = if right { Choice::Right(FLOWER.into()) } else { Choice::Decoy };
        let p = Placed {
            world: "VyssaHills".into(),
            class: "Groove_BP_C".into(),
            at: [x, y, z],
            guid: None,
            kind: Kind::Placement,
            answer: Answer::Items(vec!["Cup_Item_DA".into()]),
            choice: Some(choice),
        };
        (p, false)
    }

    /// The Watcher's Nest as the survey has it: five sets.
    fn nest() -> Vec<(Placed, bool)> {
        let mut v = Vec::new();
        for (i, x) in [-10732.0, -11124.0, -9994.0, -9977.0, -10952.0, -11224.0, -10272.0].iter().enumerate() {
            v.push(slot(*x, 24200.0 + i as f32 * 60.0, 2624.0, false));
        }
        v.push(slot(-10097.0, 24433.0, 2624.0, true));
        v.extend([slot(-12067.0, 23178.0, 2630.0, false), slot(-12261.0, 22983.0, 2630.0, false)]);
        v.push(slot(-12456.0, 22789.0, 2630.0, true));
        v.extend([slot(-12312.0, 22349.0, 2811.0, false), slot(-12067.0, 22105.0, 2811.0, false)]);
        v.extend([slot(-9780.0, 22072.0, 2930.0, false), slot(-10295.0, 22586.0, 2930.0, true)]);
        v.extend([slot(-10098.0, 20782.0, 2866.0, false), slot(-10416.0, 20464.0, 2866.0, false)]);
        v.extend([slot(-10386.0, 20519.0, 2866.0, false), slot(-10705.0, 20201.0, 2866.0, true)]);
        v
    }

    #[test]
    fn the_grooves_fall_into_their_five_sets() {
        let p = group(&nest(), &Seen::default());
        assert_eq!(p.len(), 1);
        let sizes: Vec<(usize, bool)> = p[0].sets.iter().map(|s| (s.slots.len(), s.answer().is_some())).collect();
        assert_eq!(sizes, [(8, true), (4, true), (3, true), (2, false), (2, true)]);
        assert_eq!(p[0].items, [FLOWER]);
        assert_eq!(p[0].progress(), (0, 4));
        assert!(p[0].sets.iter().all(|s| matches!(s.state(), State::Unseen | State::Nothing)));
    }

    #[test]
    fn a_set_is_done_open_or_wrong_by_what_its_slots_hold() {
        let cat = nest();
        let live = |x: f32, y: f32, holds: Option<&str>| Puzzle {
            id: 1,
            kind: Kind::Placement,
            class: String::new(),
            at: [x, y, 0.0],
            answer: Answer::Items(vec![]),
            solved: false,
            placed: vec![holds.map(str::to_string)],
            slot_at: vec![],
        };
        let mut seen = Seen::default();
        seen.see(
            &cat,
            &[
                live(-10097.0, 24433.0, Some(FLOWER)),
                live(-12456.0, 22789.0, None),
                live(-10098.0, 20782.0, Some(FLOWER)),
            ],
        );
        let p = &group(&cat, &seen)[0];
        let state = |n: usize, right: bool| {
            p.sets.iter().find(|s| s.slots.len() == n && s.answer().is_some() == right).unwrap().state()
        };
        assert_eq!(state(8, true), State::Done);
        assert_eq!(state(3, true), State::Open);
        assert_eq!(state(4, true), State::Wrong, "a flower in an alcove's decoy");
        assert_eq!(state(2, false), State::Nothing);
        assert_eq!(p.progress(), (1, 4));
    }

    #[test]
    fn the_flowers_clue_names_each_set_by_its_research_file() {
        let p = &group(&nest(), &Seen::default())[0];
        let clue = p.clue().unwrap();
        let file = |n: usize, right: bool| {
            let s = p.sets.iter().find(|s| s.slots.len() == n && s.answer().is_some() == right).unwrap();
            clue.for_set(s).unwrap().0
        };
        assert!(file(8, true).ends_with("File01_Title"));
        assert!(file(4, true).ends_with("File05_Title"));
        assert!(file(2, false).ends_with("File04_Title"));
    }

    #[test]
    fn the_right_groove_is_counted_from_the_left_as_one_faces() {
        let p = &group(&nest(), &Seen::default())[0];
        let alcove = p.sets.iter().find(|s| s.slots.len() == 4).unwrap();
        // The alcoves' grooves run along a line; facing one way the right one is n from
        // the left, facing back the other way it is n from the right.
        let (n, all) = alcove.from_left(0.0).unwrap();
        let (m, _) = alcove.from_left(180.0).unwrap();
        assert_eq!(all, 4);
        assert_eq!(n + m, all + 1);
    }

    #[test]
    fn plain_text_drops_inline_images() {
        let t = "Founders watch.\\n\n<img id=\"Reckless\" height=\"32\"/>\nHoz the reckless";
        assert_eq!(plain(t), "Founders watch.\\n\n\nHoz the reckless".replace("\n\n\n", "\n\n"));
    }
}
