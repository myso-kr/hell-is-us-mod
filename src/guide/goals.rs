//! Where the hero can still learn something: interactables whose payload holds facts
//! or tags the hero does not have yet (.spec/GUIDE.md §3).
//!
//! Every `InteractableActor` with a `PayloadRuneComponent` says what it hands out:
//! `Rune.PayloadData.ContainedFacts` (FactData assets), `Rune.ContainedFacts`, and
//! `Rune.PayloadData.TagFacts` (gameplay tags). Against the hero's knowledge
//! (knowledge.rs) that sorts them:
//! - **quest** — a new fact belonging to an open investigation, or a new `Quest.` tag
//! - **secret** — a new `Secrets.` tag (mysteries, good deeds, timeloops)
//! - **clue** — anything else new
//!
//! NPCs hand things out by talking: their `FlowComponent`s run a conversation
//! (`RootFlow`, a FlowAsset) whose `FlowNode_Payload` nodes carry the same
//! PayloadData, some inside topic subgraphs (other FlowAssets, named softly — found
//! by name among the loaded ones). An NPC that wants an item (`TradeGiveItemRune`
//! `ValidTrades`: the item, and the payload it gives back) is a goal for what it
//! gives back — that is how a good deed's hand-over is found. Topic-unlock tags
//! (`Conversation.`) alone do not make a goal.
//!
//! Pickups that hand out a quest item (`/Items/Quests/`, `/Items/Secrets/`) are goals
//! too, until taken; a `QuestNN_` item belongs to that main quest.
//!
//! What each interactable hands out does not change, so it is read once per actor;
//! NPCs are read again on every scan, as their topics stream in. Whether it is still
//! new is decided every step against the knowledge of the moment.

use crate::knowledge::Knowledge;
use crate::mem::{self, Memory};
use crate::names::{Names, CLASS, NAME, OUTER};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    Quest,
    Secret,
    Clue,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Quest, Tier::Secret, Tier::Clue];

    pub fn label(self) -> &'static str {
        match self {
            Tier::Quest => tr!("QUEST_GOAL"),
            Tier::Secret => tr!("SECRET"),
            Tier::Clue => tr!("CLUE"),
        }
    }

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Tier::Quest => [255, 90, 200],
            Tier::Secret => [120, 230, 230],
            Tier::Clue => [200, 200, 140],
        }
    }
}

/// A place to go, for the compass and the guide.
#[derive(Clone, Debug, PartialEq)]
pub struct Goal {
    pub tier: Tier,
    /// The actor: stable while it is loaded, so a choice can follow it.
    pub id: u64,
    pub label: String,
    /// What is new there: the first new fact or tag, by name.
    pub detail: String,
    pub at: [f32; 3],
    /// The main quests (QuestData name indices) its new facts belong to, and its new
    /// tags by name — what ties it to a quest in the journal.
    pub quests: Vec<u32>,
    pub tags: Vec<String>,
    /// Journal keys it belongs to directly (`Quest01` for a quest item).
    pub keys: Vec<String>,
    pub gate: Gate,
}

/// Whether going there is enough.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gate {
    /// Something to use, pick up or talk to.
    #[default]
    Open,
    /// A `_PayloadInactive_` marker that pays out when the hero comes near
    /// (`Visited`, `Proximity`, `Met`).
    Visit,
    /// A `_PayloadInactive_` marker that pays out when something else happens there —
    /// a door opened with a key, a puzzle solved: going there is not enough, so auto
    /// guiding never picks it.
    Conditional,
}

/// What a `_PayloadInactive_` actor's class says about how it pays out.
pub fn gate_of(class: &str) -> Gate {
    match class.split_once("_PayloadInactive_") {
        None => Gate::Open,
        Some((stem, _)) if ["Visited", "Proximity", "Met"].iter().any(|w| stem.ends_with(w)) => Gate::Visit,
        Some(_) => Gate::Conditional,
    }
}

impl Goal {
    /// Whether it moves `q` along.
    pub fn serves(&self, q: &crate::quests::Quest) -> bool {
        q.quest.is_some_and(|i| self.quests.contains(&i))
            || self.keys.contains(&q.key)
            || q.tags.as_ref().is_some_and(|p| self.tags.iter().any(|t| t.starts_with(p.as_str())))
    }
}

