//! What every world hands out, from the survey of the game's maps (tools/survey,
//! `Mods\survey\*.json`; .spec/SURVEY.md): pickups, devices, markers, NPCs and what
//! their conversations give, hand-overs — with where they are.
//!
//! The live goals (goals.rs) only know what is loaded around the hero. The survey
//! fills in the rest: places out of range in this world, and what is waiting in other
//! worlds. Whether something is still left is judged as for live goals — facts and
//! tags not known yet — and for an item, whether the hero holds it.

use crate::goals::{Gate, Goal, Tier};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Entry {
    /// The actor's name, as it is when loaded (World Partition names are stable).
    pub name: String,
    pub class: String,
    pub at: [f32; 3],
    /// Item asset names it hands out (`Quest01_PictureC_Item_DA`), and their folders'
    /// main quest when they are quest items (`Quest01`).
    pub items: Vec<String>,
    pub keys: Vec<String>,
    /// Lymbic rods it hands out (`LymbicRod_Zulu_Terror_Item_DA`), kept apart from `items`:
    /// they open locks, they are not something a quest waits for.
    pub rods: Vec<String>,
    pub facts: Vec<String>,
    pub tags: Vec<String>,
    /// The item it wants, for a hand-over (the first, for the goal's line).
    pub wants: Option<String>,
    /// Every hand-over it takes: the item, and the facts and tags it gives back.
    pub trades: Vec<(String, Vec<String>, Vec<String>)>,
    /// The collectible sorts of what it hands out (`COLLECT`), for the progress count.
    pub cats: Vec<&'static str>,
    /// Its save GUID, as the survey gives it — what tells taken from not.
    pub guid: Option<String>,
    pub npc: bool,
    /// An NPC's conversation flow: one person, whichever of its placements the story has
    /// put them at (Victor Gaz in the tunnel where he is met, then at the forge in Jova).
    pub flow: Option<String>,
}

/// What is left at a place, against what the hero knows and holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Left {
    pub facts: usize,
    pub tags: Vec<String>,
    pub items: Vec<String>,
}

impl Left {
    pub fn is_empty(&self) -> bool {
        self.facts == 0 && self.tags.is_empty() && self.items.is_empty()
    }
}

/// What the hero knows and holds, by name.
pub struct Known<'a> {
    pub facts: &'a HashSet<String>,
    pub tags: &'a HashSet<String>,
    pub held: &'a HashSet<String>,
    /// The GUIDs of placed things the save keeps a state for: taken, opened, used.
    pub saved: &'a HashSet<String>,
    /// NPCs done with for now, by name (goals.rs).
    pub talked: &'a HashSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Survey {
    /// By world (`AcasaMarshes`).
    pub worlds: HashMap<String, Vec<Entry>>,
    /// The Vaults of Forbidden Knowledge's dial doors: (world, where).
    pub doors: Vec<(String, [f32; 3])>,
    /// Every puzzle placed in the worlds, with its answer (F6's list).
    pub puzzles: Vec<Placed>,
    /// The ways out of each world: the APC's door and the save points (those that take
    /// the hero to the APC, and those that do not) — (world, which, where).
    pub exits: Vec<(String, crate::actors::Sub, [f32; 3])>,
}

/// A puzzle the survey found: where, and what solves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub world: String,
    pub class: String,
    pub at: [f32; 3],
    /// Its save GUID: a state in the save means it was solved (or used).
    pub guid: Option<String>,
    pub kind: crate::puzzles::Kind,
    /// The dials as the game sets them (`now` 0), the code, or the items.
    pub answer: crate::puzzles::Answer,
    /// A slot of a choice puzzle (slots.rs): what it counts as right. `answer` is then
    /// only what it accepts.
    pub choice: Option<Choice>,
}

/// A choice puzzle's slot (the survey's `expects`): the item it counts as right, or none,
/// a decoy (the Watcher's Nest's grooves but one in each set, the Eye of God's four
/// statues of keepers the orbs do not belong to).
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Right(String),
    Decoy,
}

impl Placed {
    /// A stable id for guiding to it.
    pub fn id(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&self.world, &self.class, self.at.map(|v| v as i32)).hash(&mut h);
        h.finish() | 1 << 63
    }
}

/// A survey puzzle record: `{kind, dials: [{places, solution}], code, items}`.
fn placed_answer(p: &Value) -> Option<(crate::puzzles::Kind, crate::puzzles::Answer)> {
    use crate::puzzles::{Answer, Dial, Kind};
    Some(match p["kind"].as_str()? {
        "dial" => (
            Kind::Dial,
            Answer::Dials(
                p["dials"]
                    .as_array()?
                    .iter()
                    .map(|d| Dial {
                        now: 0,
                        want: d["solution"].as_u64().unwrap_or(0) as u8,
                        places: d["places"].as_u64().unwrap_or(0) as u8,
                    })
                    .collect(),
            ),
        ),
        "keypad" => (Kind::Keypad, Answer::Code(p["code"].as_str()?.to_string())),
        "placement" => (
            Kind::Placement,
            Answer::Items(
                names(&p["items"]).into_iter().map(|i| i.rsplit('/').next().unwrap_or(&i).to_string()).collect(),
            ),
        ),
        _ => return None,
    })
}

