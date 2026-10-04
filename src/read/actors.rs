//! Things near the hero worth seeing on the minimap — enemies, items to pick up,
//! loot, people, doors and puzzles — found among the actors of every loaded level.
//!
//! The world's loaded levels are `UWorld.Levels` (reflected). A level's actors are
//! `ULevel::Actors`, which is not reflected: it is found once as the one array in
//! the hero's level that holds the hero. Every actor is sorted by its class lineage
//! (`classify`), with the answer kept per class, so a scan reads one pointer per
//! actor that is not wanted.
//!
//! A full scan runs once a second; in between, only the tracked actors' positions
//! are read, and an actor whose class pointer changed (destroyed, memory reused) is
//! dropped on the spot.
//!
//! The game leaves spent things in the world, so each is also checked for being
//! done with (`Done`): an enemy whose `HealthAttributeSet.Health` is 0 is a corpse,
//! and an item, loot or door whose `InteractionActionComponent.bHasBeenActivated` is
//! set has been picked up or used. Where those live is found once per actor.

use crate::mem::{self, Memory};
use crate::names::{Names, CLASS, OUTER};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Enemy,
    Item,
    Loot,
    Npc,
    Interact,
    Save,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Enemy, Kind::Item, Kind::Loot, Kind::Npc, Kind::Interact, Kind::Save];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Enemy => tr!("ENEMIES"),
            Kind::Item => tr!("ITEMS"),
            Kind::Loot => tr!("LOOT"),
            Kind::Npc => "NPC",
            Kind::Interact => tr!("DOORS_AND_PUZZLES"),
            Kind::Save => tr!("SAVE_AND_TRAVEL"),
        }
    }

    pub fn bit(self) -> u8 {
        1 << self as u8
    }

    /// Its colour on the map and in the panel.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Kind::Enemy => [235, 60, 50],
            Kind::Item => [90, 220, 110],
            Kind::Loot => [255, 150, 40],
            Kind::Npc => [80, 200, 240],
            Kind::Interact => [190, 130, 255],
            Kind::Save => [250, 220, 70],
        }
    }
}

/// A finer sort within a kind, for the map's filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sub {
    Feral,
    Primeval,
    Negator,
    Protector,
    OtherEnemy,
    Medicine,
    Food,
    Consumable,
    Weapon,
    Gear,
    Skill,
    DroneModule,
    Research,
    Lore,
    Quest,
    Stash,
    OtherItem,
    Loot,
    /// Someone with nothing more to say than a line or two (a villager, a soldier).
    Npc,
    /// Someone to talk with: the story's conversations, the forge, the trades.
    NpcTalk,
    /// Someone who tells a secret (a quick chat that is one).
    NpcSecret,
    /// Someone a quest needs (a quick chat that is one).
    NpcQuest,
    Door,
    LymbicLock,
    Translation,
    /// A save point that also takes the hero to the APC (most do).
    SavePoint,
    /// A save point that does not take the hero to the APC (`…NoTravel…`).
    SavePointLocal,
    /// The APC's door: the way to the other regions.
    Apc,
    /// An enemy group (spawner) not beaten yet — from the survey, not the scan.
    EnemyGroup,
    /// A puzzle not solved yet (dial, code, item placement) — from the survey.
    Puzzle,
    /// A Vault of Forbidden Knowledge's door not opened yet.
    Vault,
}

impl Sub {
    pub const ALL: [Sub; 31] = [
        Sub::Feral,
        Sub::Primeval,
        Sub::Negator,
        Sub::Protector,
        Sub::OtherEnemy,
        Sub::Medicine,
        Sub::Food,
        Sub::Consumable,
        Sub::Weapon,
        Sub::Gear,
        Sub::Skill,
        Sub::DroneModule,
        Sub::Research,
        Sub::Lore,
        Sub::Quest,
        Sub::Stash,
        Sub::OtherItem,
        Sub::Loot,
        Sub::Npc,
        Sub::NpcTalk,
        Sub::NpcSecret,
        Sub::NpcQuest,
        Sub::Door,
        Sub::LymbicLock,
        Sub::Translation,
        Sub::SavePoint,
        Sub::SavePointLocal,
        Sub::Apc,
        Sub::EnemyGroup,
        Sub::Puzzle,
        Sub::Vault,
    ];

