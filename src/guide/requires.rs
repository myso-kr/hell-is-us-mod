//! What must come first (.spec/GUIDE.md §9), step 1: what an unsolved placement takes.
//!
//! A placement — a key door (`…_KeyLocked_…`), a slot for a gear or a statue — takes
//! items the survey names (`puzzle.items`), and the survey also names where each item is
//! given (`payload.items`). So:
//!
//! - every item it takes held: the place to put them is a goal, with the quest of what
//!   gave the items, so the story's guide goes there next;
//! - an item it takes not held: the goals that give it are marked as needed for it, so
//!   the chain reads "the large gear → the gear slot".
//!
//! Choice puzzles (slots.rs) and solved placements (their save GUID has a state) are left
//! out.

use crate::goals::{Gate, Goal, Tier};
use crate::survey::{Entry, Placed, Survey};
use std::collections::HashSet;

/// A placement unsolved in `world`, and the items it takes.
struct Want<'a> {
    place: &'a Placed,
    items: &'a [String],
}

fn wants<'a>(survey: &'a Survey, world: &str, saved: &HashSet<String>) -> Vec<Want<'a>> {
    survey
        .puzzles
        .iter()
        .filter(|p| p.world == world && p.kind == crate::puzzles::Kind::Placement && p.choice.is_none())
        .filter(|p| !p.guid.as_ref().is_some_and(|g| saved.contains(g)))
        .filter_map(|p| match &p.answer {
            crate::puzzles::Answer::Items(items) if !items.is_empty() => Some(Want { place: p, items }),
            _ => None,
        })
        .collect()
}

/// What gives `item`, in any world: the survey's entries that hand it out.
fn givers<'a>(survey: &'a Survey, item: &'a str) -> impl Iterator<Item = &'a Entry> {
    survey.worlds.values().flatten().filter(move |e| e.items.iter().any(|i| i == item))
}

/// A placement's name: what goes there.
fn place_label(items: &[String]) -> String {
    let names: Vec<String> = items.iter().map(|i| crate::goals::item_label(i)).collect();
    trf!("REQ_PUT_HERE", items = names.join(", "))
}

/// The goals the placements make: the place to put what is held, where every item is.
pub fn placement_goals(survey: &Survey, world: &str, saved: &HashSet<String>, held: &HashSet<String>) -> Vec<Goal> {
    wants(survey, world, saved)
        .into_iter()
        .filter(|w| w.items.iter().all(|i| held.contains(i)))
        .map(|w| {
            // The quest of what gave the items: the placement moves the same quest along.
            let mut keys: Vec<String> = Vec::new();
            let mut tags: Vec<String> = Vec::new();
            for e in w.items.iter().flat_map(|i| givers(survey, i)) {
                keys.extend(e.keys.iter().filter(|k| !keys.contains(k)).cloned().collect::<Vec<_>>());
                tags.extend(e.tags.iter().filter(|t| !tags.contains(t)).cloned().collect::<Vec<_>>());
            }
            let quest = w.items.iter().any(|i| i.starts_with("Quest")) || !keys.is_empty();
            Goal {
                tier: if quest { Tier::Quest } else { Tier::Secret },
                id: place_id(w.place),
                label: place_label(w.items),
                detail: tr!("REQ_PUT_HELD").to_string(),
                at: w.place.at,
                quests: Vec::new(),
                tags,
                keys,
                gate: Gate::Open,
                named: true,
                reveals: Default::default(),
                first: None,
            }
        })
        .collect()
}

/// Mark the goals that give an item an unsolved placement in `world` still needs: their
/// detail says what for. Returns how many were marked.
pub fn mark_givers(
    goals: &mut [Goal],
    survey: &Survey,
    world: &str,
    saved: &HashSet<String>,
    held: &HashSet<String>,
) -> usize {
    let mut marked = 0;
    for w in wants(survey, world, saved) {
        for item in w.items.iter().filter(|i| !held.contains(*i)) {
            let name = crate::goals::item_label(item);
            for g in goals.iter_mut().filter(|g| g.label == name || g.detail.contains(&name)) {
                let note = trf!("REQ_NEEDED_FOR", place = place_label(w.items));
                if !g.detail.contains(&note) {
                    g.detail = if g.detail.is_empty() { note } else { format!("{} · {note}", g.detail) };
                    marked += 1;
                }
            }
        }
    }
    marked
}

/// A placement goal's id: its place, apart from the survey's and the live ids.
fn place_id(p: &Placed) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ("placement", &p.world, &p.class, p.at.map(|v| v as i32)).hash(&mut h);
    h.finish() | 1 << 63
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gear_world() -> Survey {
        let slot = Placed {
            world: "W".into(),
            class: "LargeGearPlacement_ActivatorInteract_BP_C".into(),
            at: [100.0, 0.0, 0.0],
            guid: Some("G1".into()),
            kind: crate::puzzles::Kind::Placement,
            answer: crate::puzzles::Answer::Items(vec!["Quest02_GearLarge_Item_DA".into()]),
            choice: None,
        };
        let gear = Entry {
            name: "Gear".into(),
            at: [0.0, 0.0, 0.0],
            items: vec!["Quest02_GearLarge_Item_DA".into()],
            keys: vec!["Q2".into()],
            ..Default::default()
        };
        Survey { worlds: [("W".to_string(), vec![gear])].into(), puzzles: vec![slot], ..Survey::default() }
    }

    #[test]
    fn the_slot_is_a_goal_once_its_item_is_held() {
        let s = gear_world();
        let none = HashSet::new();
        assert!(placement_goals(&s, "W", &none, &none).is_empty(), "the gear not held: no slot goal");
        let held = HashSet::from(["Quest02_GearLarge_Item_DA".to_string()]);
        let g = placement_goals(&s, "W", &none, &held);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].keys, ["Q2"], "the gear's quest");
        assert_eq!(g[0].tier, Tier::Quest);
        let solved = HashSet::from(["G1".to_string()]);
        assert!(placement_goals(&s, "W", &solved, &held).is_empty(), "solved: gone");
    }
}