fn names(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|x| x.as_str()).map(str::to_string).collect()
}

/// `/Game/Items/Quests/Quest01/Quest01_PictureC_Item_DA` → (`Quest01_PictureC_Item_DA`, `Quest01`).
fn item(path: &str) -> (String, Option<String>) {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    let key = path.split("/Items/Quests/").nth(1).and_then(|r| r.split('/').next()).map(str::to_string);
    (name, key)
}

/// The name an actor has once loaded: cooking adds a number after its UAID that the
/// running game does not have (`…_UAID_047C16054F89B91D02_1325171520` → `…_UAID_047C16054F89B91D02`).
/// Whether the game has `e` loaded: an actor of its name stands there. A name the survey
/// has once in the world is enough; a name it has more than once (one editor session's
/// World Partition actors) is told apart by where it stands — within `SAME_PLACE`, or
/// `SAME_PERSON` for someone who walks about.
pub fn is_loaded(e: &Entry, list: &[Entry], loaded: &HashMap<String, Vec<[f32; 3]>>) -> bool {
    let shared = list.iter().filter(|x| x.name == e.name).count() >= 2;
    loaded_as(e, shared, loaded)
}

/// The names `list` has more than once: `is_loaded` for a whole list without counting each
/// name again for every entry.
pub fn shared_names(list: &[Entry]) -> std::collections::HashSet<&str> {
    let mut seen = std::collections::HashSet::new();
    let mut twice = std::collections::HashSet::new();
    for e in list {
        if !seen.insert(e.name.as_str()) {
            twice.insert(e.name.as_str());
        }
    }
    twice
}

/// `is_loaded`, told whether `e`'s name is one the survey has more than once.
pub fn loaded_as(e: &Entry, shared: bool, loaded: &HashMap<String, Vec<[f32; 3]>>) -> bool {
    let Some(places) = loaded.get(&e.name) else { return false };
    if !shared {
        return true;
    }
    let reach = if e.npc { SAME_PERSON } else { SAME_PLACE };
    places.iter().any(|p| (p[0] - e.at[0]).hypot(p[1] - e.at[1]) <= reach)
}

/// How near a loaded actor of a shared name must stand to be the survey's (cm).
const SAME_PLACE: f32 = 500.0;
const SAME_PERSON: f32 = 3000.0;

fn runtime_name(cooked: &str) -> String {
    match cooked.rsplit_once('_') {
        Some((head, tail))
            if head.contains("_UAID_") && !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) =>
        {
            head.to_string()
        }
        _ => cooked.to_string(),
    }
}

/// Whether an item is one a quest needs: a quest's or a secret's own.
/// The collectibles counted, by their folder under `/Game/Items/`, and what the panel
/// calls them.
pub const COLLECT: [(&str, &str); 10] = [
    ("Relics", "RELICS"),
    ("LoreItems", "RECORDS"),
    ("Research", "RESEARCH"),
    ("Cosmetic", "CAPS"),
    ("Drone", "DRONE_MODULES"),
    ("WeaponModules", "LYMBIC_SKILLS"),
    ("Weapons", "WEAPONS"),
    ("DefensiveGears", "DEFENSIVE_GEAR"),
    ("Lymbic", "LYMBIC_RODS"),
    ("CraftingTomes", "CRAFTING_TOMES"),
];

/// The map sort whose icon shows a collectible sort (by its `COLLECT` label).
pub fn collect_sort(label: &str) -> crate::actors::Sub {
    use crate::actors::Sub;
    let folder = COLLECT.iter().find(|(_, l)| *l == label).map_or("", |(f, _)| *f);
    match folder {
        "Relics" => Sub::Stash,
        "LoreItems" | "CraftingTomes" => Sub::Lore,
        "Research" => Sub::Research,
        "Cosmetic" | "DefensiveGears" => Sub::Gear,
        "Drone" => Sub::DroneModule,
        "WeaponModules" | "Lymbic" => Sub::Skill,
        "Weapons" => Sub::Weapon,
        _ => Sub::OtherItem,
    }
}

/// An item's collectible sort, if it is one.
fn category(path: &str) -> Option<&'static str> {
    let folder = path.split("/Items/").nth(1)?.split('/').next()?;
    COLLECT.iter().find(|(f, _)| *f == folder).map(|(_, label)| *label)
}

fn wanted_item(path: &str) -> bool {
    path.contains("/Items/Quests/") || path.contains("/Items/Secrets/")
}