/// What an actor hands out, as name indices.
#[derive(Clone, Debug, Default)]
struct Payload {
    /// (fact, its investigation if it has one).
    facts: Vec<(u32, Option<u32>)>,
    tags: Vec<u32>,
    /// `InteractionActionComponent.bHasBeenActivated`: used already.
    used: Option<u64>,
    root: u64,
    label: String,
    /// Quest items it hands out, by name — a goal while it is not used.
    items: Vec<String>,
    /// Journal keys it belongs to directly.
    keys: Vec<String>,
    /// What to say instead of the first new tag (an NPC's hand-over).
    note: Option<String>,
    gate: Gate,
    /// An NPC: what it gives comes from every branch of its conversations, and some
    /// stay closed until a topic opens elsewhere.
    npc: bool,
}

fn talked_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("talked.txt")
}

/// `talked.txt`: an NPC's name, a tab, the topic unlocks known when the talk was had.
fn load_talked() -> HashMap<String, usize> {
    std::fs::read_to_string(talked_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter_map(|(name, n)| Some((name.to_string(), n.parse().ok()?)))
        .collect()
}

fn save_talked(talked: &HashMap<String, usize>) {
    let mut lines: Vec<String> = talked.iter().map(|(name, n)| format!("{name}\t{n}")).collect();
    lines.sort();
    let _ = std::fs::write(talked_path(), lines.join("\n") + "\n");
}

/// A PayloadData's facts (asset addresses), tags (name indices) and the items it
/// adds (addresses): ContainedFacts +0x18 (a set), TagFacts +0x68, ItemsToAdd +0x88.
fn payload_data(m: &dyn Memory, at: u64) -> (Vec<u64>, Vec<u32>, Vec<u64>) {
    let facts = pointer_set(m, at + 0x18);
    let tags = tag_array(m, at + 0x68);
    let items = pointer_array(m, at + 0x88);
    (facts, tags, items)
}

/// A GameplayTagContainer's tags, as name indices.
fn tag_array(m: &dyn Memory, at: u64) -> Vec<u32> {
    let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return Vec::new() };
    if num == 0 || num > 256 || !mem::plausible(data) {
        return Vec::new();
    }
    (0..num as u64).filter_map(|i| mem::read_u32(m, data + i * 8)).collect()
}

/// An item a quest needs: under `/Items/Quests/` or `/Items/Secrets/`. With the main
/// quest it belongs to, when its name starts `QuestNN_`.
fn quest_item(m: &dyn Memory, n: &Names, item: u64) -> Option<(String, Option<String>)> {
    let pkg = mem::read_u64(m, item + OUTER).filter(|&p| mem::plausible(p)).and_then(|p| n.object(m, p))?;
    if !pkg.contains("/Items/Quests/") && !pkg.contains("/Items/Secrets/") {
        return None;
    }
    let name = n.object(m, item)?;
    let key = name.get(..7).filter(|k| k.starts_with("Quest") && k[5..].chars().all(|c| c.is_ascii_digit()));
    Some((item_label(&name), key.map(str::to_string)))
}

/// `Caddell_GoldenWatch_Item_DA` → `Caddell GoldenWatch`.
pub fn item_label(name: &str) -> String {
    if let Some(n) = crate::i18n::item(name) {
        return n;
    }
    let name = name.rsplit('/').next().unwrap_or(name);
    let lower = name.to_lowercase();
    let cut = lower.find("_item").unwrap_or(name.len());
    name[..cut].replace('_', " ")
}

const RESCAN: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct Goals {
    levels: Option<u64>,
    interactable: HashMap<u64, bool>,
    payloads: HashMap<u64, (u64, Option<Payload>)>,
    /// Whether a class is an NPC, by class.
    npc: HashMap<u64, bool>,
    /// NPCs talked to: how much of what they give was known when last seen, and how
    /// many topic unlocks the hero knew when the talk gave something — the NPC is done
    /// until another topic opens somewhere (a new `Conversation.` tag).
    heard: HashMap<u64, usize>,
    /// By the NPC's name, kept in `Mods\talked.txt` across runs.
    talked: HashMap<String, usize>,
    talked_loaded: bool,
    /// The NPCs done with for now (talked to, no topic opened since), by name.
    pub done_npcs: std::collections::HashSet<String>,
    /// The names of the interactables and NPCs loaded now: the survey leaves these to
    /// the live goals.
    pub loaded: std::collections::HashSet<String>,
    names: HashMap<u64, String>,
    /// A fact asset's investigation, by the fact's address.
    fact_quest: HashMap<u64, Option<u32>>,
    /// Tag names, by index — for the tier rules and for the detail line.
    tag_names: HashMap<u32, String>,
    scanned: Option<Instant>,
}

