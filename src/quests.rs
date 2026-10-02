//! The quest journal, read from the game: the main story's investigations and the
//! good deeds, with their names and descriptions as the game shows them
//! (.spec/QUESTS.md).
//!
//! Main quests are `QuestData` assets (Quest01–06). Every fact that belongs to one
//! names it in `AssociatedQuestData`; their `Track` groups them by subject and their
//! `Priority` orders a track (a potential link, then the confirmed one). A quest's
//! name and description are its `Name` and `Desc…` string facts; its state is its
//! start and completion status facts; its leads are the tracks the hero has begun
//! but not finished.
//!
//! Good deeds are `SecretsSubsystem.GoodDeeds` (always loaded): GUID, tags and a
//! `Title`. The save keeps each deed's state by GUID. A title the game has not shown
//! yet is still a reference into the string table `UI_Secrets_ST`, read here for its
//! English source; once shown, its localised text is read and kept in `quests.txt`.
//!
//! Text is `FText`, which reflection does not open. Its display string was found on
//! 24045435: TextData → +0x10 → +0x30 → +0x08 an FString (the localised text), with
//! TextData → +0x10 → +0x20 → +0x10 the source text behind it.

use crate::knowledge::Knowledge;
use crate::mem::{self, Memory};
use crate::names::{Names, CLASS};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// An FString's text: wide characters, count with the terminator.
fn fstring(m: &dyn Memory, at: u64) -> Option<String> {
    let (data, num) = (mem::read_u64(m, at)?, mem::read_u32(m, at + 8)?);
    if !mem::plausible(data) || !(2..=4096).contains(&num) {
        return None;
    }
    let mut b = vec![0u8; num as usize * 2];
    m.read(data, &mut b).then_some(())?;
    let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    (w[w.len() - 1] == 0).then(|| String::from_utf16_lossy(&w[..w.len() - 1]))
}

/// An `FText`'s display string: the localised text, or the source text if that is all
/// there is.
pub fn ftext(m: &dyn Memory, at: u64) -> Option<String> {
    let data = mem::read_u64(m, at).filter(|&p| mem::plausible(p))?;
    let history = mem::read_u64(m, data + 0x10).filter(|&p| mem::plausible(p))?;
    if let Some(s) = mem::read_u64(m, history + 0x30).filter(|&p| mem::plausible(p)).and_then(|d| fstring(m, d + 8)) {
        return Some(s);
    }
    let source = mem::read_u64(m, history + 0x20).filter(|&p| mem::plausible(p))?;
    fstring(m, source + 0x10)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    NotStarted,
    Started,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The main story's investigation, by its number (1–6).
    Main(u32),
    GoodDeed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quest {
    /// Stable across runs: `Quest01`, or a good deed's GUID in hex.
    pub key: String,
    pub kind: Kind,
    pub name: String,
    pub detail: String,
    pub status: Status,
    /// Facts known, of those the quest has (main quests).
    pub progress: Option<(usize, usize)>,
    /// What is under way: subjects begun but not finished — the subject, and what
    /// the hero last learned about it.
    pub leads: Vec<(String, Option<String>)>,
    /// The QuestData's name index — what a goal's facts point at (main quests).
    pub quest: Option<u32>,
    /// The prefix a good deed's gameplay tags share — what a goal's tags start with.
    pub tags: Option<String>,
}

impl Quest {
    pub fn active(&self) -> bool {
        self.status == Status::Started
    }
}

/// One fact of a main quest, as found.
#[derive(Clone, Debug)]
struct Fact {
    name: u32,
    object: String,
    track: String,
    /// Its `Description`, if it has one.
    text: Option<String>,
}

/// A main quest's asset and its facts.
#[derive(Clone, Debug)]
struct Main {
    number: u32,
    key: String,
    name: u32,
    facts: Vec<Fact>,
}

/// A good deed as the data table gives it.
#[derive(Clone, Debug, PartialEq)]
struct Deed {
    title: String,
    tags: String,
    /// Where it happens: its `LocationNameFact`'s text.
    place: String,
}

/// Tracks that are about the quest itself, not a lead.
const OWN_TRACKS: [&str; 4] = ["Name", "Portrait", "Identity Type", "Quest Status"];

/// What a class's objects are to the journal.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Quest,
    /// SecretsSubsystem: the good deeds.
    Secrets,
    /// A StringTable: `UI_Secrets_ST` holds the deeds' titles.
    Strings,
    /// A FlowAsset (a conversation, a topic): goals.rs follows NPCs into them.
    Flow,
    Fact,
    Other,
}