impl Entry {
    fn add_payload(&mut self, p: &Value) {
        for c in names(&p["items"]).iter().filter_map(|i| category(i)) {
            if !self.cats.contains(&c) {
                self.cats.push(c);
            }
        }
        for name in names(&p["items"]).iter().map(|i| i.rsplit('/').next().unwrap_or(i)).filter(|n| is_rod(n)) {
            if !self.rods.iter().any(|r| r == name) {
                self.rods.push(name.to_string());
            }
        }
        for path in names(&p["items"]).iter().filter(|p| wanted_item(p)) {
            let (name, key) = item(path);
            if !self.items.contains(&name) {
                self.items.push(name);
            }
            if let Some(k) = key.filter(|k| !self.keys.contains(k)) {
                self.keys.push(k);
            }
        }
        for f in names(&p["facts"]) {
            if !self.facts.contains(&f) {
                self.facts.push(f);
            }
        }
        for t in names(&p["tags"]) {
            if !self.tags.contains(&t) {
                self.tags.push(t);
            }
        }
    }

    /// What is still to get here: facts and tags not known (topic unlocks aside), items
    /// not held. An item counts as got once the place's facts and tags are all known —
    /// keys and notes are used up, and leave the inventory.
    ///
    /// A pickup or device the save keeps a state for has been taken or used (verified:
    /// a photo picked up gains a `PersistentActorSaveGameState` under its GUID; those not
    /// picked up have none). Not for NPCs: having talked once says nothing of the rest.
    pub fn left(&self, k: &Known) -> Left {
        if !self.npc && self.guid.as_ref().is_some_and(|g| k.saved.contains(g)) {
            return Left::default();
        }
        if self.npc && k.talked.contains(&self.name) {
            return Left::default();
        }
        let facts = self.facts.iter().filter(|f| !k.facts.contains(*f)).count();
        let tags: Vec<String> =
            self.tags.iter().filter(|t| !t.starts_with("Conversation.") && !k.tags.contains(*t)).cloned().collect();
        let settled = !(self.facts.is_empty() && self.tags.is_empty()) && facts == 0 && tags.is_empty();
        let items =
            if settled { Vec::new() } else { self.items.iter().filter(|i| !k.held.contains(*i)).cloned().collect() };
        Left { facts, tags, items }
    }

    /// A stable goal id: the name's hash with the top bit set, never an address.
    /// Its name and its place to the metre: a World Partition name is the same for every
    /// actor one editor session placed (`…_UAID_<16 hex>`, the survey drops the number
    /// after it), so the name alone made different places one.
    pub fn id(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let at = self.at.map(|v| (v / 100.0).round() as i32);
        for b in self.name.bytes().chain(at.iter().flat_map(|v| v.to_le_bytes())) {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        h | 1 << 63
    }

    pub fn label(&self) -> String {
        if self.npc {
            let p = crate::goals::npc_label(&self.class);
            trf!("TALK_NPC", p = p)
        } else {
            // A pickup by what it gives, in the game's words; a place that gives no item
            // (a fact, a tag) has no name the game shows: what it is, by its class — a fight
            // (a spawner), a lever, something to look at, a door — and only else "a place to
            // examine" (a boss's fight far below read as one, seen in play).
            match self.items.first() {
                Some(i) => crate::goals::item_label(i),
                None if self.class.ends_with("_Spawner_C") => tr!("GRAPH_FIGHT").to_string(),
                None => crate::actors::by_class(&self.class)
                    .map_or_else(|| tr!("PLACE_TO_EXAMINE").to_string(), |s| s.label().to_string()),
            }
        }
    }
}

impl Survey {
    /// The ways out of `world` (its name as the survey has it).
    pub fn exits(&self, world: &str) -> Vec<(crate::actors::Sub, [f32; 3])> {
        self.exits.iter().filter(|(w, ..)| w == world).map(|(_, s, at)| (*s, *at)).collect()
    }