/// An NPC's name: the game's (the real one once learned), else made from its class.
pub fn npc_label(class: &str) -> String {
    crate::i18n::npc_known(class).unwrap_or_else(|| pretty(class))
}

/// `Cons_MedicineCivilianT01_GatherSingleUse_Interact_BP_C` → `Cons MedicineCivilianT01`.
pub fn pretty(class: &str) -> String {
    let mut s = class.to_string();
    for tail in [
        "_GatherSingleUse_Interact_BP_C",
        "_PayloadInactive_Interact_BP_C",
        "_Interact_BP_C",
        "_Interact_BP1_C",
        "_BP_C",
        "_C",
    ] {
        if let Some(t) = s.strip_suffix(tail) {
            s = t.to_string();
            break;
        }
    }
    s.replace('_', " ")
}

fn pointer_set(m: &dyn Memory, at: u64) -> Vec<u64> {
    // TSet<T*>: a TSparseArray whose TArray holds { value, next, hash } — 16 bytes.
    let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return Vec::new() };
    if !mem::plausible(data) || num == 0 || num > 4096 {
        return Vec::new();
    }
    let mut buf = vec![0u8; num as usize * 16];
    if !m.read(data, &mut buf) {
        return Vec::new();
    }
    buf.chunks_exact(16)
        .map(|e| u64::from_le_bytes(e[..8].try_into().unwrap()))
        .filter(|&p| mem::plausible(p))
        .collect()
}

/// A FlowAsset's nodes: `Nodes` (+0x50) is a TMap<FGuid, UFlowNode*> — elements of
/// 32 bytes, the node at +0x10.
fn flow_nodes(m: &dyn Memory, asset: u64) -> Vec<u64> {
    let (Some(data), Some(num)) = (mem::read_u64(m, asset + 0x50), mem::read_u32(m, asset + 0x58)) else {
        return Vec::new();
    };
    if !mem::plausible(data) || num > 4096 {
        return Vec::new();
    }
    let mut buf = vec![0u8; num as usize * 32];
    if !m.read(data, &mut buf) {
        return Vec::new();
    }
    buf.chunks_exact(32).map(|e| u64::from_le_bytes(e[16..24].try_into().unwrap())).filter(|&p| mem::plausible(p)).collect()
}

fn pointer_array(m: &dyn Memory, at: u64) -> Vec<u64> {
    crate::actors::array(m, at, 4096)
}

impl Goals {
    fn quest_of(&mut self, m: &dyn Memory, n: &Names, fact: u64) -> Option<u32> {
        *self.fact_quest.entry(fact).or_insert_with(|| {
            let q = n.follow(m, fact, "AssociatedQuestData").ok()?;
            mem::read_u32(m, q + NAME)
        })
    }