/// Objects looked at per step: a pass is ~55 steps (~350k objects).
const SLICE: usize = 4000;

#[derive(Default)]
pub struct Quests {
    mains: Vec<Main>,
    /// The pass under way: every object, how far it got, and what it found so far.
    pending: Vec<u64>,
    cursor: usize,
    found: Vec<u64>,
    by_quest: BTreeMap<u64, Vec<Fact>>,
    secrets: Option<u64>,
    /// Loaded flow assets by name index: this pass's, and the last complete one's.
    flows_found: HashMap<u32, u64>,
    flows: HashMap<u32, u64>,
    /// `UI_Secrets_ST`: source strings by text key index.
    strings: HashMap<u32, String>,
    roles: HashMap<u64, Role>,
    /// Good deeds by GUID: from the table when it was loaded, and from `quests.txt`.
    deeds: BTreeMap<[u8; 16], Deed>,
    /// Main quest names and descriptions once read: assets stream out.
    seen: HashMap<String, (String, String)>,
    loaded: bool,
}

pub fn cache_path() -> PathBuf {
    crate::paths::data_dir().join("quests.txt")
}

fn hex(g: &[u8; 16]) -> String {
    g.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<[u8; 16]> {
    let v: Vec<u8> = (0..s.len()).step_by(2).filter_map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect();
    v.try_into().ok()
}

/// `quests.txt`: one line each — `deed <guid> <tags> <title>` or
/// `main <key> <name>\t<description>`, tab-separated.
fn render(deeds: &BTreeMap<[u8; 16], Deed>, seen: &HashMap<String, (String, String)>) -> String {
    let mut out = String::new();
    for (g, d) in deeds {
        out += &format!("deed\t{}\t{}\t{}\t{}\n", hex(g), d.tags, d.title.replace(['\t', '\n'], " "), d.place.replace(['\t', '\n'], " "));
    }
    let mut mains: Vec<_> = seen.iter().collect();
    mains.sort();
    for (k, (name, desc)) in mains {
        out += &format!("main\t{k}\t{}\t{}\n", name.replace(['\t', '\n'], " "), desc.replace(['\t', '\n'], " "));
    }
    out
}

/// Good deeds by GUID, and main quests' (name, description) by key.
type Cache = (BTreeMap<[u8; 16], Deed>, HashMap<String, (String, String)>);

fn parse(text: &str) -> Cache {
    let (mut deeds, mut seen) = (BTreeMap::new(), HashMap::new());
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[..] {
            ["deed", g, tags, title, ref place @ ..] => {
                if let Some(g) = unhex(g) {
                    let place = place.first().unwrap_or(&"").to_string();
                    deeds.insert(g, Deed { title: title.to_string(), tags: tags.to_string(), place });
                }
            }
            ["main", k, name, desc] => {
                seen.insert(k.to_string(), (name.to_string(), desc.to_string()));
            }
            _ => {}
        }
    }
    (deeds, seen)
}

/// "Quest01_Tania" → "Tania"; "Quest01_HeroFamilyHome" → "Hero Family Home".
fn pretty(track: &str) -> String {
    let t = track.split_once('_').map_or(track, |(_, rest)| rest);
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && c.is_uppercase() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(if c == '_' { ' ' } else { c });
    }
    out
}