    /// Read `dir\*.json`; empty when there is none.
    pub fn load(dir: &Path) -> Survey {
        let read = |p: &Path| std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
        let flows = read(&dir.join("flows.json")).unwrap_or(Value::Null);
        // A conversation's payloads, through its sub-graphs.
        let flow_payloads = |root: &str| {
            let mut out = Vec::new();
            let mut stack = vec![root.to_string()];
            let mut seen = HashSet::new();
            while let Some(f) = stack.pop() {
                if !seen.insert(f.clone()) || seen.len() > 200 {
                    continue;
                }
                let node = &flows[f.as_str()];
                out.extend(node["payloads"].as_array().cloned().unwrap_or_default());
                stack.extend(names(&node["subgraphs"]));
            }
            out
        };
        let mut worlds = HashMap::new();
        let mut doors = Vec::new();
        let mut puzzles = Vec::new();
        let mut exits: Vec<(String, crate::actors::Sub, [f32; 3])> = Vec::new();
        let Ok(files) = std::fs::read_dir(dir) else { return Survey::default() };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "json") || path.file_name().is_some_and(|n| n == "flows.json") {
                continue;
            }
            let Some(v) = read(&path) else { continue };
            let Some(world) = v["world"].as_str() else { continue };
            let mut list = Vec::new();
            for a in v["actors"].as_array().into_iter().flatten() {
                let at =
                    a["at"].as_array().map(|x| x.iter().map(|c| c.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
                let Some(at) = at.filter(|x| x.len() == 3) else { continue };
                if let Some(travel) = a["travel"].as_str() {
                    use crate::actors::Sub;
                    let sub = match travel {
                        "apc" => Sub::Apc,
                        "save.local" => Sub::SavePointLocal,
                        _ => Sub::SavePoint,
                    };
                    let at = [at[0], at[1], at[2]];
                    // One per place: a blueprint holds a copy per cell it streams in.
                    if !exits
                        .iter()
                        .any(|(w, s, p)| w == world && *s == sub && (p[0] - at[0]).hypot(p[1] - at[1]) < 300.0)
                    {
                        exits.push((world.to_string(), sub, at));
                    }
                    continue;
                }
                if let Some((kind, answer)) = placed_answer(&a["puzzle"]) {
                    let p = Placed {
                        world: world.to_string(),
                        class: a["class"].as_str().unwrap_or("").to_string(),
                        at: [at[0], at[1], at[2]],
                        guid: a["guid"].as_str().map(str::to_string),
                        kind,
                        answer,
                        choice: a["puzzle"]["expects"].as_str().map(|e| match e.rsplit('/').next().unwrap_or("") {
                            "" => Choice::Decoy,
                            item => Choice::Right(item.to_string()),
                        }),
                    };
                    // A blueprint holds a copy per cell it streams in: one is enough.
                    if !puzzles.iter().any(|q: &Placed| {
                        q.world == p.world && (q.at[0] - p.at[0]).hypot(q.at[1] - p.at[1]) < 50.0 && q.kind == p.kind
                    }) {
                        puzzles.push(p);
                    }
                }
                if a["vault"].as_bool() == Some(true) {
                    doors.push((world.to_string(), [at[0], at[1], at[2]]));
                    continue;
                }
                let mut e = Entry {
                    name: runtime_name(a["name"].as_str().unwrap_or("")),
                    class: a["class"].as_str().unwrap_or("").to_string(),
                    at: [at[0], at[1], at[2]],
                    guid: a["guid"].as_str().map(str::to_string),
                    ..Default::default()
                };
                e.add_payload(&a["payload"]);
                if let Some(flow) = a["flow"].as_str() {
                    e.npc = true;
                    e.flow = Some(flow.to_string());
                    for p in flow_payloads(flow) {
                        e.add_payload(&p);
                    }
                }
                for t in a["trades"].as_array().into_iter().flatten() {
                    e.add_payload(&t["payload"]);
                    if let Some(w) = t["item"].as_str() {
                        let want = item(w).0;
                        e.wants.get_or_insert(want.clone());
                        e.trades.push((want, names(&t["payload"]["facts"]), names(&t["payload"]["tags"])));
                    }
                }
                // A copy per cell it streams in, at the same place: one is enough.
                let copy = list
                    .iter()
                    .any(|x: &Entry| x.name == e.name && (x.at[0] - e.at[0]).hypot(x.at[1] - e.at[1]) < 100.0);
                if !copy && !(e.items.is_empty() && e.facts.is_empty() && e.tags.is_empty() && e.cats.is_empty()) {
                    list.push(e);
                }
            }
            worlds.insert(world.to_string(), list);
        }
        Survey { worlds, doors, puzzles, exits }
    }

    pub fn is_empty(&self) -> bool {
        self.worlds.is_empty()
    }

    /// The survey's world for a loaded world's name (`AcasaMarshes_Root_WP`).
    pub fn world_of(loaded: &str) -> &str {
        loaded.strip_suffix("_Root_WP").unwrap_or(loaded)
    }