    pub fn kind(self) -> Kind {
        use Sub::*;
        match self {
            Feral | Primeval | Negator | Protector | OtherEnemy | EnemyGroup => Kind::Enemy,
            Medicine | Food | Consumable | Weapon | Gear | Skill | DroneModule | Research | Lore | Quest | Stash
            | OtherItem => Kind::Item,
            Loot => Kind::Loot,
            Npc | NpcTalk | NpcSecret | NpcQuest => Kind::Npc,
            Door | LymbicLock | Translation | Puzzle | Vault => Kind::Interact,
            SavePoint | SavePointLocal | Apc => Kind::Save,
        }
    }

    /// How minimap.txt names it.
    pub fn id(self) -> &'static str {
        use Sub::*;
        match self {
            Feral => "enemy.feral",
            Primeval => "enemy.primeval",
            Negator => "enemy.negator",
            Protector => "enemy.protector",
            OtherEnemy => "enemy.other",
            Medicine => "item.medicine",
            Food => "item.food",
            Consumable => "item.consumable",
            Weapon => "item.weapon",
            Gear => "item.gear",
            Skill => "item.skill",
            DroneModule => "item.drone",
            Research => "item.research",
            Lore => "item.lore",
            Quest => "item.quest",
            Stash => "item.stash",
            OtherItem => "item.other",
            Loot => "loot",
            Npc => "npc",
            NpcTalk => "npc.talk",
            NpcSecret => "npc.secret",
            NpcQuest => "npc.quest",
            Door => "interact.door",
            LymbicLock => "interact.lock",
            Translation => "interact.translation",
            SavePoint => "save",
            SavePointLocal => "save.local",
            Apc => "save.apc",
            EnemyGroup => "enemy.group",
            Puzzle => "interact.puzzle",
            Vault => "interact.vault",
        }
    }

    pub fn label(self) -> &'static str {
        use Sub::*;
        match self {
            Feral => "Feral",
            Primeval => "Primeval",
            Negator => "Negator",
            Protector => "Protector",
            OtherEnemy => tr!("OTHER"),
            Medicine => tr!("MEDICINE"),
            Food => tr!("FOOD"),
            Consumable => tr!("OTHER_CONSUMABLES"),
            Weapon => tr!("WEAPONS"),
            Gear => tr!("DEFENSIVE_GEAR"),
            Skill => tr!("LYMBIC_SKILLS"),
            DroneModule => tr!("DRONE_MODULES"),
            Research => tr!("RESEARCH"),
            Lore => tr!("RECORDS"),
            Quest => tr!("QUESTS_AND_SECRETS"),
            Stash => tr!("SUPPLIES"),
            OtherItem => tr!("OTHER"),
            Loot => tr!("ENEMY_LOOT_BOX"),
            Npc => tr!("NPC_OTHER"),
            NpcTalk => tr!("NPC_TALK"),
            NpcSecret => tr!("NPC_SECRET"),
            NpcQuest => tr!("NPC_QUEST"),
            Door => tr!("DOOR"),
            LymbicLock => tr!("LYMBIC_LOCK"),
            Translation => tr!("DRONE_TRANSLATION"),
            SavePoint => tr!("SAVE_POINT"),
            SavePointLocal => tr!("SAVE_POINT_LOCAL"),
            Apc => tr!("APC"),
            EnemyGroup => tr!("ENEMY_GROUP_LEFT"),
            Puzzle => tr!("UNSOLVED_PUZZLE"),
            Vault => tr!("VAULT_DOOR"),
        }
    }
}

