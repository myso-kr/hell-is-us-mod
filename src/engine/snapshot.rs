//! What one step saw: plain data a UI thread keeps a copy of.

use super::*;

/// What one step saw. Plain data, so a UI thread can hold a copy.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// (pid, Steam build id) when attached.
    pub game: Result<(u32, String), String>,
    pub gate: Result<(), String>,
    /// The current value of every attribute the cheat table names, `None` where
    /// unreadable this step.
    pub values: Vec<(Attr, Option<f32>)>,
    pub active: Vec<Active>,
    /// Originals on record, waiting to be put back.
    pub pending: usize,
    /// The originals themselves — what the debug tab compares against.
    pub originals: Vec<(Attr, (f32, f32))>,
    /// Why the toggles stopped, or what the last tick could not do.
    pub notice: Option<String>,
    /// Where the hero is (cm) and the camera's yaw (degrees) — the minimap's input.
    pub pose: Option<([f64; 3], f64)>,
    /// Where the overlay reads the pose itself, every frame (`player::PoseSource`).
    pub pose_src: Option<crate::player::PoseSource>,
    /// The world the hero is in, by name.
    pub world: Option<String>,
    /// What the minimap marks besides the hero.
    pub things: Arc<Vec<Thing>>,
    /// A Haze and a Hollow Walker it keeps alive, each pair (JOURNEY §3.7).
    pub haze_links: Arc<Vec<crate::actors::HazeLink>>,
    /// The minimap's background.
    pub footprints: Arc<Vec<Footprint>>,
    /// Places with something new to learn.
    pub goals: Arc<Vec<Goal>>,
    /// The quest journal: main quests and good deeds, with their state.
    pub journal: Arc<Vec<crate::quests::Quest>>,
    /// The game is paused (a menu that stops it is open).
    pub paused: bool,
    /// What stands in the way, and the ground, for the route.
    pub obstacles: Arc<Scene>,
    /// The game's navmesh: the route's first choice.
    pub nav: Arc<crate::navmesh::NavMesh>,
    /// For each quest under way, what it needs in every world (the survey).
    pub needs: Arc<Vec<(String, Vec<crate::survey::Need>)>>,
    /// NPCs that want an item the hero holds.
    pub handovers: Arc<Vec<crate::survey::Need>>,
    /// Missable good deeds not done yet, and their deadlines.
    pub deadlines: Arc<Vec<crate::missables::Deadline>>,
    /// Collectibles placed and taken, per sort (the survey); NPCs with more to tell.
    pub collection: Arc<Vec<crate::survey::Collect>>,
    pub stories: Arc<Vec<crate::survey::Need>>,
    /// How many good deeds, mysteries and timeloops the game has.
    pub secret_totals: [usize; 3],
    /// The vault notebook (F7) and the research entries known.
    pub vaults: Arc<Vec<crate::tables::VaultNote>>,
    pub lore_known: usize,
    /// Every world's Hollows left (F8).
    pub hollows: Arc<Vec<crate::tables::Hollows>>,
    /// The puzzles near the hero, nearest first, with their answers (F6).
    pub puzzles: Arc<Vec<crate::puzzles::Puzzle>>,
    /// Every puzzle in the worlds (the survey) and whether it was solved.
    pub catalogue: Arc<Vec<(crate::survey::Placed, bool)>>,
    /// Every Lymbic lock: the rods it takes, held or where to find them (JOURNEY.md §3.3).
    pub locks: Arc<Vec<crate::survey::Lock>>,
    /// The clue board: the Datapad by entry, and the items held (clues.rs).
    pub clues: Arc<crate::clues::Clues>,
    /// The shard budget of the upgrade achievements (budget.rs); empty without recipes.
    pub budget: Arc<crate::budget::Budget>,
    /// Saved positions: (world, where).
    pub slots: [Option<(String, [f64; 3])>; SLOTS],
}

/// How many positions can be saved.
pub const SLOTS: usize = 5;
impl Snapshot {
    pub fn value(&self, a: Attr) -> Option<f32> {
        self.values.iter().find(|(x, _)| *x == a).and_then(|(_, v)| *v)
    }
}