    /// Goals in `world` that are not loaded (`loaded` names what is), with something
    /// still left. `quests` names a fact's main quest key, where known.
    ///
    /// A person is where their conversation was last seen loaded (`met`, by flow, kept
    /// by the caller): their other placements are where the story had them before or
    /// will later, and were pins to the wrong place (Victor Gaz's first meeting in the
    /// tunnel, long after he moved to the forge).
    pub fn goals(
        &self,
        world: &str,
        k: &Known,
        loaded: &HashMap<String, Vec<[f32; 3]>>,
        quests: &HashMap<String, String>,
        met: &mut HashMap<String, String>,
        empty: &HashSet<u64>,
    ) -> Vec<Goal> {
        let Some(list) = self.worlds.get(world) else { return Vec::new() };
        let shared = shared_names(list);
        let is_loaded = |e: &Entry, _: &[Entry], loaded: &HashMap<String, Vec<[f32; 3]>>| {
            loaded_as(e, shared.contains(e.name.as_str()), loaded)
        };
        let here = |e: &&Entry| is_loaded(e, list, loaded);
        for e in list.iter().filter(here) {
            if let Some(f) = &e.flow {
                met.insert(f.clone(), e.name.clone());
            }
        }
        list.iter()
            .filter(|e| !is_loaded(e, list, loaded) && !empty.contains(&e.id()))
            .filter(|e| e.flow.as_ref().and_then(|f| met.get(f)).is_none_or(|at| *at == e.name))
            .filter_map(|e| {
                let left = e.left(k);
                if left.is_empty() {
                    return None;
                }
                let mut keys = e.keys.clone();
                for f in &e.facts {
                    if let Some(q) = quests.get(f).filter(|q| !keys.contains(q)) {
                        keys.push(q.clone());
                    }
                }
                let tier = if !keys.is_empty() || left.tags.iter().any(|t| t.starts_with("Quest.")) {
                    Tier::Quest
                } else if !left.items.is_empty() || left.tags.iter().any(|t| t.starts_with("Secrets.")) {
                    Tier::Secret
                } else {
                    Tier::Clue
                };
                let mut detail = match (&e.wants, left.items.first(), left.tags.first()) {
                    (Some(w), _, _) => trf!("HAND_OVER", item = crate::goals::item_label(w)),
                    (_, Some(i), _) => trf!("ITEM", items = crate::goals::item_label(i)),
                    (_, _, Some(t)) => t.clone(),
                    _ => trf!("NEW_FACTS", count = left.facts),
                };
                detail += tr!("SURVEY_DB_NOT_LOADED_YET");
                let gate = crate::goals::gate_of(&e.class);
                if gate == Gate::Conditional {
                    detail += tr!("NEEDS_SOMETHING");
                }
                Some(Goal {
                    tier,
                    id: e.id(),
                    label: e.label(),
                    detail,
                    at: e.at,
                    quests: Vec::new(),
                    tags: left.tags.clone(),
                    keys,
                    gate,
                    // An item's name is the game's; a trigger's class the journal may name.
                    named: !left.items.is_empty(),
                    reveals: if tier == Tier::Quest {
                        crate::goals::Reveal::Nothing
                    } else {
                        crate::goals::Reveal::Places
                    },
                    first: None,
                })
            })
            .collect()
    }

    /// For each world, how many places still hold something for a quest (by key or
    /// tag prefix): what the panel lists as "needed".
    pub fn needs(&self, key: &str, tags: Option<&str>, k: &Known, quests: &HashMap<String, String>) -> Vec<Need> {
        let mut out = Vec::new();
        for (world, list) in &self.worlds {
            for e in list {
                let mine = e.keys.iter().any(|x| x == key)
                    || e.facts.iter().any(|f| quests.get(f).is_some_and(|q| q == key))
                    || tags.is_some_and(|p| e.tags.iter().any(|t| t.starts_with(p)));
                if !mine {
                    continue;
                }
                let left = e.left(k);
                out.push(Need {
                    world: world.clone(),
                    id: e.id(),
                    label: e.label(),
                    what: left
                        .items
                        .first()
                        .map(|i| crate::goals::item_label(i))
                        .or_else(|| e.items.first().map(|i| crate::goals::item_label(i)))
                        .unwrap_or_default(),
                    at: e.at,
                    done: left.is_empty(),
                });
            }
        }
        out.sort_by(|a, b| a.world.cmp(&b.world).then(a.done.cmp(&b.done)));
        out
    }
}

/// One collectible sort's count: in the world the hero is in, and everywhere — (got,
/// placed) — and the nearest not taken here, to guide to.
#[derive(Clone, Debug, PartialEq)]
pub struct Collect {
    pub label: &'static str,
    pub here: (usize, usize),
    pub all: (usize, usize),
    pub left_here: Vec<Need>,
    /// How many are left in each world that has any (the region ledger, ledger.rs).
    pub left_by_world: Vec<(String, usize)>,
}

/// A Lymbic lock: a placement puzzle whose answer is Lymbic rods (.spec/JOURNEY.md §3.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Lock {
    pub id: u64,
    pub world: String,
    pub at: [f32; 3],
    pub solved: bool,
    pub rods: Vec<Rod>,
}

/// One rod a lock takes: held or not, and where the nearest one not taken yet lies.
#[derive(Clone, Debug, PartialEq)]
pub struct Rod {
    /// Its data asset name (`LymbicRod_XRay_Rage_Item_DA`).
    pub item: String,
    pub held: bool,
    pub source: Option<Need>,
}

