//! What every world hands out, from the survey of the game's maps (tools/survey,
//! `Mods\survey\*.json`; .spec/ITEMS.md): pickups, devices, markers, NPCs and what
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
fn runtime_name(cooked: &str) -> String {
    match cooked.rsplit_once('_') {
        Some((head, tail)) if head.contains("_UAID_") && !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head.to_string(),
        _ => cooked.to_string(),
    }
}

/// Whether an item is one a quest needs: a quest's or a secret's own.
/// The collectibles counted, by their folder under `/Game/Items/`, and what the panel
/// calls them.
pub const COLLECT: [(&str, &str); 10] = [
    ("Relics", "유물"),
    ("LoreItems", "기록물"),
    ("Research", "연구 자료"),
    ("Cosmetic", "모자"),
    ("Drone", "드론 모듈"),
    ("WeaponModules", "림빅 스킬"),
    ("Weapons", "무기"),
    ("DefensiveGears", "방어구"),
    ("Lymbic", "림빅 막대"),
    ("CraftingTomes", "제작서"),
];

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
        let items = if settled { Vec::new() } else { self.items.iter().filter(|i| !k.held.contains(*i)).cloned().collect() };
        Left { facts, tags, items }
    }

    /// A stable goal id: the name's hash with the top bit set, never an address.
    pub fn id(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in self.name.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        h | 1 << 63
    }

    pub fn label(&self) -> String {
        let p = crate::goals::pretty(&self.class);
        if self.npc {
            format!("대화: {p}")
        } else {
            p
        }
    }
}

impl Survey {
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
                let at = a["at"].as_array().map(|x| x.iter().map(|c| c.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
                let Some(at) = at.filter(|x| x.len() == 3) else { continue };
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
                if !(e.items.is_empty() && e.facts.is_empty() && e.tags.is_empty() && e.cats.is_empty()) {
                    list.push(e);
                }
            }
            worlds.insert(world.to_string(), list);
        }
        Survey { worlds }
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
    pub fn goals(&self, world: &str, k: &Known, loaded: &HashSet<String>, quests: &HashMap<String, String>) -> Vec<Goal> {
        let Some(list) = self.worlds.get(world) else { return Vec::new() };
        list.iter()
            .filter(|e| !loaded.contains(&e.name))
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
                    (Some(w), _, _) => format!("전달: {}", crate::goals::item_label(w)),
                    (_, Some(i), _) => format!("아이템: {}", crate::goals::item_label(i)),
                    (_, _, Some(t)) => t.clone(),
                    _ => format!("새 사실 {}개", left.facts),
                };
                detail += " · 조사 DB (아직 로드 안 됨)";
                let gate = crate::goals::gate_of(&e.class);
                if gate == Gate::Conditional {
                    detail += " · 조건 필요";
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
                        .unwrap_or_else(|| crate::goals::pretty(&e.class)),
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
}

impl Survey {
    /// What is placed in the worlds to collect, and how much is taken (the save keeps
    /// a state for each taken pickup). NPC rewards are not placed, so not counted.
    pub fn collection(&self, world: &str, k: &Known) -> Vec<Collect> {
        COLLECT
            .iter()
            .map(|&(_, label)| {
                let mut c = Collect { label, here: (0, 0), all: (0, 0), left_here: Vec::new() };
                for (w, list) in &self.worlds {
                    for e in list.iter().filter(|e| !e.npc && e.cats.contains(&label)) {
                        let got = e.guid.as_ref().is_some_and(|g| k.saved.contains(g));
                        c.all.1 += 1;
                        c.all.0 += got as usize;
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
                c
            })
            .filter(|c| c.all.1 > 0)
            .collect()
    }

    /// NPCs whose talk still holds something the hero does not know, in every world.
    pub fn stories(&self, k: &Known) -> Vec<Need> {
        let mut out = Vec::new();
        for (world, list) in &self.worlds {
            for e in list.iter().filter(|e| e.npc) {
                if e.left(k).is_empty() {
                    continue;
                }
                out.push(Need { world: world.clone(), id: e.id(), label: e.label(), what: String::new(), at: e.at, done: false });
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
    fn what_is_left_follows_knowledge_and_the_inventory() {
        let (facts, tags, held) = (HashSet::from(["F1".to_string()]), HashSet::new(), HashSet::from(["Key_Item_DA".to_string()]));
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
        assert!(entry(&[], &[], &["Conversation.TopicsUnlock.X"]).left(&k).is_empty(), "topic unlocks are not a reason");
    }

    #[test]
    fn cooked_names_lose_their_number() {
        assert_eq!(runtime_name("X_BP_C_UAID_047C16054F89B91D02_1325171520"), "X_BP_C_UAID_047C16054F89B91D02");
        assert_eq!(runtime_name("Plain_Name_12"), "Plain_Name_12");
    }

    #[test]
    fn quest_items_name_their_quest() {
        assert_eq!(item("/Game/Items/Quests/Quest01/Quest01_PictureC_Item_DA"), ("Quest01_PictureC_Item_DA".into(), Some("Quest01".into())));
        assert_eq!(item("/Game/Items/Secrets/AcasaMarshes/X_Item_DA").1, None);
        assert!(entry(&[], &[], &[]).id() >> 63 == 1, "never an address");
    }
}
