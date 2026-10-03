//! The region ledger (.spec/JOURNEY.md §3.1, §3.4): for every region, how many things are
//! left there, by kind — counts only, so it answers "did I miss something here?" without
//! naming anything. The same rows, sorted by what can be done now, are the trip planner.

use crate::puzzles::Kind;
use crate::survey::{Collect, Lock, Need, Placed};
use crate::tables::{Hollows, VaultNote, VaultState};
use std::collections::{BTreeMap, HashSet};

/// What is left in one region.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub world: String,
    /// Places the quests under way still need.
    pub quest: usize,
    /// NPCs who want an item the hero holds.
    pub handovers: usize,
    /// Dials and keypads not solved (vault doors and Lymbic locks counted apart).
    pub puzzles: usize,
    /// Lymbic locks not opened, and how many of them the rods held already open.
    pub locks: usize,
    pub openable: usize,
    pub collect: usize,
    /// Enemy groups left.
    pub groups: usize,
    /// NPCs with more to tell.
    pub stories: usize,
    /// Vault doors not opened.
    pub vaults: usize,
}

impl Region {
    /// Everything left, of every kind.
    pub fn total(&self) -> usize {
        self.quest
            + self.handovers
            + self.puzzles
            + self.locks
            + self.collect
            + self.groups
            + self.stories
            + self.vaults
    }

    /// What the hero can do there now without finding anything first: hand items over,
    /// open the locks whose rods are held.
    pub fn doable(&self) -> usize {
        self.handovers + self.openable
    }
}

/// The lists a ledger is counted from — the snapshot's, borrowed.
pub struct Sources<'a> {
    pub needs: &'a [(String, Vec<Need>)],
    pub handovers: &'a [Need],
    pub catalogue: &'a [(Placed, bool)],
    pub locks: &'a [Lock],
    pub collection: &'a [Collect],
    pub hollows: &'a [Hollows],
    pub stories: &'a [Need],
    pub vaults: &'a [VaultNote],
}

fn row<'a>(by: &'a mut BTreeMap<String, Region>, w: &str) -> &'a mut Region {
    by.entry(w.to_string()).or_insert_with(|| Region { world: w.to_string(), ..Default::default() })
}

/// Every region with anything left, by world name. A place two quests need counts once.
pub fn ledger(s: &Sources) -> Vec<Region> {
    let mut by: BTreeMap<String, Region> = BTreeMap::new();
    let mut seen = HashSet::new();
    for (_, list) in s.needs {
        for n in list.iter().filter(|n| !n.done && seen.insert(n.id)) {
            row(&mut by, &n.world).quest += 1;
        }
    }
    for n in s.handovers {
        row(&mut by, &n.world).handovers += 1;
    }
    let mut seen = HashSet::new();
    for (p, solved) in s.catalogue {
        let plain = matches!(p.kind, Kind::Dial | Kind::Keypad) && !p.class.starts_with("VOFK_");
        if !*solved && plain && seen.insert(p.id()) {
            row(&mut by, &p.world).puzzles += 1;
        }
    }
    for l in s.locks.iter().filter(|l| !l.solved) {
        let r = row(&mut by, &l.world);
        r.locks += 1;
        r.openable += l.openable() as usize;
    }
    for c in s.collection {
        for (w, n) in &c.left_by_world {
            row(&mut by, w).collect += n;
        }
    }
    for h in s.hollows.iter().filter(|h| h.left > 0) {
        row(&mut by, &h.world).groups += h.left;
    }
    for n in s.stories {
        row(&mut by, &n.world).stories += 1;
    }
    for v in s.vaults.iter().filter(|v| v.state != VaultState::Opened) {
        if let Some((w, _)) = &v.door {
            row(&mut by, w).vaults += 1;
        }
    }
    by.into_values().filter(|r| r.total() > 0).collect()
}

/// The trip planner's order: most to do now first, then most left, then by name.
pub fn by_trip(mut rows: Vec<Region>) -> Vec<Region> {
    rows.sort_by(|a, b| b.doable().cmp(&a.doable()).then(b.total().cmp(&a.total())).then(a.world.cmp(&b.world)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzles::Answer;
    use crate::survey::Rod;

    fn need(world: &str, id: u64) -> Need {
        Need { world: world.into(), id, label: String::new(), what: String::new(), at: [0.0; 3], done: false }
    }

    fn placed(world: &str, class: &str, kind: Kind, x: f32) -> Placed {
        Placed {
            world: world.into(),
            class: class.into(),
            at: [x, 0.0, 0.0],
            guid: None,
            kind,
            answer: Answer::Code("1".into()),
        }
    }

    #[test]
    fn counts_each_kind_per_region_once() {
        let needs = vec![
            ("Q1".to_string(), vec![need("Acasa", 1), need("Acasa", 2)]),
            ("Q2".to_string(), vec![need("Acasa", 1)]),
        ];
        let handovers = vec![need("Talju", 9)];
        let catalogue = vec![
            (placed("Acasa", "Keypad_BP_C", Kind::Keypad, 0.0), false),
            (placed("Acasa", "Keypad_BP_C", Kind::Keypad, 0.0), false),
            (placed("Acasa", "Dial_BP_C", Kind::Dial, 5.0), true),
            (placed("Acasa", "VOFK_Acasa_DialPuzzle_BP_C", Kind::Dial, 9.0), false),
            (placed("Acasa", "Door_BP_C", Kind::Placement, 7.0), false),
        ];
        let lock = |world: &str, held: bool| Lock {
            id: 1,
            world: world.into(),
            at: [0.0; 3],
            solved: false,
            rods: vec![Rod { item: "LymbicRod_X".into(), held, source: None }],
        };
        let locks = vec![lock("Talju", true), lock("Talju", false)];
        let collection = vec![Collect {
            label: "RELIC",
            here: (0, 0),
            all: (0, 3),
            left_here: Vec::new(),
            left_by_world: vec![("Acasa".into(), 2), ("Jeljin".into(), 1)],
        }];
        let rows = ledger(&Sources {
            needs: &needs,
            handovers: &handovers,
            catalogue: &catalogue,
            locks: &locks,
            collection: &collection,
            hollows: &[],
            stories: &[],
            vaults: &[],
        });
        let acasa = rows.iter().find(|r| r.world == "Acasa").unwrap();
        assert_eq!(acasa.quest, 2, "a place two quests need counts once");
        assert_eq!(acasa.puzzles, 1, "solved, duplicated, vault doors and key doors are not counted");
        assert_eq!(acasa.collect, 2);
        let talju = rows.iter().find(|r| r.world == "Talju").unwrap();
        assert_eq!((talju.locks, talju.openable, talju.handovers), (2, 1, 1));
        assert_eq!(talju.doable(), 2);
        assert_eq!(by_trip(rows).first().map(|r| r.world.as_str()), Some("Talju"), "most to do now first");
    }

    #[test]
    fn regions_with_nothing_left_are_not_listed() {
        let rows = ledger(&Sources {
            needs: &[("Q".to_string(), vec![Need { done: true, ..need("Acasa", 1) }])],
            handovers: &[],
            catalogue: &[],
            locks: &[],
            collection: &[],
            hollows: &[],
            stories: &[],
            vaults: &[],
        });
        assert!(rows.is_empty());
    }
}
