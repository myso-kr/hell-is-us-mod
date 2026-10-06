//! What stands about the hero for the camera to look at: the enemies, the people, what can be
//! picked up, and the things to work (levers, doors, save points). The worker puts them here from each step's scan;
//! a take reads them as it rolls.

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubjectKind {
    Enemy,
    Npc,
    /// Items and loot.
    Item,
    /// Things to work: levers, doors, save points.
    Thing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Subject {
    pub kind: SubjectKind,
    pub at: [f32; 3],
}

/// The subjects about the hero, as the worker last saw them.
pub type SubjectFeed = Arc<Mutex<Vec<Subject>>>;

/// How far from the hero a subject is kept (cm).
const WITHIN: f32 = 3000.0;

/// The subjects among the scan's things within `WITHIN` of `hero`.
pub fn about(things: &[crate::actors::Thing], hero: [f32; 3]) -> Vec<Subject> {
    use crate::actors::Kind;
    things
        .iter()
        .filter(|t| (t.at[0] - hero[0]).hypot(t.at[1] - hero[1]) < WITHIN)
        .map(|t| Subject {
            kind: match t.sub.kind() {
                Kind::Enemy => SubjectKind::Enemy,
                Kind::Npc => SubjectKind::Npc,
                Kind::Item | Kind::Loot => SubjectKind::Item,
                Kind::Interact | Kind::Save => SubjectKind::Thing,
            },
            at: t.at,
        })
        .collect()
}