impl Lock {
    /// Every rod it takes is in the inventory: it can be opened now.
    pub fn openable(&self) -> bool {
        !self.solved && self.rods.iter().all(|r| r.held)
    }
}

/// A Lymbic rod, by its data asset name.
pub fn is_rod(item: &str) -> bool {
    item.starts_with("LymbicRod")
}

impl Survey {
    /// What is placed in the worlds to collect, and how much is taken (the save keeps
    /// a state for each taken pickup). NPC rewards are not placed, so not counted.
    pub fn collection(&self, world: &str, k: &Known) -> Vec<Collect> {
        COLLECT
            .iter()
            .map(|&(_, label)| {
                let mut c =
                    Collect { label, here: (0, 0), all: (0, 0), left_here: Vec::new(), left_by_world: Vec::new() };
                let mut by_world: std::collections::BTreeMap<&str, usize> = Default::default();
                for (w, list) in &self.worlds {
                    for e in list.iter().filter(|e| !e.npc && e.cats.contains(&label)) {
                        let got = e.guid.as_ref().is_some_and(|g| k.saved.contains(g));
                        c.all.1 += 1;
                        c.all.0 += got as usize;
                        if !got {
                            *by_world.entry(w.as_str()).or_default() += 1;
                        }
                        if w == world {
                            c.here.1 += 1;
                            c.here.0 += got as usize;
                            if !got {
                                c.left_here.push(Need {
                                    world: w.clone(),
                                    id: e.id(),
                                    label: e.label(),
                                    what: label.to_string(),
                                    at: e.at,
                                    done: false,
                                });
                            }
                        }
                    }
                }
                c.left_by_world = by_world.into_iter().map(|(w, n)| (w.to_string(), n)).collect();
                c
            })
            .filter(|c| c.all.1 > 0)
            .collect()
    }

    /// Every Lymbic lock in the worlds: solved or not, and for each rod it takes,
    /// whether it is held and, if not, the nearest pickup of it not taken yet — in the
    /// lock's own world first.
    pub fn locks(&self, k: &Known) -> Vec<Lock> {
        use crate::puzzles::Answer;
        let mut seen = HashSet::new();
        self.puzzles
            .iter()
            // The survey can list one panel twice (a copy at the same place).
            .filter(|p| seen.insert(p.id()))
            .filter_map(|p| {
                let Answer::Items(items) = &p.answer else { return None };
                if !items.iter().any(|i| is_rod(i)) {
                    return None;
                }
                let rods = items
                    .iter()
                    .filter(|i| is_rod(i))
                    .map(|i| {
                        let held = k.held.contains(i);
                        Rod {
                            item: i.clone(),
                            held,
                            source: if held { None } else { self.pickup(i, &p.world, p.at, k) },
                        }
                    })
                    .collect();
                Some(Lock {
                    id: p.id(),
                    world: p.world.clone(),
                    at: p.at,
                    solved: p.guid.as_ref().is_some_and(|g| k.saved.contains(g)),
                    rods,
                })
            })
            .collect()
    }

    /// The nearest place not taken yet that hands out `item`: in `world` first (by
    /// distance from `near`), else the first in any other world.
    fn pickup(&self, item: &str, world: &str, near: [f32; 3], k: &Known) -> Option<Need> {
        let mut best: Option<(bool, f32, &String, &Entry)> = None;
        for (w, list) in &self.worlds {
            for e in list.iter().filter(|e| !e.npc && e.rods.iter().any(|i| i == item)) {
                if e.guid.as_ref().is_some_and(|g| k.saved.contains(g)) {
                    continue;
                }
                let here = w == world;
                let d = if here { (e.at[0] - near[0]).hypot(e.at[1] - near[1]) } else { f32::MAX };
                if best.is_none_or(|(bh, bd, bw, _)| (here && !bh) || (here == bh && (d < bd || (d == bd && w < bw)))) {
                    best = Some((here, d, w, e));
                }
            }
        }
        best.map(|(_, _, w, e)| Need {
            world: w.clone(),
            id: e.id(),
            label: e.label(),
            what: item.to_string(),
            at: e.at,
            done: false,
        })
    }

    /// NPCs whose talk still holds something the hero does not know, in every world.
    pub fn stories(&self, k: &Known) -> Vec<Need> {
        let mut out = Vec::new();
        for (world, list) in &self.worlds {
            for e in list.iter().filter(|e| e.npc) {
                if e.left(k).is_empty() {
                    continue;
                }
                out.push(Need {
                    world: world.clone(),
                    id: e.id(),
                    label: e.label(),
                    what: String::new(),
                    at: e.at,
                    done: false,
                });
            }
        }
        out
    }