/// The finer sort of an item, by its class name, as named on build 24045435.
fn item(name: &str) -> Sub {
    let name = name.strip_prefix("Tutorial_").unwrap_or(name);
    let has = |p: &str| name.contains(p);
    if name.starts_with("LymbicSkill") {
        Sub::Skill
    } else if name.starts_with("Weapon_") || ["Sword", "Axe", "Spear", "Glaive", "Halberd"].iter().any(|w| has(w)) {
        Sub::Weapon
    } else if has("Gear") {
        Sub::Gear
    } else if name.starts_with("Drone_") {
        Sub::DroneModule
    } else if name.starts_with("Research_") {
        Sub::Research
    } else if name.starts_with("Quest") || ["Key", "Treasure", "Secret"].iter().any(|w| has(w)) {
        Sub::Quest
    } else if name.starts_with("Lore") || ["Tome", "Journal", "Note", "Book", "Parchment"].iter().any(|w| has(w)) {
        Sub::Lore
    } else if has("Stash") {
        Sub::Stash
    } else if has("Medicine") || has("FirstAid") {
        Sub::Medicine
    } else if has("Food") {
        Sub::Food
    } else if name.starts_with("Cons_") {
        Sub::Consumable
    } else {
        Sub::OtherItem
    }
}

/// What an actor is, from its class and the classes above it (nearest first), as
/// read on build 24045435. `None`: not shown.
pub fn classify(lineage: &[String]) -> Option<Sub> {
    let has = |name: &str| lineage.iter().any(|c| c == name);
    let any = |part: &str| lineage.iter().any(|c| c.contains(part));
    let own = lineage.first().map(String::as_str).unwrap_or("");
    if has("CharlieLymbicEntity") {
        Some(
            [
                ("Feral", Sub::Feral),
                ("Primeval", Sub::Primeval),
                ("Negator", Sub::Negator),
                ("Protector", Sub::Protector),
            ]
            .into_iter()
            .find(|(w, _)| any(w))
            .map_or(Sub::OtherEnemy, |(_, s)| s),
        )
    } else if has("Base_Item_GatherSingleUse_Interact_BP_C") {
        Some(item(own))
    } else if has("Base_EnemyLootContainer_BP_C") {
        Some(Sub::Loot)
    } else if has("NpcActor") {
        // As the game names them: `Convo_…` (a conversation), `Quickchat_Secret_…`,
        // `Quickchat_Quest_…`, else a line or two (`…Generic…`).
        let own = own.to_ascii_lowercase();
        Some(if has("Base_NPC_Conversation_BP_C") {
            Sub::NpcTalk
        } else if own.contains("_secret_") {
            Sub::NpcSecret
        } else if own.contains("_quest_") {
            Sub::NpcQuest
        } else {
            Sub::Npc
        })
    } else if has("InteractableCheckpointActor") {
        // Every save point can take the hero to the APC (`TravelToAPCAction`) but the
        // `…NoTravel…` ones.
        Some(if own.contains("NoTravel") { Sub::SavePointLocal } else { Sub::SavePoint })
    } else if has("APC_Enter_Interact_BP_C") {
        Some(Sub::Apc)
    } else if any("LymbicLockPanel") {
        Some(Sub::LymbicLock)
    } else if any("DroneTranslation") {
        Some(Sub::Translation)
    } else if has("InteractableDoorActor") {
        Some(Sub::Door)
    } else {
        None
    }
}

/// One thing for the map: what it is, and where (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thing {
    pub sub: Sub,
    pub at: [f32; 3],
}

impl Thing {
    pub fn kind(&self) -> Kind {
        self.sub.kind()
    }
}

/// How far into a level to look for its actor array.
const LEVEL_SPAN: std::ops::Range<u64> = 0x28..0x300;
const MAX_LEVELS: u32 = 4096;
const MAX_ACTORS: u32 = 500_000;
const RESCAN: Duration = Duration::from_secs(1);

/// Where to read whether an actor is spent.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Done {
    /// `FGameplayAttributeData.CurrentValue` of its health: spent at 0.
    Health(u64),
    /// A one-byte bool: spent when set.
    Activated(u64),
}

impl Done {
    fn spent(self, m: &dyn Memory) -> bool {
        match self {
            Done::Health(at) => mem::read_f32(m, at).is_some_and(|h| h <= 0.0),
            Done::Activated(at) => {
                let mut b = [0u8];
                m.read(at, &mut b) && b[0] != 0
            }
        }
    }
}

struct Tracked {
    actor: u64,
    class: u64,
    root: u64,
    sub: Sub,
    done: Option<Done>,
    /// A Hollow Walker's `HazeRecords` (the Hazes that keep it alive): the field's offset.
    records: Option<u32>,
}