    fn read(&mut self, m: &dyn Memory, n: &Names, actor: u64, root: u64) -> Option<Payload> {
        let comp = crate::actors::component(m, n, actor, "PayloadRuneComponent")?;
        let mut facts: Vec<u64> = Vec::new();
        if let Some((at, _)) = n.path(m, comp, &["Rune", "PayloadData", "ContainedFacts"]) {
            facts.extend(pointer_set(m, at));
        }
        if let Some((at, _)) = n.path(m, comp, &["Rune", "ContainedFacts"]) {
            facts.extend(pointer_array(m, at));
        }
        facts.sort_unstable();
        facts.dedup();
        let facts: Vec<(u32, Option<u32>)> =
            facts.into_iter().filter_map(|f| Some((mem::read_u32(m, f + NAME)?, self.quest_of(m, n, f)))).collect();
        let tags: Vec<u32> = n
            .path(m, comp, &["Rune", "PayloadData", "TagFacts", "GameplayTags"])
            .and_then(|(at, _)| {
                let (data, num) = (mem::read_u64(m, at)?, mem::read_u32(m, at + 8)? as u64);
                (num > 0 && num < 256 && mem::plausible(data))
                    .then(|| (0..num).filter_map(|i| mem::read_u32(m, data + i * 8)).collect())
            })
            .unwrap_or_default();
        for &t in &tags {
            self.tag_names.entry(t).or_insert_with(|| n.get(m, t).unwrap_or_default());
        }
        let mut items = Vec::new();
        let mut keys = Vec::new();
        if let Some((at, _)) = n.path(m, comp, &["Rune", "PayloadData", "ItemsToAdd"]) {
            for (label, key) in pointer_array(m, at).into_iter().filter_map(|i| quest_item(m, n, i)) {
                items.push(label);
                keys.extend(key);
            }
        }
        if facts.is_empty() && tags.is_empty() && items.is_empty() {
            return None;
        }
        let rc = mem::read_u64(m, actor + root).filter(|&p| mem::plausible(p))?;
        let used = crate::actors::component(m, n, actor, "InteractionActionComponent")
            .and_then(|c| n.field(m, c, "bHasBeenActivated").filter(|p| p.size == 1).map(|p| c + p.offset as u64));
        let class = n.class(m, actor).unwrap_or_default();
        Some(Payload { facts, tags, used, root: rc, label: items.first().cloned().unwrap_or_else(|| pretty(&class)), items, keys, note: None, gate: gate_of(&class), npc: false })
    }

    /// What an NPC hands out: every payload in its conversations (following topic
    /// subgraphs two deep), and what it gives back for an item it wants.
    fn read_npc(&mut self, m: &dyn Memory, n: &Names, actor: u64, root: u64, flows: &HashMap<u32, u64>) -> Option<Payload> {
        let mut facts: Vec<u64> = Vec::new();
        let mut tags: Vec<u32> = Vec::new();
        let mut assets: Vec<(u64, u32)> = crate::actors::components(m, n, actor, "FlowComponent")
            .into_iter()
            .filter_map(|c| mem::read_u64(m, c + 0x198).filter(|&p| mem::plausible(p)))
            .map(|a| (a, 0))
            .collect();
        let mut seen: Vec<u64> = Vec::new();
        while let Some((asset, depth)) = assets.pop() {
            if seen.contains(&asset) || seen.len() > 64 {
                continue;
            }
            seen.push(asset);
            for node in flow_nodes(m, asset) {
                let Some(class) = n.class(m, node) else { continue };
                match class.as_str() {
                    "FlowNode_Payload" => {
                        let (f, t, _) = payload_data(m, node + 0x1d0);
                        facts.extend(f);
                        tags.extend(t);
                    }
                    // A topic (or a plain subgraph): its asset, named softly — the
                    // FSoftObjectPath's AssetName is 0x10 into the soft pointer.
                    "FlowNode_TopicSubGraph" | "FlowNode_SubGraph" | "FlowNode_SubGraphInstanced" if depth < 2 => {
                        let soft = if class == "FlowNode_TopicSubGraph" { 0x218 } else { 0x1d0 };
                        if let Some(&a) = mem::read_u32(m, node + soft + 0x10).and_then(|i| flows.get(&i)) {
                            assets.push((a, depth + 1));
                        }
                    }
                    _ => {}
                }
            }
        }
        // A hand-over: the item it wants, and what it gives back.
        let mut note = None;
        for comp in crate::actors::components(m, n, actor, "TradeGiveItemRuneComponent") {
            let Some((at, p)) = n.path(m, comp, &["Rune", "ValidTrades"]) else { continue };
            let size = n.inner_of(m, p.field).and_then(|i| mem::read_u32(m, i + n.layout.size)).unwrap_or(0) as u64;
            let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { continue };
            if size < 0xF0 || !mem::plausible(data) || num > 64 {
                continue;
            }
            for i in 0..num as u64 {
                let e = data + i * size;
                let (f, t, _) = payload_data(m, e + 0x18);
                if f.is_empty() && t.is_empty() {
                    continue;
                }
                facts.extend(f);
                tags.extend(t);
                if let Some(item) = mem::read_u64(m, e).filter(|&p| mem::plausible(p)).and_then(|p| n.object(m, p)) {
                    note.get_or_insert(trf!("HAND_OVER", item = item_label(&item)));
                }
            }
        }
        facts.sort_unstable();
        facts.dedup();
        tags.sort_unstable();
        tags.dedup();
        let facts: Vec<(u32, Option<u32>)> =
            facts.into_iter().filter_map(|f| Some((mem::read_u32(m, f + NAME)?, self.quest_of(m, n, f)))).collect();
        for &t in &tags {
            self.tag_names.entry(t).or_insert_with(|| n.get(m, t).unwrap_or_default());
        }
        if facts.is_empty() && tags.is_empty() {
            return None;
        }
        let rc = mem::read_u64(m, actor + root).filter(|&p| mem::plausible(p))?;
        let label = trf!("TALK_GOAL", npc = npc_label(&n.class(m, actor).unwrap_or_default()));
        Some(Payload { facts, tags, used: None, root: rc, label, items: Vec::new(), keys: Vec::new(), note, gate: Gate::Open, npc: true })
    }