    /// Hand-overs the hero can make now: NPCs in every world that want an item the
    /// hero holds, and have not been given it (what they give back is still new).
    pub fn handovers(&self, k: &Known) -> Vec<Need> {
        let mut out = Vec::new();
        for (world, list) in &self.worlds {
            for e in list {
                for (want, facts, tags) in &e.trades {
                    // Held, and what it gives back not known yet — a reward of nothing
                    // but topic unlocks is no reason to go.
                    let new = facts.iter().any(|f| !k.facts.contains(f))
                        || tags.iter().any(|t| !t.starts_with("Conversation.") && !k.tags.contains(t));
                    if !k.held.contains(want) || !new {
                        continue;
                    }
                    out.push(Need {
                        world: world.clone(),
                        id: e.id(),
                        label: e.label(),
                        what: crate::goals::item_label(want),
                        at: e.at,
                        done: false,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.world.cmp(&b.world).then(a.what.cmp(&b.what)));
        out.dedup_by(|a, b| a.id == b.id && a.what == b.what);
        out
    }
}

/// One place a quest needs, for the panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Need {
    pub world: String,
    pub id: u64,
    pub label: String,
    pub what: String,
    pub at: [f32; 3],
    pub done: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(items: &[&str], facts: &[&str], tags: &[&str]) -> Entry {
        Entry {
            name: "A".into(),
            items: items.iter().map(|s| s.to_string()).collect(),
            facts: facts.iter().map(|s| s.to_string()).collect(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_person_is_where_their_conversation_was_last_loaded() {
        let talk = |name: &str, x: f32| Entry {
            name: name.into(),
            at: [x, 0.0, 0.0],
            npc: true,
            flow: Some("VictorGaz_ConvoRoot_FA".into()),
            ..entry(&["Key_Item_DA"], &[], &[])
        };
        let survey = Survey {
            worlds: HashMap::from([("W".to_string(), vec![talk("Tunnel_BP_2", 9000.0), talk("Forge_BP_2", 10.0)])]),
            doors: Vec::new(),
            puzzles: Vec::new(),
            exits: Vec::new(),
        };
        let none = HashSet::new();
        let k = Known { facts: &none, tags: &none, held: &none, saved: &none, talked: &none };
        let mut met = HashMap::new();
        let at = |loaded: &[&str], met: &mut HashMap<String, String>| -> Vec<f32> {
            let loaded: HashMap<String, Vec<[f32; 3]>> = loaded.iter().map(|s| (s.to_string(), Vec::new())).collect();
            survey.goals("W", &k, &loaded, &HashMap::new(), met, &HashSet::new()).iter().map(|g| g.at[0]).collect()
        };
        // Not met yet: either placement may be where they are.
        assert_eq!(at(&[], &mut met), [9000.0, 10.0]);
        // Met at the forge: the live goal is the forge's; the tunnel is not shown.
        assert_eq!(at(&["Forge_BP_2"], &mut met), Vec::<f32>::new());
        // Gone out of reach again: the forge, where they were last seen.
        assert_eq!(at(&[], &mut met), [10.0]);
    }

    #[test]
    fn a_shared_name_is_told_apart_by_its_place() {
        // One editor session's World Partition actors share a name once the number after
        // `_UAID_` is dropped: two save points far apart, one loaded.
        let at = |x: f32| Entry { at: [x, 0.0, 0.0], name: "Save_UAID_AB".into(), ..entry(&["Key_Item_DA"], &[], &[]) };
        let survey =
            Survey { worlds: HashMap::from([("W".to_string(), vec![at(0.0), at(20_000.0)])]), ..Survey::default() };
        let none = HashSet::new();
        let k = Known { facts: &none, tags: &none, held: &none, saved: &none, talked: &none };
        let loaded = HashMap::from([("Save_UAID_AB".to_string(), vec![[20_050.0, 0.0, 0.0]])]);
        let shown: Vec<f32> = survey
            .goals("W", &k, &loaded, &HashMap::new(), &mut HashMap::new(), &HashSet::new())
            .iter()
            .map(|g| g.at[0])
            .collect();
        assert_eq!(shown, [0.0], "the far one is left to the live goals, the other still shown");
        assert_ne!(at(0.0).id(), at(20_000.0).id(), "two places, two ids");
    }

    #[test]
    fn what_is_left_follows_knowledge_and_the_inventory() {
        let (facts, tags, held) =
            (HashSet::from(["F1".to_string()]), HashSet::new(), HashSet::from(["Key_Item_DA".to_string()]));
        let saved = HashSet::from(["G".to_string()]);
        let talked = HashSet::new();
        let k = Known { facts: &facts, tags: &tags, held: &held, saved: &saved, talked: &talked };
        let taken = Entry { guid: Some("G".into()), ..entry(&["Photo_Item_DA"], &["F9"], &[]) };
        assert!(taken.left(&k).is_empty(), "the save keeps its state: taken");
        assert!(!Entry { npc: true, ..taken.clone() }.left(&k).is_empty(), "an NPC talked to once is not done");
        assert!(entry(&["Key_Item_DA"], &[], &[]).left(&k).is_empty(), "held");
        assert_eq!(entry(&["Note_Item_DA"], &[], &[]).left(&k).items, ["Note_Item_DA"], "not held");
        assert!(entry(&["Note_Item_DA"], &["F1"], &[]).left(&k).is_empty(), "its fact known: got, though used up");
        assert_eq!(entry(&[], &["F1", "F2"], &[]).left(&k).facts, 1);
        assert!(
            entry(&[], &[], &["Conversation.TopicsUnlock.X"]).left(&k).is_empty(),
            "topic unlocks are not a reason"
        );
    }

    #[test]
    fn locks_know_their_rods_held_missing_and_where() {
        use crate::puzzles::{Answer, Kind};
        let lock = |world: &str, guid: Option<&str>, rods: &[&str]| Placed {
            world: world.into(),
            class: "LymbicLockPanel_2ndGen_Rage_X_V_Interact_BP_C".into(),
            at: [0.0, 0.0, 0.0],
            guid: guid.map(str::to_string),
            kind: Kind::Placement,
            answer: Answer::Items(rods.iter().map(|r| r.to_string()).collect()),
            choice: None,
        };
        // Through the payload reader, as the survey's own paths come in.
        let pick = |name: &str, at: [f32; 3], guid: &str, item: &str| {
            let mut e = Entry { name: name.into(), at, guid: Some(guid.into()), ..Default::default() };
            e.add_payload(&serde_json::json!({ "items": [format!("/Game/Items/Lymbic/RodsRage/{item}")] }));
            e
        };
        let survey = Survey {
            worlds: HashMap::from([
                (
                    "Jeljin".to_string(),
                    vec![
                        pick("far", [9000.0, 0.0, 0.0], "g1", "LymbicRod_Victor_Rage_Item_DA"),
                        pick("near", [100.0, 0.0, 0.0], "g2", "LymbicRod_Victor_Rage_Item_DA"),
                        pick("taken", [10.0, 0.0, 0.0], "g3", "LymbicRod_Victor_Rage_Item_DA"),
                    ],
                ),
                ("Talju".to_string(), vec![pick("away", [0.0, 0.0, 0.0], "g4", "LymbicRod_Victor_Rage_Item_DA")]),
            ]),
            doors: Vec::new(),
            puzzles: vec![
                lock("Jeljin", Some("L1"), &["LymbicRod_XRay_Rage_Item_DA", "LymbicRod_Victor_Rage_Item_DA"]),
                lock("Talju", Some("L2"), &["LymbicRod_XRay_Rage_Item_DA"]),
                Placed { answer: Answer::Items(vec!["Door_Key_Item_DA".into()]), ..lock("Talju", None, &[]) },
            ],
            exits: Vec::new(),
        };
        let (facts, tags, talked) = (HashSet::new(), HashSet::new(), HashSet::new());
        let held = HashSet::from(["LymbicRod_XRay_Rage_Item_DA".to_string()]);
        let saved = HashSet::from(["g3".to_string(), "L2".to_string()]);
        let k = Known { facts: &facts, tags: &tags, held: &held, saved: &saved, talked: &talked };
        let locks = survey.locks(&k);
        assert_eq!(locks.len(), 2, "a key door is not a Lymbic lock");
        let jeljin = locks.iter().find(|l| l.world == "Jeljin").unwrap();
        assert!(!jeljin.openable(), "one rod missing");
        let missing = jeljin.rods.iter().find(|r| !r.held).unwrap();
        assert_eq!(missing.source.as_ref().map(|n| n.world.as_str()), Some("Jeljin"), "its own world first");
        assert_eq!(missing.source.as_ref().map(|n| n.at[0]), Some(100.0), "the nearest not taken");
        assert!(jeljin.rods.iter().find(|r| r.held).unwrap().source.is_none());
        let talju = locks.iter().find(|l| l.world == "Talju").unwrap();
        assert!(talju.solved && !talju.openable(), "solved locks are not to open again");
    }

    #[test]
    fn cooked_names_lose_their_number() {
        assert_eq!(runtime_name("X_BP_C_UAID_047C16054F89B91D02_1325171520"), "X_BP_C_UAID_047C16054F89B91D02");
        assert_eq!(runtime_name("Plain_Name_12"), "Plain_Name_12");
    }

    #[test]
    fn quest_items_name_their_quest() {
        assert_eq!(
            item("/Game/Items/Quests/Quest01/Quest01_PictureC_Item_DA"),
            ("Quest01_PictureC_Item_DA".into(), Some("Quest01".into()))
        );
        assert_eq!(item("/Game/Items/Secrets/AcasaMarshes/X_Item_DA").1, None);
        assert!(entry(&[], &[], &[]).id() >> 63 == 1, "never an address");
    }
}