/// A Haze and a Hollow Walker it keeps alive: where each is (cm).
pub type HazeLink = ([f32; 3], [f32; 3]);

/// `HazeRecord` (build 24045435): `HazeToSpawn` (a soft class, 40 B) at 0x0 and
/// `SpawnedHaze` (the Haze actor) at 0x30, null until the Haze is spawned in the fight;
/// 0x38 bytes a record. Confirmed in play 2026-10-04: one `ST_HazeEcstasy_Wheelface`
/// held four Walkers (Protector, Feral, two Artillery), each a record pointing at it.
const HAZE_RECORD: u64 = 0x38;
const SPAWNED_HAZE: u64 = 0x30;
/// More records than this on one Walker is not a record array.
const MAX_RECORDS: u32 = 16;

/// The actor's property holding an object of class `want` (or one below it).
pub(crate) fn component(m: &dyn Memory, n: &Names, actor: u64, want: &str) -> Option<u64> {
    let class = mem::read_u64(m, actor + CLASS)?;
    n.lineage(m, class).into_iter().find_map(|c| {
        n.properties(m, c).into_iter().filter(|p| p.size == 8).find_map(|p| {
            let v = mem::read_u64(m, actor + p.offset as u64).filter(|&v| mem::plausible(v))?;
            n.is_a(m, v, want).then_some(v)
        })
    })
}

/// Every one of the actor's properties holding an object of class `want` (or below).
pub(crate) fn components(m: &dyn Memory, n: &Names, actor: u64, want: &str) -> Vec<u64> {
    let Some(class) = mem::read_u64(m, actor + CLASS) else { return Vec::new() };
    let mut out: Vec<u64> = n
        .lineage(m, class)
        .into_iter()
        .flat_map(|c| n.properties(m, c))
        .filter(|p| p.size == 8)
        .filter_map(|p| mem::read_u64(m, actor + p.offset as u64).filter(|&v| mem::plausible(v)))
        .filter(|&v| n.is_a(m, v, want))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Where this actor says it is spent, if it says so anywhere this knows.
fn find_done(m: &dyn Memory, n: &Names, actor: u64, kind: Kind, sets: u64) -> Option<Done> {
    if kind == Kind::Enemy {
        let asc = component(m, n, actor, "AbilitySystemComponent")?;
        let set =
            array(m, asc + sets, 64).into_iter().find(|&s| n.class(m, s).as_deref() == Some("HealthAttributeSet"))?;
        let p = n.field(m, set, "Health").filter(|p| p.size == 16)?;
        return Some(Done::Health(set + p.offset as u64 + 0xC));
    }
    // A save point is used again and again: having been used says nothing.
    if kind == Kind::Save {
        return None;
    }
    let comp = component(m, n, actor, "InteractionActionComponent")?;
    let p = n.field(m, comp, "bHasBeenActivated").filter(|p| p.size == 1)?;
    Some(Done::Activated(comp + p.offset as u64))
}

#[derive(Default)]
pub struct Scanner {
    /// World → Levels (reflected) and Level → Actors (found), once known.
    levels: Option<u64>,
    actors: Option<u64>,
    kinds: HashMap<u64, Option<Sub>>,
    /// Per class: where its `HazeRecords` is, if it has one.
    records: HashMap<u64, Option<u32>>,
    /// The Haze links as last read with the positions.
    links: Vec<HazeLink>,
    /// `Actor.RootComponent`'s offset, from `refresh`: where a Haze not tracked as a thing
    /// is found.
    root_off: u64,
    /// Per actor, kept across scans: (its class, where it says it is spent).
    done: HashMap<u64, (u64, Option<Done>)>,
    tracked: Vec<Tracked>,
    scanned: Option<Instant>,
}

/// A `TArray` of object pointers, read in one go; empty if it does not look like one.
pub fn array(m: &dyn Memory, at: u64, cap: u32) -> Vec<u64> {
    let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return Vec::new() };
    if !mem::plausible(data) || num == 0 || num > cap {
        return Vec::new();
    }
    let mut buf = vec![0u8; num as usize * 8];
    if !m.read(data, &mut buf) {
        return Vec::new();
    }
    buf.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).filter(|&p| mem::plausible(p)).collect()
}