impl Quests {
    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if let Ok(text) = std::fs::read_to_string(cache_path()) {
            (self.deeds, self.seen) = parse(&text);
        }
    }

    fn save(&self) {
        let path = cache_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, render(&self.deeds, &self.seen));
    }

    /// The loaded flow assets (conversations, topics) by name index, as of the last
    /// complete pass — how a topic a conversation names softly is found.
    pub fn flows(&self) -> &HashMap<u32, u64> {
        &self.flows
    }

    /// Whether the journal has been put together at least once.
    pub fn ready(&self) -> bool {
        !self.mains.is_empty()
    }

    /// A slice of the walk over every object that finds the main quests and their
    /// facts, and the good deeds' table if it is loaded. A pass ends with the journal
    /// rebuilt and the next one starts over; `objects` is asked for only then.
    pub fn step(&mut self, m: &dyn Memory, n: &Names, objects: impl FnOnce() -> Vec<u64>) {
        self.load();
        if self.cursor >= self.pending.len() {
            if !self.pending.is_empty() {
                self.finish(m, n);
            }
            self.pending = objects();
            self.cursor = 0;
        }
        let end = (self.cursor + SLICE).min(self.pending.len());
        for i in self.cursor..end {
            let o = self.pending[i];
            let Some(class) = mem::read_u64(m, o + CLASS).filter(|&c| mem::plausible(c)) else { continue };
            let role = *self.roles.entry(class).or_insert_with(|| match n.object(m, class).as_deref() {
                Some("QuestData") => Role::Quest,
                Some("SecretsSubsystem") => Role::Secrets,
                Some("StringTable") => Role::Strings,
                _ if n.is_a(m, o, "FlowAsset") => Role::Flow,
                _ if n.is_a(m, o, "FactData") => Role::Fact,
                _ => Role::Other,
            });
            match role {
                Role::Other => {}
                Role::Quest => {
                    if !n.object(m, o).unwrap_or_default().starts_with("Default__") {
                        self.found.push(o);
                    }
                }
                Role::Secrets => {
                    if !n.object(m, o).unwrap_or_default().starts_with("Default__") {
                        self.secrets = Some(o);
                    }
                }
                Role::Strings => {
                    if n.object(m, o).as_deref() == Some("UI_Secrets_ST") {
                        self.strings = string_table(m, o);
                    }
                }
                Role::Fact => self.fact(m, n, o),
                Role::Flow => {
                    if let Some(i) = mem::read_u32(m, o + crate::names::NAME) {
                        self.flows_found.insert(i, o);
                    }
                }
            }
        }
        self.cursor = end;
    }

    fn fact(&mut self, m: &dyn Memory, n: &Names, o: u64) {
        {
            let Ok(q) = n.follow(m, o, "AssociatedQuestData") else { return };
            let object = n.object(m, o).unwrap_or_default();
            let track = n
                .field(m, o, "Track")
                .and_then(|p| mem::read_u32(m, o + p.offset as u64))
                .and_then(|i| n.get(m, i))
                .unwrap_or_default();
            let text = n.field(m, o, "Description").and_then(|p| ftext(m, o + p.offset as u64)).filter(|t| !t.trim().is_empty());
            let name = mem::read_u32(m, o + crate::names::NAME).unwrap_or(0);
            self.by_quest.entry(q).or_default().push(Fact { name, object, track, text });
        }
    }

    fn finish(&mut self, m: &dyn Memory, n: &Names) {
        let mut by_quest = std::mem::take(&mut self.by_quest);
        let mut changed = false;
        self.flows = std::mem::take(&mut self.flows_found);
        if let Some(s) = self.secrets.take() {
            changed |= self.read_deeds(m, n, s);
        }
        self.mains = std::mem::take(&mut self.found)
            .into_iter()
            .filter_map(|q| {
                let key = n.object(m, q)?.trim_end_matches("_DA").to_string();
                let number = key.strip_prefix("Quest")?.parse().ok()?;
                Some(Main {
                    number,
                    key,
                    name: mem::read_u32(m, q + crate::names::NAME)?,
                    facts: by_quest.remove(&q).unwrap_or_default(),
                })
            })
            .collect();
        self.mains.sort_by_key(|q| q.number);
        // Names and descriptions as the game has them now, kept for when they stream out.
        for q in &self.mains {
            let text = |f: &Fact| f.text.clone();
            let name = q.facts.iter().find(|f| f.track == "Name").and_then(text);
            let desc = q.facts.iter().filter(|f| f.track.starts_with("Desc")).find_map(text);
            if let Some(name) = name {
                let entry = (name, desc.unwrap_or_default());
                if self.seen.get(&q.key) != Some(&entry) {
                    self.seen.insert(q.key.clone(), entry);
                    changed = true;
                }
            }
        }
        if changed {
            self.save();
        }
    }

    /// The good deeds: `SecretsSubsystem.GoodDeeds`, each a GoodDeedData — Guid at
    /// +0x18, StartedTag +0x28, then `Title`. A title the game has shown reads as
    /// itself (localised); one it has not is looked up in `UI_Secrets_ST` (English),
    /// and never replaces a localised one already kept. Whether anything changed.
    fn read_deeds(&mut self, m: &dyn Memory, n: &Names, secrets: u64) -> bool {
        let Some((at, p)) = n.path(m, secrets, &["GoodDeeds"]) else { return false };
        let size = n.inner_of(m, p.field).and_then(|i| mem::read_u32(m, i + n.layout.size)).unwrap_or(0) as u64;
        let inner = n.inner_of(m, p.field).and_then(|inner| n.struct_of(m, inner));
        let title_at = inner.and_then(|st| n.find(m, st, "Title")).map(|f| f.offset as u64);
        let place_at = inner.and_then(|st| n.find(m, st, "LocationNameFact")).map(|f| f.offset as u64);
        let (Some(data), Some(num), Some(title_at)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8), title_at) else {
            return false;
        };
        if size < 0x48 || !mem::plausible(data) || num > 500 {
            return false;
        }
        let mut changed = false;
        for i in 0..num as u64 {
            let e = data + i * size;
            let mut g = [0u8; 16];
            if !m.read(e + 0x18, &mut g) || g == [0; 16] {
                continue;
            }
            let tag = mem::read_u32(m, e + 0x28).and_then(|i| n.get(m, i)).unwrap_or_default();
            let tags = tag.strip_suffix("Started").unwrap_or(&tag).to_string();
            let (title, local) = match ftext(m, e + title_at) {
                Some(t) => (t, true),
                None => match text_key(m, e + title_at).and_then(|k| self.strings.get(&k)) {
                    Some(t) => (t.clone(), false),
                    None => continue,
                },
            };
            // The place: a string fact's Description, localised once the game showed it.
            let place = place_at
                .and_then(|o| mem::read_u64(m, e + o))
                .filter(|&f| mem::plausible(f))
                .and_then(|f| {
                    let d = n.field(m, f, "Description")?;
                    let at = f + d.offset as u64;
                    ftext(m, at).or_else(|| text_key(m, at).and_then(|k| self.strings.get(&k).cloned()))
                })
                .unwrap_or_default();
            let old = self.deeds.get(&g);
            let place = if place.is_empty() { old.map(|d| d.place.clone()).unwrap_or_default() } else { place };
            let deed = Deed { title, tags, place };
            let keep = old.is_some_and(|d| d == &deed || (!local && d.tags == deed.tags && !d.place.is_empty()));
            if !keep {
                self.deeds.insert(g, deed);
                changed = true;
            }
        }
        changed
    }

    /// The journal against what the hero knows, and the save's good-deed states
    /// (GUID, state: 1 started, 2 completed, 3 rewarded, 4 failed).
    pub fn journal(&self, k: &Knowledge, deeds: &[([u8; 16], u8)]) -> Vec<Quest> {
        let mut out = Vec::new();
        for q in &self.mains {
            let known = |f: &Fact| k.facts.contains(&f.name);
            let status_fact = |part: &str| q.facts.iter().any(|f| f.track == "Quest Status" && f.object.contains(part) && known(f));
            let status = if status_fact("Complete") {
                Status::Completed
            } else if status_fact("Start") || q.facts.iter().any(known) {
                Status::Started
            } else {
                Status::NotStarted
            };
            let (name, detail) = self.seen.get(&q.key).cloned().unwrap_or_else(|| (format!("메인 퀘스트 {}", q.number), String::new()));
            // Per subject: facts known, facts in all, the text of the last known one.
            let mut tracks: BTreeMap<&str, (usize, usize, Option<&String>)> = BTreeMap::new();
            for f in q.facts.iter().filter(|f| !OWN_TRACKS.contains(&f.track.as_str()) && !f.track.starts_with("Desc")) {
                let e = tracks.entry(&f.track).or_default();
                e.1 += 1;
                if known(f) {
                    e.0 += 1;
                    e.2 = f.text.as_ref().or(e.2);
                }
            }
            let leads = tracks
                .iter()
                .filter(|(_, (k, all, _))| *k > 0 && k < all)
                .map(|(t, (_, _, text))| (pretty(t), text.cloned()))
                .collect();
            let total = q.facts.len();
            let got = q.facts.iter().filter(|f| known(f)).count();
            out.push(Quest {
                key: q.key.clone(),
                kind: Kind::Main(q.number),
                name,
                detail,
                status,
                progress: Some((got, total)),
                leads,
                quest: Some(q.name),
                tags: None,
            });
        }
        for (guid, state) in deeds {
            let status = match state {
                1 => Status::Started,
                2 | 3 => Status::Completed,
                4 => Status::Failed,
                _ => continue,
            };
            let deed = self.deeds.get(guid);
            out.push(Quest {
                key: hex(guid),
                kind: Kind::GoodDeed,
                name: deed.map_or_else(|| format!("선행 {}", &hex(guid)[..4]), |d| d.title.clone()),
                detail: deed.filter(|d| !d.place.is_empty()).map(|d| format!("장소: {}", d.place)).unwrap_or_default(),
                status,
                progress: None,
                leads: Vec::new(),
                quest: None,
                tags: deed.map(|d| d.tags.clone()).filter(|t| !t.is_empty()),
            });
        }
        out
    }
}

