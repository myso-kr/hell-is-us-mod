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
    /// Whether `label` is the game's own text (an item, a person, a Datapad entry). If
    /// not it is the trigger's class in words, and a mystery's, good deed's or timeloop's
    /// title may name it better, by its tags (`name_by_secrets`).
    pub named: bool,
    /// What guiding there gives away, beyond the way: a place the player agreed to keep
    /// hidden, or a puzzle's answer (the Settings page's questions; `Consent`).
    pub reveals: Reveal,
    /// Held back by the requirement graph: the goal of its chain's first step, where the
    /// auto guide goes for it (graph.rs `gate`, `flood`; target.rs).
    pub first: Option<u64>,
}

/// What a goal gives away if guided to (`Goal::reveals`): nothing beyond the story's way, a
/// hidden thing's place (needs `Consent::PLACES`), or a puzzle's answer (`Consent::ANSWERS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reveal {
    #[default]
    Nothing,
    Places,
    Answers,
}

/// Name the goals the game's text left unnamed after the good deed, mystery or timeloop
/// whose tag prefix theirs extend (`Secrets.Mystery.TheHermit` names a trigger tagged
/// `Secrets.Mystery.TheHermitCompleted`), the longest such prefix, keeping what the
/// trigger marks after the dot. `secrets` is every one of them, (prefix, title), begun
/// or not: the journal holds only those begun, and most triggers wait on one not begun.
pub fn name_by_secrets(goals: &mut [Goal], secrets: &[(String, String)]) {
    // The goals' own tag sets tie steps to deeds too (an NPC's hand-over that gives a
    // step's tag beside a deed's).
    let groups: Vec<Vec<String>> = goals.iter().map(|g| g.tags.clone()).collect();
    let mut secrets = secrets.to_vec();
    secrets.extend(tags_together(&secrets.clone(), groups.iter().map(Vec::as_slice)));
    for g in goals.iter_mut().filter(|g| !g.named) {
        let deed = secrets
            .iter()
            .filter(|(prefix, _)| g.tags.iter().any(|t| t.starts_with(prefix.as_str())))
            .max_by_key(|(prefix, _)| prefix.len());
        let title = deed.map(|(_, t)| t.as_str()).or_else(|| deed_by_words(g, &secrets));
        if let Some(title) = title {
            let event = g.label.find(" · ").map(|i| g.label[i..].to_string()).unwrap_or_default();
            g.label = format!("{title}{event}");
            g.named = true;
        }
    }
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
    /// Whether `label` is an item's name in the game's text.
    own: bool,
    label: String,
    /// What the trigger's class name says it marks (an i18n key: opened, done…).
    event: Option<&'static str>,
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

/// An actor's name as the survey has it: see `refresh`.
fn live_name(n: &Names, m: &dyn Memory, actor: u64) -> Option<String> {
    let base = n.object(m, actor)?;
    if base.contains("_UAID_") {
        Some(base)
    } else {
        n.object_full(m, actor)
    }
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
    /// The interactables and NPCs loaded now, by name, with where each stands: the survey
    /// leaves these to the live goals. A World Partition name (`_UAID_`) is the same for
    /// every actor one editor session placed, so where it stands tells them apart.
    pub loaded: HashMap<String, Vec<[f32; 3]>>,
    names: HashMap<u64, String>,
    /// A fact asset's investigation, by the fact's address.
    fact_quest: HashMap<u64, Option<u32>>,
    /// Tag names, by index — for the tier rules and for the detail line.
    tag_names: HashMap<u32, String>,
    /// Facts' asset names, by name index: how a place is named in the player's language
    /// (`place_name`).
    fact_names: HashMap<u32, String>,
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

/// A trigger's class as a name a player can read. Places that hand out no item are named
/// only by their class (`AcasaHermitTombFullyOpened_PayloadInactive_Interact_BP_C`), and
/// shown so the list read like code: the words split apart, and what the trigger marks
/// (opened, done, visited…) in the player's language after a dot.
pub fn readable(class: &str) -> String {
    let (name, event) = split_event(class);
    match event {
        Some(key) => format!("{name} · {}", crate::i18n::tr(key)),
        None => name,
    }
}

/// A trigger's class as its place's words and the event its name ends with (an i18n
/// key), if it ends with one of `EVENTS` (the longest that fits).
fn split_event(class: &str) -> (String, Option<&'static str>) {
    let words = split_words(&pretty(class));
    for (tail, key) in EVENTS {
        if words.len() > tail.len() && words[words.len() - tail.len()..].iter().zip(tail.iter()).all(|(w, t)| w == t) {
            return (words[..words.len() - tail.len()].join(" "), Some(key));
        }
    }
    (words.join(" "), None)
}

/// Name what is still unnamed after the place its trigger's name holds: the longest run
/// of its words that is a place the game names (`Universal_Location_…`, the words joined,
/// also without "Of" and "The": `TempleOfTheFallen` is `TempleFallen`), else this region
/// when the name starts with it (`Auriga…` in `AurigaMuseum`). What the trigger marks
/// stays after the dot. `location` and `region` are the lookups (i18n's, in play).
pub fn name_by_place(
    goals: &mut [Goal],
    world: Option<&str>,
    location: impl Fn(&str) -> Option<String>,
    region: impl Fn(&str) -> Option<String>,
) {
    for g in goals.iter_mut().filter(|g| !g.named) {
        let (name, event) = match g.label.find(" · ") {
            Some(i) => (g.label[..i].to_string(), g.label[i..].to_string()),
            None => (g.label.clone(), String::new()),
        };
        // The name's words first, then each tag's segments (`…TempleOfTheFallen.…`).
        let mut sources: Vec<Vec<String>> = vec![name.split_whitespace().map(str::to_string).collect()];
        sources.extend(g.tags.iter().flat_map(|t| t.split('.').map(split_words)));
        let found = sources.iter().find_map(|words| place_in(words, &location)).or_else(|| {
            let world = world?;
            sources[0].iter().any(|w| w.len() >= 4 && world.starts_with(w.as_str())).then(|| region(world)).flatten()
        });
        if let Some(place) = found {
            g.label = format!("{place}{event}");
            g.named = true;
        }
    }
}

/// The longest run of `words` that is a place the game names, also without "Of" and
/// "The".
fn place_in(words: &[String], location: &impl Fn(&str) -> Option<String>) -> Option<String> {
    for len in (1..=words.len()).rev() {
        for span in words.windows(len) {
            let short: Vec<&str> = span.iter().map(String::as_str).filter(|w| *w != "Of" && *w != "The").collect();
            for key in [span.concat(), short.concat()] {
                if let Some(place) = location(&key) {
                    return Some(place);
                }
            }
        }
    }
    None
}

/// Words that say nothing of which deed a tag is about.
const PLAIN: [&str; 14] = [
    "Secrets",
    "Secret",
    "Facts",
    "Mystery",
    "Mysteries",
    "Started",
    "Completed",
    "Complete",
    "Confirmed",
    "Failed",
    "Entered",
    "Visited",
    "Opened",
    "Timeloops",
];

/// The deed a goal's tags and name share a telling word with (five letters or more, not
/// one of `PLAIN`), when exactly one deed shares the most: `Survivor01TradeTooLate` on
/// `Secret_TheSurvivors01DeadConfirmed` is `TaljuSurvivors`'s. The last way to name a
/// trigger after a deed, for a step no payload ties to it.
fn deed_by_words<'a>(g: &Goal, secrets: &'a [(String, String)]) -> Option<&'a str> {
    let telling = |w: &String| w.len() >= 5 && !PLAIN.contains(&w.as_str()) && !w.chars().all(|c| c.is_ascii_digit());
    let mut mine: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in g.tags.iter().filter(|t| t.starts_with("Secrets.")) {
        mine.extend(t.rsplit('.').next().map(split_words).unwrap_or_default());
    }
    if mine.is_empty() {
        return None;
    }
    let name = g.label.split(" · ").next().unwrap_or_default();
    mine.extend(name.split_whitespace().map(str::to_string));
    // Plural or not: `Survivor` meets `Survivors`.
    let stem = |w: &str| w.strip_suffix('s').unwrap_or(w).to_string();
    let mine: std::collections::HashSet<String> = mine.iter().filter(|w| telling(w)).map(|w| stem(w)).collect();
    let mut scored: Vec<(usize, &str)> = secrets
        .iter()
        .map(|(prefix, title)| {
            let words = prefix.rsplit('.').next().map(split_words).unwrap_or_default();
            let shared = words.iter().filter(|w| telling(w) && mine.contains(&stem(w))).count();
            (shared, title.as_str())
        })
        .filter(|(n, _)| *n > 0)
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored.dedup_by(|a, b| a.1 == b.1);
    match scored.as_slice() {
        [best, next, ..] if next.0 == best.0 => None,
        [best, ..] => Some(best.1),
        [] => None,
    }
}