/// A TArray of inline structs at `at`: each element's address, `size` bytes apart.
pub fn array_of(m: &dyn Memory, at: u64, size: u64, cap: u32) -> Vec<u64> {
    let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return Vec::new() };
    if !mem::plausible(data) || num == 0 || num > cap || size == 0 {
        return Vec::new();
    }
    (0..num as u64).map(|i| data + i * size).collect()
}

/// The offset of `ULevel::Actors`: the one array in the hero's level holding the hero.
fn find_actors(m: &dyn Memory, level: u64, hero: u64) -> Option<u64> {
    let hits: Vec<u64> =
        LEVEL_SPAN.step_by(8).filter(|&off| array(m, level + off, MAX_ACTORS).contains(&hero)).collect();
    match hits[..] {
        [off] => Some(off),
        _ => None,
    }
}

impl Scanner {
    /// `ULevel::Actors`' offset, once a scan has found it.
    pub fn actors_offset(&self) -> Option<u64> {
        self.actors
    }

    /// A full scan when one is due. `hero` is the hero pawn; `root` is
    /// `Actor.RootComponent`'s offset, `sets` the ability system's `SpawnedAttributes`.
    pub fn refresh(&mut self, m: &dyn Memory, n: &Names, hero: u64, root: u64, sets: u64) -> Result<(), String> {
        self.root_off = root;
        if self.scanned.is_some_and(|t| t.elapsed() < RESCAN) {
            return Ok(());
        }
        self.scanned = Some(Instant::now());
        let level = mem::read_u64(m, hero + OUTER).filter(|&p| mem::plausible(p)).ok_or("no level")?;
        let world = mem::read_u64(m, level + OUTER).filter(|&p| mem::plausible(p)).ok_or("no world")?;
        if self.levels.is_none() {
            self.levels = Some(n.field(m, world, "Levels").ok_or("World.Levels not found")?.offset as u64);
        }
        if self.actors.is_none() {
            self.actors = Some(find_actors(m, level, hero).ok_or("the level's actor list was not found")?);
        }
        let (levels_off, actors_off) = (self.levels.unwrap(), self.actors.unwrap());

        let mut tracked = Vec::new();
        for lv in array(m, world + levels_off, MAX_LEVELS) {
            for actor in array(m, lv + actors_off, MAX_ACTORS) {
                if actor == hero {
                    continue;
                }
                let Some(class) = mem::read_u64(m, actor + CLASS).filter(|&c| mem::plausible(c)) else { continue };
                let sub = *self.kinds.entry(class).or_insert_with(|| {
                    let lineage: Vec<String> = n.lineage(m, class).into_iter().filter_map(|c| n.object(m, c)).collect();
                    classify(&lineage)
                });
                let Some(sub) = sub else { continue };
                let Some(rc) = mem::read_u64(m, actor + root).filter(|&p| mem::plausible(p)) else { continue };
                let done = match self.done.get(&actor) {
                    Some(&(c, d)) if c == class => d,
                    _ => {
                        let d = find_done(m, n, actor, sub.kind(), sets);
                        self.done.insert(actor, (class, d));
                        d
                    }
                };
                let records = if sub.kind() == Kind::Enemy {
                    *self
                        .records
                        .entry(class)
                        .or_insert_with(|| n.field(m, actor, "HazeRecords").filter(|p| p.size == 16).map(|p| p.offset))
                } else {
                    None
                };
                tracked.push(Tracked { actor, class, root: rc, sub, done, records });
            }
        }
        let alive: std::collections::HashSet<u64> = tracked.iter().map(|t| t.actor).collect();
        self.done.retain(|a, _| alive.contains(a));
        self.tracked = tracked;
        Ok(())
    }

    /// Where every tracked actor is now (cm). `location` is
    /// `SceneComponent.RelativeLocation`'s offset.
    /// The actors of a kind still in play (not spent — an enemy not dead), as last
    /// scanned: what the enemy cheats write to.
    pub fn actors_of(&self, m: &dyn Memory, kind: Kind) -> Vec<u64> {
        self.tracked
            .iter()
            .filter(|t| t.sub.kind() == kind && mem::read_u64(m, t.actor + CLASS) == Some(t.class))
            .filter(|t| !t.done.is_some_and(|d| d.spent(m)))
            .map(|t| t.actor)
            .collect()
    }