/// A string-table reference's key: an `FText` whose history is
/// FTextHistory_StringTableEntry — TableId FName at history +0x10, the key's index
/// in the text key pool at +0x18.
fn text_key(m: &dyn Memory, at: u64) -> Option<u32> {
    let data = mem::read_u64(m, at).filter(|&p| mem::plausible(p))?;
    let history = mem::read_u64(m, data + 0x10).filter(|&p| mem::plausible(p))?;
    mem::read_u32(m, history + 0x18)
}

/// A `UStringTable`'s entries: its FStringTable (shared pointer at +0x28) keeps
/// KeysToEntries at +0x20 — elements of 32 bytes: key index (u32), entry pointer at
/// +8 — and each entry its source FString at +0x10.
fn string_table(m: &dyn Memory, table: u64) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let Some(t) = mem::read_u64(m, table + 0x28).filter(|&p| mem::plausible(p)) else { return out };
    let (Some(data), Some(num)) = (mem::read_u64(m, t + 0x20), mem::read_u32(m, t + 0x28)) else { return out };
    if !mem::plausible(data) || num > 5000 {
        return out;
    }
    for i in 0..num as u64 {
        let e = data + i * 32;
        let (Some(key), Some(entry)) = (mem::read_u32(m, e), mem::read_u64(m, e + 8)) else { continue };
        if let Some(s) = mem::plausible(entry).then(|| fstring(m, entry + 0x10)).flatten() {
            out.insert(key, s);
        }
    }
    out
}