    /// Read what the loaded interactables hand out, when a scan is due.
    pub fn refresh(&mut self, m: &dyn Memory, n: &Names, hero: u64, root: u64, actors: u64, flows: &HashMap<u32, u64>) {
        if self.scanned.is_some_and(|t| t.elapsed() < RESCAN) {
            return;
        }
        self.scanned = Some(Instant::now());
        let Some(world) = mem::read_u64(m, hero + OUTER)
            .filter(|&p| mem::plausible(p))
            .and_then(|level| mem::read_u64(m, level + OUTER))
            .filter(|&p| mem::plausible(p))
        else {
            return;
        };
        if self.levels.is_none() {
            self.levels = n.field(m, world, "Levels").map(|p| p.offset as u64);
        }
        let Some(levels) = self.levels else { return };
        let mut now = HashMap::with_capacity(self.payloads.len());
        let mut loaded = std::collections::HashSet::new();
        let mut names = HashMap::new();
        for lv in crate::actors::array(m, world + levels, 4096) {
            for actor in crate::actors::array(m, lv + actors, 500_000) {
                let Some(class) = mem::read_u64(m, actor + CLASS).filter(|&c| mem::plausible(c)) else { continue };
                let interactable = *self.interactable.entry(class).or_insert_with(|| {
                    n.lineage(m, class).into_iter().any(|c| n.object(m, c).as_deref() == Some("InteractableActor"))
                });
                let npc = *self.npc.entry(class).or_insert_with(|| {
                    n.lineage(m, class).into_iter().any(|c| n.object(m, c).as_deref() == Some("NpcActor"))
                });
                if !npc && !interactable {
                    continue;
                }
                let name = self.names.remove(&actor).or_else(|| n.object(m, actor)).unwrap_or_default();
                loaded.insert(name.clone());
                names.insert(actor, name);
                if npc {
                    now.insert(actor, (class, self.read_npc(m, n, actor, root, flows)));
                    continue;
                }
                let entry = match self.payloads.remove(&actor) {
                    Some((c, p)) if c == class => (c, p),
                    _ => (class, self.read(m, n, actor, root)),
                };
                now.insert(actor, entry);
            }
        }
        self.payloads = now;
        self.loaded = loaded;
        self.names = names;
    }