/// Tags handed out together with one of `deeds`' (a prefix and a title), each under that
/// deed's title: a deed's steps, which its row does not name.
pub fn tags_together<'a>(
    deeds: &[(String, String)],
    groups: impl IntoIterator<Item = &'a [String]>,
) -> Vec<(String, String)> {
    let deed_of = |t: &str| {
        deeds.iter().filter(|(p, _)| t.starts_with(p.as_str())).max_by_key(|(p, _)| p.len()).map(|(_, title)| title)
    };
    let mut out = Vec::new();
    for tags in groups {
        let Some(title) = tags.iter().find_map(|t| deed_of(t)) else { continue };
        for t in tags.iter().filter(|t| deed_of(t).is_none() && t.starts_with("Secrets.")) {
            out.push((t.clone(), title.clone()));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// What a trigger's name can end with, longest first, and its i18n key.
const EVENTS: [(&[&str], &str); 20] = [
    (&["Fully", "Opened"], "EVENT_OPENED"),
    (&["All", "Opened"], "EVENT_OPENED"),
    (&["Travel", "Allowed"], "EVENT_TRAVEL"),
    (&["State", "Change"], "EVENT_CHANGED"),
    (&["Opened"], "EVENT_OPENED"),
    (&["Opening"], "EVENT_OPENING"),
    (&["Completion"], "EVENT_DONE"),
    (&["Complete"], "EVENT_DONE"),
    (&["Completed"], "EVENT_DONE"),
    (&["Success"], "EVENT_DONE"),
    (&["Visited"], "EVENT_VISITED"),
    (&["Entered"], "EVENT_ENTERED"),
    (&["Failed"], "EVENT_FAILED"),
    (&["Activated"], "EVENT_ACTIVATED"),
    (&["Reveal"], "EVENT_REVEALED"),
    (&["Confirmed"], "EVENT_CONFIRMED"),
    (&["Discovered"], "EVENT_FOUND"),
    (&["Started"], "EVENT_STARTED"),
    (&["Looted"], "EVENT_LOOTED"),
    (&["Unlocked"], "EVENT_UNLOCKED"),
];

/// Words out of a CamelCase name (spaces already between its parts): a break before a
/// capital after a small letter or a digit, before the last capital of a run followed by
/// a small letter (`ONSoldier`), and before digits after a small letter (`Mausoleum01`,
/// but not `T01`).
fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    for part in s.split_whitespace() {
        let c: Vec<char> = part.chars().collect();
        let mut word = String::new();
        for i in 0..c.len() {
            let (prev, next) = (i.checked_sub(1).map(|j| c[j]), c.get(i + 1).copied());
            let cut = match prev {
                Some(p) => {
                    (p.is_lowercase() && (c[i].is_uppercase() || c[i].is_ascii_digit()))
                        || (p.is_ascii_digit() && c[i].is_uppercase())
                        || (p.is_uppercase() && c[i].is_uppercase() && next.is_some_and(char::is_lowercase))
                }
                None => false,
            };
            if cut && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.push(c[i]);
        }
        if !word.is_empty() {
            words.push(word);
        }
    }
    words
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
    buf.chunks_exact(32)
        .map(|e| u64::from_le_bytes(e[16..24].try_into().unwrap()))
        .filter(|&p| mem::plausible(p))
        .collect()
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
        for &(f, _) in &facts {
            self.fact_names.entry(f).or_insert_with(|| n.get(m, f).unwrap_or_default());
        }
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
        // Items outside the quests' and secrets' (a Lymbic rod): not a reason to go, but
        // their name in the game's language is a better label than the trigger's class.
        let mut other = None;
        if let Some((at, _)) = n.path(m, comp, &["Rune", "PayloadData", "ItemsToAdd"]) {
            for i in pointer_array(m, at) {
                match quest_item(m, n, i) {
                    Some((label, key)) => {
                        items.push(label);
                        keys.extend(key);
                    }
                    None => other = other.or_else(|| n.object(m, i).and_then(|name| crate::i18n::item(&name))),
                }
            }
        }
        if facts.is_empty() && tags.is_empty() && items.is_empty() {
            return None;
        }
        let rc = mem::read_u64(m, actor + root).filter(|&p| mem::plausible(p))?;
        let used = crate::actors::component(m, n, actor, "InteractionActionComponent")
            .and_then(|c| n.field(m, c, "bHasBeenActivated").filter(|p| p.size == 1).map(|p| c + p.offset as u64));
        let class = n.class(m, actor).unwrap_or_default();
        Some(Payload {
            facts,
            tags,
            used,
            root: rc,
            own: !items.is_empty() || other.is_some(),
            label: items.first().cloned().or(other).unwrap_or_else(|| readable(&class)),
            event: split_event(&class).1,
            items,
            keys,
            note: None,
            gate: gate_of(&class),
            npc: false,
        })
    }

    /// What an NPC hands out: every payload in its conversations (following topic
    /// subgraphs two deep), and what it gives back for an item it wants.
    fn read_npc(
        &mut self,
        m: &dyn Memory,
        n: &Names,
        actor: u64,
        root: u64,
        flows: &HashMap<u32, u64>,
    ) -> Option<Payload> {
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
        Some(Payload {
            facts,
            tags,
            used: None,
            root: rc,
            label,
            items: Vec::new(),
            keys: Vec::new(),
            note,
            gate: Gate::Open,
            npc: true,
            event: None,
            own: true,
        })
    }

    /// What a place is called, and whether that is the game's own text (`Goal::named`), in
    /// the player's language where the game has it: an item it
    /// hands out by the item's name, a person by theirs, and a trigger by the Datapad entry
    /// its facts are about (the one most of them are about) with what it marks after a dot.
    /// The entry's name is the one the hero knows (`i18n::subject`), so a real name is not
    /// given away early. Without the game's text, the trigger's class in words.
    fn place_name(&self, p: &Payload) -> (String, bool) {
        if p.own || p.npc {
            return (p.label.clone(), true);
        }
        let mut units: HashMap<String, usize> = HashMap::new();
        for (f, _) in &p.facts {
            if let Some(fact) = self.fact_names.get(f).and_then(|name| crate::i18n::fact(name)) {
                *units.entry(fact.unit).or_default() += 1;
            }
        }
        let unit = units.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(u, _)| u);
        match unit.and_then(|u| crate::i18n::subject(&u)) {
            Some(name) => match p.event {
                Some(key) => (format!("{name} · {}", crate::i18n::tr(key)), true),
                None => (name, true),
            },
            None => (p.label.clone(), false),
        }
    }

    /// Read what the loaded interactables hand out, when a scan is due.
    #[allow(clippy::too_many_arguments)]
    /// `actors`: every actor of the loaded levels and its class, as the scanner last walked
    /// them (actors.rs `all_actors`) — one walk for both.
    pub fn refresh(
        &mut self,
        m: &dyn Memory,
        n: &Names,
        root: u64,
        location: u64,
        actors: &[(u64, u64)],
        flows: &HashMap<u32, u64>,
    ) {
        if self.scanned.is_some_and(|t| t.elapsed() < RESCAN) || actors.is_empty() {
            return;
        }
        self.scanned = Some(Instant::now());
        let mut now = HashMap::with_capacity(self.payloads.len());
        let mut loaded: HashMap<String, Vec<[f32; 3]>> = HashMap::new();
        let mut names = HashMap::new();
        for &(actor, class) in actors {
            {
                let interactable = *self.interactable.entry(class).or_insert_with(|| {
                    n.lineage(m, class).into_iter().any(|c| n.object(m, c).as_deref() == Some("InteractableActor"))
                });
                let npc = *self.npc.entry(class).or_insert_with(|| {
                    n.lineage(m, class).into_iter().any(|c| n.object(m, c).as_deref() == Some("NpcActor"))
                });
                if !npc && !interactable {
                    continue;
                }
                // Named as the survey names it (survey.rs `runtime_name`): a placed actor
                // with its number (`Convo_Victor_Crafting_BP_2`; without it a loaded one went
                // unrecognised and was shown twice), a World Partition one (`_UAID_`) without.
                let name = self.names.remove(&actor).or_else(|| live_name(n, m, actor)).unwrap_or_default();
                let at = mem::read_u64(m, actor + root)
                    .filter(|&p| mem::plausible(p))
                    .and_then(|rc| {
                        let mut b = [0u8; 24];
                        m.read(rc + location, &mut b).then_some(b)
                    })
                    .map(|b| {
                        let d = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()) as f32;
                        [d(0), d(1), d(2)]
                    })
                    .filter(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1.0e7));
                let places = loaded.entry(name.clone()).or_default();
                places.extend(at);
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
        // What was heard of an actor no longer in play goes with it: the map grew by every
        // NPC ever seen. One seen again starts over, which only waits for its next talk.
        let payloads = &self.payloads;
        self.heard.retain(|a, _| payloads.contains_key(a));
        if !self.talked_loaded {
            self.talked_loaded = true;
            self.talked = load_talked();
        }
        // Topic unlocks known: a new one may open more talk with someone already talked to.
        let topics =
            k.tags.iter().filter(|t| self.tag_names.get(t).is_some_and(|n| n.starts_with("Conversation."))).count();
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
            let mut detail =
                p.note.clone().or_else(|| items_left.then(|| trf!("ITEM", items = p.items.join(", ")))).unwrap_or_else(
                    || new_tags.first().map(|t| tag(t)).unwrap_or_else(|| trf!("NEW_FACTS", count = new_facts.len())),
                );
            let mut quests: Vec<u32> = new_facts.iter().filter_map(|(_, q)| *q).collect();
            quests.sort_unstable();
            quests.dedup();
            let tags = new_tags.iter().map(|t| tag(t)).collect();
            if p.gate == Gate::Conditional {
                detail += tr!("NEEDS_SOMETHING_A_KEY_A_PUZZLE");
            }
            let (label, named) = self.place_name(p);
            out.push(Goal {
                tier,
                id: actor,
                label: label.clone(),
                named,
                detail,
                at,
                quests,
                tags,
                keys: p.keys.clone(),
                gate: p.gate,
                // A story goal shows the way; a secret's or a clue's place is a hidden thing's.
                reveals: if tier == Tier::Quest { Reveal::Nothing } else { Reveal::Places },
                first: None,
            });
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
    fn secrets_name_what_the_game_text_did_not() {
        let deed = |name: &str, tags: &str| (tags.to_string(), name.to_string());
        let secrets = [deed("은둔자", "Secrets.Mystery.TheHermit"), deed("다른 것", "Secrets.Mystery.The")];
        let goal = |label: &str, named: bool| Goal {
            tier: Tier::Secret,
            id: 1,
            label: label.into(),
            detail: String::new(),
            at: [0.0; 3],
            quests: Vec::new(),
            tags: vec!["Secrets.Mystery.TheHermitCompleted".into()],
            keys: Vec::new(),
            gate: Gate::Open,
            named,
            reveals: Reveal::Nothing,
            first: None,
        };
        let mut goals = [goal("Acasa Hermit Tomb · opened", false), goal("Hermit's Key", true)];
        name_by_secrets(&mut goals, &secrets);
        // The longest prefix wins; what the trigger marks stays.
        assert_eq!(goals[0].label, "은둔자 · opened");
        // The game's own name is left alone.
        assert_eq!(goals[1].label, "Hermit's Key");
    }

    #[test]
    fn a_trigger_is_named_after_the_place_its_name_holds() {
        let goal = |class: &str| Goal {
            tier: Tier::Quest,
            id: 1,
            label: readable(class),
            detail: String::new(),
            at: [0.0; 3],
            quests: Vec::new(),
            tags: Vec::new(),
            keys: Vec::new(),
            gate: Gate::Open,
            named: false,
            reveals: Default::default(),
            first: None,
        };
        let places = |k: &str| match k {
            "ArcasSpire" => Some("아르카스 첨탑".to_string()),
            "TempleFallen" => Some("타락자의 신전".to_string()),
            _ => None,
        };
        let mut goals = [
            goal("ArcasSpireLeftCage01Complete_PayloadInactive_Interact_BP_C"),
            goal("TempleOfTheFallenVisited_PayloadInactive_Interact_BP_C"),
            goal("AurigaGeneratorActivated_PayloadInactive_Interact_BP_C"),
            goal("SomethingElse_PayloadInactive_Interact_BP_C"),
        ];
        name_by_place(&mut goals, Some("AurigaMuseum"), places, |_| Some("아우리가 박물관".to_string()));
        // A place in a tag, and the region by a word that is not the first.
        let mut more = [
            goal("TemplePuzzleDoor_PayloadInactive_Interact_BP_C"),
            goal("UniqueAuriga_FloorB1_PayloadInactive_Interact_BP_C"),
        ];
        more[0].tags = vec!["Interactable.Event.TempleOfTheFallen.PressurePlateFirstDoorOpened".into()];
        name_by_place(&mut more, Some("AurigaMuseum"), places, |_| Some("아우리가 박물관".to_string()));
        assert_eq!(more[0].label, "타락자의 신전");
        assert_eq!(more[1].label, "아우리가 박물관");
        assert!(goals[0].label.starts_with("아르카스 첨탑 · "), "{}", goals[0].label);
        assert!(goals[1].label.starts_with("타락자의 신전 · "), "{}", goals[1].label);
        assert!(goals[2].label.starts_with("아우리가 박물관 · "), "{}", goals[2].label);
        assert!(!goals[3].named);
    }

    #[test]
    fn a_step_takes_the_title_of_the_deed_it_comes_with() {
        let deeds = [("Secrets.Facts.ClassroomPicture".to_string(), "이웃을 사랑하라".to_string())];
        // Thomas's hand-over gives a step's tag beside the deed's.
        let handover =
            ["Secrets.Facts.PhotoAcquired".to_string(), "Secrets.Facts.ClassroomPictureObtained".to_string()];
        let steps = tags_together(&deeds, [&handover[..]]);
        assert_eq!(steps, [("Secrets.Facts.PhotoAcquired".to_string(), "이웃을 사랑하라".to_string())]);
        let mut secrets = deeds.to_vec();
        secrets.extend(steps);
        let mut goals = [Goal {
            tier: Tier::Secret,
            id: 1,
            label: readable("AcasaMarshesThomasStateChange_PayloadInactive_Interact_BP_C"),
            detail: String::new(),
            at: [0.0; 3],
            quests: Vec::new(),
            tags: vec!["Secrets.Facts.PhotoAcquiredThomasStateChange".into()],
            keys: Vec::new(),
            gate: Gate::Open,
            named: false,
            reveals: Default::default(),
            first: None,
        }];
        name_by_secrets(&mut goals, &secrets);
        assert!(goals[0].label.starts_with("이웃을 사랑하라 · "), "{}", goals[0].label);
    }

    #[test]
    fn a_step_no_payload_ties_takes_the_deed_it_shares_a_word_with() {
        let secrets = [
            ("Secrets.Facts.TaljuSurvivors".to_string(), "어둠 속의 빛".to_string()),
            ("Secrets.Facts.RedShoes".to_string(), "새 신발".to_string()),
        ];
        let mut goals = [Goal {
            tier: Tier::Secret,
            id: 1,
            label: readable("Secret_TheSurvivors01DeadConfirmed_PayloadInactive_Interact_BP_C"),
            detail: String::new(),
            at: [0.0; 3],
            quests: Vec::new(),
            tags: vec!["Secrets.Facts.Survivor01TradeTooLate".into()],
            keys: Vec::new(),
            gate: Gate::Open,
            named: false,
            reveals: Default::default(),
            first: None,
        }];
        name_by_secrets(&mut goals, &secrets);
        assert!(goals[0].label.starts_with("어둠 속의 빛"), "{}", goals[0].label);
    }

    #[test]
    fn trigger_names_are_split_and_their_event_named() {
        let name = |c: &str| readable(c).split(" · ").next().unwrap().to_string();
        assert_eq!(name("AcasaHermitTombFullyOpened_PayloadInactive_Interact_BP_C"), "Acasa Hermit Tomb");
        assert_eq!(name("CaptainVaasOfficeCompletion_PayloadInactive_Interact_BP_C"), "Captain Vaas Office");
        assert_eq!(name("LetheHidingONSoldierDiscovered_PayloadInactive_Interact_BP_C"), "Lethe Hiding ON Soldier");
        assert_eq!(name("JeljinMausoleum01Completed_PayloadInactive_Interact_BP_C"), "Jeljin Mausoleum 01");
        assert!(readable("TrainingRoomCompletion_PayloadInactive_Interact_BP_C").contains(" · "));
        // No event at the end: the words only.
        assert_eq!(readable("BloodQueenTombFrescoes_PayloadInactive_Interact_BP_C"), "Blood Queen Tomb Frescoes");
        assert_eq!(readable("Cons_MedicineCivilianT01_GatherSingleUse_Interact_BP_C"), "Cons Medicine Civilian T01");
        // An event alone is not a name.
        assert_eq!(
            readable("DoorOpened_PayloadInactive_Interact_BP_C"),
            "Door · ".to_string() + crate::i18n::tr("EVENT_OPENED")
        );
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