    /// The Hazes and the Walkers they keep alive, as of the last `positions`.
    pub fn links(&self) -> Vec<HazeLink> {
        self.links.clone()
    }

    pub fn positions(&mut self, m: &dyn Memory, location: u64) -> Vec<Thing> {
        let mut out = Vec::with_capacity(self.tracked.len());
        // Where each live actor is, for the Haze links after.
        let mut at: HashMap<u64, [f32; 3]> = HashMap::new();
        self.tracked.retain(|t| {
            if mem::read_u64(m, t.actor + CLASS) != Some(t.class) {
                return false;
            }
            if t.done.is_some_and(|d| d.spent(m)) {
                return true; // still tracked — a door can close again — but not shown
            }
            let mut b = [0u8; 24];
            if !m.read(t.root + location, &mut b) {
                return false;
            }
            let d = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()) as f32;
            let p = [d(0), d(1), d(2)];
            if p.iter().all(|v| v.is_finite()) {
                out.push(Thing { sub: t.sub, at: p });
                at.insert(t.actor, p);
            }
            true
        });
        // Each live Walker's records: the Hazes it hangs on, those that are in play.
        let mut links = Vec::new();
        for t in &self.tracked {
            let (Some(off), Some(&walker)) = (t.records, at.get(&t.actor)) else { continue };
            let (Some(data), Some(num)) =
                (mem::read_u64(m, t.actor + off as u64), mem::read_u32(m, t.actor + off as u64 + 8))
            else {
                continue;
            };
            if !mem::plausible(data) || num > MAX_RECORDS {
                continue;
            }
            for i in 0..num as u64 {
                let haze = mem::read_u64(m, data + i * HAZE_RECORD + SPAWNED_HAZE).unwrap_or(0);
                if haze == t.actor || !mem::plausible(haze) {
                    continue;
                }
                // Tracked as a thing, or read where it stands: a Haze may be of a class the
                // things are not sorted into.
                let h = at.get(&haze).copied().or_else(|| {
                    let rc = mem::read_u64(m, haze + self.root_off).filter(|&p| mem::plausible(p))?;
                    let mut b = [0u8; 24];
                    m.read(rc + location, &mut b).then_some(())?;
                    let d = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()) as f32;
                    let p = [d(0), d(1), d(2)];
                    p.iter().all(|v| v.is_finite() && v.abs() < 1.0e7).then_some(p)
                });
                if let Some(h) = h {
                    links.push((h, walker));
                }
            }
        }
        self.links = links;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sorts_actors_by_their_lineage() {
        let walker = l(&[
            "ST_HollowWalker_Loner_Feral_Tier1_BP_C",
            "ST_Base_HollowWalker_Loner_Feral_BP_C",
            "HollowWalkerLoner",
            "CharlieLymbicEntity",
        ]);
        assert_eq!(classify(&walker), Some(Sub::Feral));
        assert_eq!(classify(&l(&["Odd_BP_C", "CharlieLymbicEntity"])), Some(Sub::OtherEnemy));
        let gather = |own: &str| l(&[own, "Base_Item_GatherSingleUse_Interact_BP_C", "InteractableActor"]);
        for (own, want) in [
            ("Cons_MedicineCivilianT01_GatherSingleUse_Interact_BP_C", Sub::Medicine),
            ("Tutorial_Cons_MedicineCivilianT01_GatherSingleUse_Interact_BP_C", Sub::Medicine),
            ("Quest01OMSIFFirstAidKit_GatherSingleUse_Interact_BP_C", Sub::Quest),
            ("Cons_FoodCivilianT01_GatherSingleUse_Interact_BP_C", Sub::Food),
            ("Cons_LymbicChargerT01_GatherSingleUse_Interact_BP_C", Sub::Consumable),
            ("Weapon_G3_Lvl10_1HSwordEcstasy_GatherSingleUse_Interact_BP_C", Sub::Weapon),
            ("TwinAxesNeutralG1_GatherSingleUse_Interact_BP_C", Sub::Weapon),
            ("DefensiveGear_TerrorV04_BracedForImpact_GatherSingleUse_Interact_BP_C", Sub::Gear),
            ("LymbicSkillActive_Rage_RageSpike_G02_GatherSingleUse_Interact_BP_C", Sub::Skill),
            ("Drone_BackstabPrevention_G01_GatherSingleUse_Interact_BP_C", Sub::DroneModule),
            ("Research_Conspiracy04_GatherSingleUse_Interact_BP_C", Sub::Research),
            ("LoreBloodQueenPrisonerJournal_GatherSingleUse_Interact_BP_C", Sub::Lore),
            ("ArcasSpireDoorTopFloor_Tome02_GatherSingleUse_Interact_BP_C", Sub::Lore),
            ("SenedraCaddellsTreasureNote01_GatherSingleUse_Interact_BP_C", Sub::Quest),
            ("SabinianSoldierStash_GatherSingleUse_Interact_BP_C", Sub::Stash),
            ("Mystery_GatherSingleUse_Interact_BP_C", Sub::OtherItem),
        ] {
            assert_eq!(classify(&gather(own)), Some(want), "{own}");
        }
        assert_eq!(classify(&l(&["Base_EnemyLootContainer_BP_C", "InteractableActor"])), Some(Sub::Loot));
        let quick = |own: &str| l(&[own, "Base_NPC_QuickChat_BP_C", "NpcActor"]);
        assert_eq!(classify(&quick("Quickchat_ZGeneric_SabinianSoldier04_BP_C")), Some(Sub::Npc));
        assert_eq!(classify(&quick("QuickChat_Secret_JudithKarryBabyAlive_BP_C")), Some(Sub::NpcSecret));
        assert_eq!(classify(&quick("Quickchat_Quest_DyingOMSIF_BP_C")), Some(Sub::NpcQuest));
        assert_eq!(classify(&l(&["Convo_Sophie_BP_C", "Base_NPC_Conversation_BP_C", "NpcActor"])), Some(Sub::NpcTalk));
        assert_eq!(classify(&l(&["APC_Enter_Interact_BP_C", "InteractableActor"])), Some(Sub::Apc));
        let local = l(&[
            "Child_SavePointLeftNoTravel_Interact_BP_C",
            "Base_SavePoint_Interact_BP_C",
            "InteractableCheckpointActor",
        ]);
        assert_eq!(classify(&local), Some(Sub::SavePointLocal));
        assert_eq!(
            classify(&l(&["LymbicLockPanel_2ndGen_Rage_Y_Z_Interact_BP_C", "InteractableActor"])),
            Some(Sub::LymbicLock)
        );
        assert_eq!(classify(&l(&["X_DroneTranslation_Interact_BP_C", "InteractableActor"])), Some(Sub::Translation));
        assert_eq!(classify(&l(&["SmallFenceDoor01_BP_C", "InteractableDoorActor"])), Some(Sub::Door));
        let save = l(&[
            "SenedraTravelGated_SavePointRight_Interact_BP_C",
            "Base_SavePoint_Interact_BP_C",
            "InteractableCheckpointActor",
            "InteractableActor",
        ]);
        assert_eq!(classify(&save), Some(Sub::SavePoint));
        assert_eq!(
            classify(&l(&["Base_CheckPoint_Trigger_BP_C", "OverlapCheckpointActor"])),
            None,
            "autosave triggers"
        );
        assert_eq!(classify(&l(&["Base_TutorialToast_Interact_BP_C", "InteractableActor"])), None, "tutorial triggers");
        assert_eq!(classify(&l(&["StaticMeshActor", "Actor", "Object"])), None);
    }

    #[test]
    fn every_sub_has_a_unique_id_and_a_kind_in_order() {
        let ids: std::collections::HashSet<_> = Sub::ALL.iter().map(|s| s.id()).collect();
        assert_eq!(ids.len(), Sub::ALL.len());
        for k in Kind::ALL {
            assert!(Sub::ALL.iter().any(|s| s.kind() == k), "{k:?} has no sub");
        }
    }

    #[test]
    fn layer_bits_are_distinct() {
        let all = Kind::ALL.iter().fold(0u8, |acc, k| {
            assert_eq!(acc & k.bit(), 0);
            acc | k.bit()
        });
        assert_eq!(all, 0b11_1111);
    }
}
