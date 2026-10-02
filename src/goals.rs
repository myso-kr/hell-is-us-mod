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
//! What each actor hands out does not change, so it is read once per actor; whether
//! it is still new is decided every step against the knowledge of the moment.

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
            Tier::Quest => "퀘스트 목표",
            Tier::Secret => "비밀",
            Tier::Clue => "단서",
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
}

const RESCAN: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct Goals {
    levels: Option<u64>,
    interactable: HashMap<u64, bool>,
    payloads: HashMap<u64, (u64, Option<Payload>)>,
    /// A fact asset's investigation, by the fact's address.
    fact_quest: HashMap<u64, Option<u32>>,
    /// Tag names, by index — for the tier rules and for the detail line.
    tag_names: HashMap<u32, String>,
    scanned: Option<Instant>,
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
        if facts.is_empty() && tags.is_empty() {
            return None;
        }
        let rc = mem::read_u64(m, actor + root).filter(|&p| mem::plausible(p))?;
        let used = crate::actors::component(m, n, actor, "InteractionActionComponent")
            .and_then(|c| n.field(m, c, "bHasBeenActivated").filter(|p| p.size == 1).map(|p| c + p.offset as u64));
        Some(Payload { facts, tags, used, root: rc, label: pretty(&n.class(m, actor).unwrap_or_default()) })
    }

    /// Read what the loaded interactables hand out, when a scan is due.
    pub fn refresh(&mut self, m: &dyn Memory, n: &Names, hero: u64, root: u64, actors: u64) {
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
        for lv in crate::actors::array(m, world + levels, 4096) {
            for actor in crate::actors::array(m, lv + actors, 500_000) {
                let Some(class) = mem::read_u64(m, actor + CLASS).filter(|&c| mem::plausible(c)) else { continue };
                let interactable = *self.interactable.entry(class).or_insert_with(|| {
                    n.lineage(m, class).into_iter().any(|c| n.object(m, c).as_deref() == Some("InteractableActor"))
                });
                if !interactable {
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
    }

    /// Every place that still holds something new, and what kind, against `k`.
    pub fn evaluate(&self, m: &dyn Memory, k: &Knowledge, location: u64) -> Vec<Goal> {
        let mut out = Vec::new();
        for (&actor, (class, p)) in &self.payloads {
            let Some(p) = p else { continue };
            if mem::read_u64(m, actor + CLASS) != Some(*class) {
                continue;
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
            let tier = if new_facts.iter().any(|(_, q)| q.is_some_and(|q| k.quests.contains(&q)))
                || new_tags.iter().any(|t| tag(t).starts_with("Quest."))
            {
                Tier::Quest
            } else if new_tags.iter().any(|t| tag(t).starts_with("Secrets.")) {
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
            let detail = new_tags.first().map(|t| tag(t)).unwrap_or_else(|| format!("새 사실 {}개", new_facts.len()));
            out.push(Goal { tier, id: actor, label: p.label.clone(), detail, at });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