    /// Every place that still holds something new, and what kind, against `k`.
    pub fn evaluate(&mut self, m: &dyn Memory, k: &Knowledge, location: u64) -> Vec<Goal> {
        let mut out = Vec::new();
        if !self.talked_loaded {
            self.talked_loaded = true;
            self.talked = load_talked();
        }
        // Topic unlocks known: a new one may open more talk with someone already talked to.
        let topics = k.tags.iter().filter(|t| self.tag_names.get(t).is_some_and(|n| n.starts_with("Conversation."))).count();
        self.done_npcs = self.talked.iter().filter(|(_, &at)| topics <= at).map(|(n, _)| n.clone()).collect();
        for (&actor, (class, p)) in &self.payloads {
            let Some(p) = p else { continue };
            if mem::read_u64(m, actor + CLASS) != Some(*class) {
                continue;
            }
            if p.npc {
                // A talk that taught something of this NPC's: done with it for now.
                let known = p.facts.iter().filter(|(f, _)| k.facts.contains(f)).count()
                    + p.tags.iter().filter(|t| k.tags.contains(t)).count();
                let name = self.names.get(&actor).cloned().unwrap_or_default();
                if self.heard.insert(actor, known).is_some_and(|before| known > before) && !name.is_empty() {
                    self.talked.insert(name.clone(), topics);
                    save_talked(&self.talked);
                }
                if self.talked.get(&name).is_some_and(|&at| topics <= at) {
                    continue;
                }
            }
            if p.used.is_some_and(|at| {
                let mut b = [0u8];
                m.read(at, &mut b) && b[0] != 0
            }) {
                continue;
            }
            let new_facts: Vec<&(u32, Option<u32>)> = p.facts.iter().filter(|(f, _)| !k.facts.contains(f)).collect();
            let new_tags: Vec<&u32> = p.tags.iter().filter(|t| !k.tags.contains(t)).collect();
            let tag = |t: &u32| self.tag_names.get(t).cloned().unwrap_or_default();
            // Topic unlocks only open more talk: not a reason to go.
            let new_tags: Vec<&u32> = new_tags.into_iter().filter(|t| !tag(t).starts_with("Conversation.")).collect();
            // A quest item still lying there counts as new until it is taken.
            let items_left = !p.items.is_empty() && p.used.is_some();
            let tier = if new_facts.iter().any(|(_, q)| q.is_some_and(|q| k.quests.contains(&q)))
                || !p.keys.is_empty() && items_left
                || new_tags.iter().any(|t| tag(t).starts_with("Quest."))
            {
                Tier::Quest
            } else if items_left || new_tags.iter().any(|t| tag(t).starts_with("Secrets.")) {
                Tier::Secret
            } else if !new_facts.is_empty()
                || new_tags.iter().any(|t| !tag(t).starts_with("Tutorial.") && !tag(t).starts_with("DLC."))
            {
                Tier::Clue
            } else {
                continue;
            };
            let mut b = [0u8; 24];
            if !m.read(p.root + location, &mut b) {
                continue;
            }
            let d = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()) as f32;
            let at = [d(0), d(1), d(2)];
            if !at.iter().all(|v| v.is_finite()) {
                continue;
            }
            let mut detail = p.note.clone().or_else(|| items_left.then(|| trf!("ITEM", items = p.items.join(", ")))).unwrap_or_else(|| {
                new_tags.first().map(|t| tag(t)).unwrap_or_else(|| trf!("NEW_FACTS", count = new_facts.len()))
            });
            let mut quests: Vec<u32> = new_facts.iter().filter_map(|(_, q)| *q).collect();
            quests.sort_unstable();
            quests.dedup();
            let tags = new_tags.iter().map(|t| tag(t)).collect();
            if p.gate == Gate::Conditional {
                detail += tr!("NEEDS_SOMETHING_A_KEY_A_PUZZLE");
            }
            out.push(Goal { tier, id: actor, label: p.label.clone(), detail, at, quests, tags, keys: p.keys.clone(), gate: p.gate });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_payloads_are_told_apart() {
        assert_eq!(gate_of("Cons_Medicine_GatherSingleUse_Interact_BP_C"), Gate::Open);
        assert_eq!(gate_of("ArcasSpireVisited_PayloadInactive_Interact_BP_C"), Gate::Visit);
        assert_eq!(gate_of("Senedra_Timeloop_A_Proximity_PayloadInactive_Interact_BP_C"), Gate::Visit);
        assert_eq!(gate_of("SenedraSmugglersRefugeesMet_PayloadInactive_Interact_BP_C"), Gate::Visit);
        assert_eq!(gate_of("SenedraForestArcasSpireDoorOpening_PayloadInactive_Interact_BP_C"), Gate::Conditional);
        assert_eq!(gate_of("SenedraForestPillarPuzzleComplete_PayloadInactive_Interact_BP_C"), Gate::Conditional);
    }

    #[test]
    fn class_names_read_like_names() {
        assert_eq!(pretty("Cons_MedicineCivilianT01_GatherSingleUse_Interact_BP_C"), "Cons MedicineCivilianT01");
        assert_eq!(
            pretty("SenedraForestPillarPuzzleComplete_PayloadInactive_Interact_BP_C"),
            "SenedraForestPillarPuzzleComplete"
        );
        assert_eq!(pretty("Odd"), "Odd");
    }
}