/// The save's good deeds: (GUID, state) — CharlieSaveGame.Player.SecretsState.GoodDeeds.
pub fn deed_states(m: &dyn Memory, n: &Names, save: u64) -> Vec<([u8; 16], u8)> {
    let Some((at, p)) = n.path(m, save, &["Player", "SecretsState", "GoodDeeds"]) else { return Vec::new() };
    let size = n.inner_of(m, p.field).and_then(|i| mem::read_u32(m, i + n.layout.size)).unwrap_or(0) as u64;
    let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return Vec::new() };
    if size < 0x12 || !mem::plausible(data) || num > 500 {
        return Vec::new();
    }
    (0..num as u64)
        .filter_map(|i| {
            let mut b = [0u8; 0x12];
            m.read(data + i * size, &mut b).then(|| (b[..16].try_into().unwrap(), b[0x11]))
        })
        .collect()
}

/// The quest the guide follows: the one chosen, if it is still under way — else the
/// main story's: the lowest-numbered main quest under way, or the first not started.
pub fn followed<'a>(journal: &'a [Quest], chosen: Option<&str>) -> Option<&'a Quest> {
    if let Some(q) = chosen.and_then(|c| journal.iter().find(|q| q.key == c && q.active())) {
        return Some(q);
    }
    let mut mains: Vec<&Quest> = journal.iter().filter(|q| matches!(q.kind, Kind::Main(_))).collect();
    mains.sort_by_key(|q| match q.kind {
        Kind::Main(n) => n,
        Kind::GoodDeed => u32::MAX,
    });
    mains.iter().find(|q| q.active()).or_else(|| mains.iter().find(|q| q.status == Status::NotStarted)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn main_quest(n: u32, status: Status) -> Quest {
        Quest {
            key: format!("Quest0{n}"),
            kind: Kind::Main(n),
            name: String::new(),
            detail: String::new(),
            status,
            progress: None,
            leads: vec![],
            quest: None,
            tags: None,
        }
    }

    #[test]
    fn the_main_story_follows_the_lowest_quest_under_way() {
        let j = vec![main_quest(2, Status::Started), main_quest(1, Status::Completed), main_quest(3, Status::NotStarted)];
        assert_eq!(followed(&j, None).unwrap().key, "Quest02");
        let j = vec![main_quest(1, Status::Completed), main_quest(2, Status::NotStarted)];
        assert_eq!(followed(&j, None).unwrap().key, "Quest02", "the next one when none is under way");
        let mut deed = main_quest(9, Status::Started);
        deed.kind = Kind::GoodDeed;
        deed.key = "abc".into();
        let j = vec![main_quest(1, Status::Started), deed];
        assert_eq!(followed(&j, Some("abc")).unwrap().key, "abc", "a chosen quest under way wins");
        assert_eq!(followed(&j, Some("gone")).unwrap().key, "Quest01");
    }

    #[test]
    fn tracks_read_as_names() {
        assert_eq!(pretty("Quest01_Tania"), "Tania");
        assert_eq!(pretty("Quest01_HeroFamilyHome"), "Hero Family Home");
    }

    #[test]
    fn the_cache_round_trips() {
        let mut deeds = BTreeMap::new();
        deeds.insert([7u8; 16], Deed { title: "헛간 구조".into(), tags: "Secrets.GoodDeeds.BarnRescue".into(), place: "하데아".into() });
        let mut seen = HashMap::new();
        seen.insert("Quest01".to_string(), ("가족 재회".to_string(), "부모에 대한 단서".to_string()));
        let (d, s) = parse(&render(&deeds, &seen));
        assert_eq!((d, s), (deeds, seen));
    }
}
