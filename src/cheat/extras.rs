//! Cheats whose targets are not the hero: the enemies in play and the inventory
//! (.spec/CHEATS-RESEARCH.md §4 A).
//!
//! The hero's attributes have one owner and a record on disk (hold.rs). These have
//! many owners that come and go — an enemy spawns, dies, streams out — so what they
//! overwrite is remembered here, per target, in memory, and put back to the targets
//! still in play when the cheat stops. A killed panel leaves enemies as they were
//! last written until they respawn; nothing here outlives them.
//!
//! An enemy's record is tied to the actor it was taken from — its address and its class —
//! and, for the frail cheat, to the Health attribute it found there. A record whose enemy
//! is gone is dropped each tick, and nothing is written to an address unless that same
//! enemy (and attribute) is still there: a dead enemy's memory may hold something else by
//! then. The ghost's originals (one hero) are also kept on disk (`ghost.txt`), so a killed
//! panel's next run puts the hero back on its own side.
//!
//! - Enemy time: each enemy's `CustomTimeDilation` (the hero's own speed-up, turned on
//!   the enemies).
//! - Frail enemies: each enemy's `HealthAttributeSet.Health` held at 1 — one blow kills.
//!   Enemies keep health there, not in the hero's Endurance (probed on 24045435).
//! - Stock: inventory stacks of a class held at no less than when first seen, and
//!   never below 2 — a stack that reaches 0 is removed from the inventory before it
//!   could be put back.
//! - Set stock: a class's stacks written once, up to the item's `QuantityMax`.
//! - Weapon XP: when the game grants a weapon experience, as much again times the
//!   multiplier less one is added — to the total and to the experience within the
//!   level. Levelling is left to the game: it levels on its next grant.
//!
//! - Ghost: the hero's `TeamID` and `Faction` (CharlieCharacter, reflected) set to an
//!   enemy's — 1 against 2 on 24045435. Tried in play: with both, enemies ignore the
//!   hero and the hero's blows do not land; with TeamID alone, enemies still attack.
//!   Faction decides both, so a ghost is for getting past, not for fighting.
//! - Untouchable (no row in the table): neither `bCanBeDamaged` nor clearing the
//!   hero's `bGenerateOverlapEvents` stopped a blow — the game's hits go through
//!   tags and traces data writes do not reach (.spec/CHEATS-RESEARCH.md §6).
//!
//! A weapon item's experience is native, past its reflected fields (found with
//! `doctor watch` on 24045435 — a kill with the sword took the total 90 → 265,
//! within-level 90 → 5, next threshold 260 → 520, level 0 → 1, cap 3). Every read is
//! checked for that shape before anything is written.

use crate::cheats::{Active, Effect};
/// What the effects past the hero need of the attached game — engine.rs implements it
/// for `Attached`, so this module does not depend on the engine.
pub trait Reach {
    fn memory(&self) -> &dyn crate::mem::Memory;
    fn names(&self) -> &crate::names::Names;
    /// The game image's base address.
    fn base(&self) -> u64;
    fn hero(&self) -> Result<u64, String>;
    fn enemies(&self) -> Vec<u64>;
    fn inventory(&self) -> Result<u64, String>;
}
use crate::mem::{self, Memory};
use crate::names::{Names, CLASS};
use std::collections::HashMap;

/// A weapon item's experience, past its reflected fields (24045435): level, the
/// grade's level cap, total experience, experience within the level, and the total
/// the next level needs (0 at the cap).
const W_LEVEL: u64 = 0x130;
const W_CAP: u64 = 0x138;
const W_TOTAL: u64 = 0x13C;
const W_WITHIN: u64 = 0x140;
const W_NEXT: u64 = 0x148;

/// A weapon's (total, within-level) experience — if the bytes have the shape the
/// layout above promises: a level below its cap, a next threshold (0 means capped),
/// within-level no more than the total.
fn weapon_xp(m: &dyn Memory, item: u64) -> Option<(u32, u32)> {
    let mut b = [0u8; (W_NEXT - W_LEVEL + 4) as usize];
    m.read(item + W_LEVEL, &mut b).then_some(())?;
    let at = |o: u64| u32::from_le_bytes(b[(o - W_LEVEL) as usize..(o - W_LEVEL + 4) as usize].try_into().unwrap());
    let (level, cap, total, within, next) = (at(W_LEVEL), at(W_CAP), at(W_TOTAL), at(W_WITHIN), at(W_NEXT));
    // Past the next threshold is fine: the bonus lands between the game's grants, and
    // the game levels on its next one. At the cap the next threshold is 0.
    let sane = (1..=20).contains(&cap) && level < cap && next > 0 && within <= total;
    sane.then_some((total, within))
}

/// `ghost.txt` in the mod's data folder: the hero (address, hex) and its TeamID and Faction
/// before the ghost cheat, while it is on.
fn ghost_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("ghost.txt")
}

fn save_ghost(record: Option<(u64, u8, u8)>) {
    match record {
        Some((h, t, f)) => {
            let _ = std::fs::write(ghost_path(), format!("{h:x} {t} {f}\n"));
        }
        None => {
            let _ = std::fs::remove_file(ghost_path());
        }
    }
}

fn load_ghost() -> Option<(u64, u8, u8)> {
    parse_ghost(&std::fs::read_to_string(ghost_path()).ok()?)
}

fn parse_ghost(text: &str) -> Option<(u64, u8, u8)> {
    let mut w = text.split_whitespace();
    let h = u64::from_str_radix(w.next()?, 16).ok()?;
    Some((h, w.next()?.parse().ok()?, w.next()?.parse().ok()?))
}

/// A stack held by `Stock` never drops below this.
const STOCK_FLOOR: u32 = 2;
/// Frail enemies are held at this much health.
const FRAIL: f32 = 1.0;

#[derive(Default)]
pub struct Extras {
    /// Enemy → (its class, its time dilation before the cheat).
    time: HashMap<u64, (u64, f32)>,
    /// Enemy → what was found of it and its Health before the cheat.
    frail: HashMap<u64, Frail>,
    /// Enemy → (its class, its HealthMax when first cut, the share it was cut to).
    weaker: HashMap<u64, (u64, f32, f32)>,
    /// Enemies looked at for frail's leftovers since frail was last on (`tick`).
    mended: std::collections::HashSet<u64>,
    /// Inventory stack → the least it is held at.
    stock: HashMap<u64, u32>,
    /// Weapon item → its total experience as last seen (after any bonus).
    weapon_xp: HashMap<u64, u32>,
    /// The hero → its (TeamID, Faction) before the ghost cheat.
    team: Option<(u64, u8, u8)>,
    /// The hero, and its primitive components' overlap bits before the untouchable
    /// cheat: (byte address, mask, byte as it was).
    overlaps: Option<(u64, Vec<Bit>)>,
}

/// A frail enemy's record: the actor's class, its Health attribute and the set that holds
/// it (with the set's class and the attribute's vtable as found), and the values before.
#[derive(Clone, Copy)]
struct Frail {
    class: u64,
    at: u64,
    set: u64,
    set_class: u64,
    vtable: u64,
    base: f32,
    current: f32,
}

impl Frail {
    /// Still the attribute it was: two reads — the attribute's vtable and the set's class —
    /// where finding it again walks the enemy's ability system.
    fn holds(&self, m: &dyn Memory) -> bool {
        mem::read_u64(m, self.at) == Some(self.vtable) && mem::read_u64(m, self.set + CLASS) == Some(self.set_class)
    }
}

/// A bitfield bool to put back: (byte address, mask, byte as it was).
type Bit = (u64, u8, u8);

/// An actor's team and faction: CharlieCharacter.TeamID (GenericTeamId, one byte) and
/// Faction — their addresses.
fn team_at(m: &dyn Memory, n: &Names, actor: u64) -> Option<(u64, u64)> {
    let t = n.field(m, actor, "TeamID")?;
    let f = n.field(m, actor, "Faction")?;
    (t.size == 1 && f.size == 1).then(|| (actor + t.offset as u64, actor + f.offset as u64))
}

/// A bitfield bool's byte and mask: FBoolProperty keeps FieldSize, ByteOffset,
/// ByteMask, FieldMask just past the FProperty base (0x70 on 5.5).
fn bool_bit(m: &dyn Memory, n: &Names, obj: u64, name: &str) -> Option<(u64, u8)> {
    let p = n.field(m, obj, name)?;
    let mut b = [0u8; 4];
    m.read(p.field + 0x70, &mut b).then_some(())?;
    let (size, offset, mask) = (b[0], b[1], b[2]);
    (size == 1 && mask.count_ones() == 1).then(|| (obj + p.offset as u64 + offset as u64, mask))
}

fn byte(m: &dyn Memory, at: u64) -> Option<u8> {
    let mut b = [0u8; 1];
    m.read(at, &mut b).then_some(b[0])
}

/// One inventory stack: where its count is, the count, and its class's name.
struct Stack {
    count_at: u64,
    count: u32,
    max: u32,
    class: String,
    item: u64,
}

/// The stacks in the hero's inventory. The count is native — the u32 right after the
/// reflected `ItemData` pointer (probed on 24045435: it matches the save's `Quantity`).
fn stacks(m: &dyn Memory, n: &Names, inv: u64) -> Vec<Stack> {
    let Some(items) = n.field(m, inv, "Items") else { return Vec::new() };
    let mut out = Vec::new();
    for item in crate::actors::array(m, inv + items.offset as u64, 4096) {
        let Some(data) = n.field(m, item, "ItemData") else { continue };
        let Some(def) = mem::read_u64(m, item + data.offset as u64).filter(|&p| mem::plausible(p)) else { continue };
        let count_at = item + data.offset as u64 + 8;
        let Some(count) = mem::read_u32(m, count_at) else { continue };
        let max = n.field(m, def, "QuantityMax").and_then(|p| mem::read_u32(m, def + p.offset as u64)).unwrap_or(0);
        let class = mem::read_u64(m, item + CLASS).and_then(|c| n.object(m, c)).unwrap_or_default();
        // A count past the item's own maximum is not a count: the layout moved.
        if max == 0 || count > max {
            continue;
        }
        out.push(Stack { count_at, count, max, class, item });
    }
    out
}

/// An enemy's Health attribute: the address of its `FGameplayAttributeData`.
fn health_of(m: &dyn Memory, n: &Names, enemy: u64) -> Option<(u64, u64)> {
    let (_, asc) = crate::player::find_asc(n, m, enemy).ok()?;
    let sets = n.field(m, asc, "SpawnedAttributes")?;
    for set in crate::actors::array(m, asc + sets.offset as u64, 64) {
        if mem::read_u64(m, set + CLASS).and_then(|c| n.object(m, c)).as_deref() == Some("HealthAttributeSet") {
            return n.field(m, set, "Health").map(|p| (set, set + p.offset as u64));
        }
    }
    None
}

/// The health set's HealthMax (current).
fn max_of(m: &dyn Memory, n: &Names, set: u64) -> Option<f32> {
    n.field(m, set, "HealthMax").and_then(|p| pair(m, set + p.offset as u64)).map(|(_, c)| c)
}

/// An attribute's (BaseValue, CurrentValue): FGameplayAttributeData is vtable, base,
/// current.
fn pair(m: &dyn Memory, at: u64) -> Option<(f32, f32)> {
    Some((mem::read_f32(m, at + 8)?, mem::read_f32(m, at + 12)?))
}

fn put_pair(m: &dyn Memory, at: u64, base: f32, current: f32) -> bool {
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&base.to_le_bytes());
    b[4..].copy_from_slice(&current.to_le_bytes());
    m.write(at + 8, &b)
}

impl Extras {
    /// One tick of every active cheat that reaches past the hero. Errors are reported,
    /// not fatal — an inventory not found yet, an enemy streaming out.
    pub fn tick(&mut self, a: &dyn Reach, active: &[Active]) -> Vec<String> {
        let (m, n) = (a.memory(), a.names());
        let mut errors = Vec::new();
        let wants = |f: fn(&Effect) -> bool| active.iter().find(|t| t.effects().iter().any(f));
        let alive = a.enemies();
        let class_of = |e: u64| mem::read_u64(m, e + CLASS);
        // The gone are forgotten: there is nothing of them to put back, and their memory
        // may be another object's by now.
        self.time.retain(|e, (c, _)| alive.contains(e) && class_of(*e) == Some(*c));
        self.frail.retain(|e, f| alive.contains(e) && class_of(*e) == Some(f.class));
        self.weaker.retain(|e, (c, ..)| alive.contains(e) && class_of(*e) == Some(*c));
        // The ghost let go of with a record left (a killed panel's): put back now.
        if self.team.is_some() && wants(|e| matches!(e, Effect::Ghost)).is_none() {
            self.release_ghost(a);
        }

        if let Some(t) = wants(|e| matches!(e, Effect::EnemyTime)) {
            for &e in &alive {
                let Some(p) = n.field(m, e, "CustomTimeDilation") else { continue };
                let at = e + p.offset as u64;
                if let std::collections::hash_map::Entry::Vacant(v) = self.time.entry(e) {
                    match (class_of(e), mem::read_f32(m, at)) {
                        (Some(c), Some(orig)) => {
                            v.insert((c, orig));
                        }
                        _ => continue,
                    }
                }
                if !m.write(at, &t.value.to_le_bytes()) {
                    errors.push("enemy time: write failed".into());
                }
            }
        }

        if wants(|e| matches!(e, Effect::EnemyFrail)).is_some() {
            for &e in &alive {
                // The attribute written is the one this enemy has now, never an address kept
                // from an enemy that was here before: checked every tick, found again when
                // it no longer holds.
                let at = match self.frail.get(&e).filter(|f| f.holds(m)) {
                    Some(f) => f.at,
                    None => {
                        let Some((set, at)) = health_of(m, n, e) else { continue };
                        let (Some(class), Some(set_class), Some(vtable), Some((base, current))) =
                            (class_of(e), class_of(set), mem::read_u64(m, at), pair(m, at))
                        else {
                            continue;
                        };
                        // Already held at frail's value (a panel started again with frail on):
                        // what to put back is its maximum, not that.
                        let (base, current) = match max_of(m, n, set) {
                            Some(max) if current <= FRAIL => (max, max),
                            _ => (base, current),
                        };
                        self.frail.insert(e, Frail { class, at, set, set_class, vtable, base, current });
                        at
                    }
                };
                if pair(m, at).is_some_and(|(_, c)| c > FRAIL) && !put_pair(m, at, FRAIL, FRAIL) {
                    errors.push("frail enemies: write failed".into());
                }
            }
        }

        // Frail's leftovers: with frail off, an enemy whose health is exactly frail's (base and
        // current alike — no hit leaves that) was held by an earlier panel and not let go.
        // Healed to its maximum, once per enemy.
        if wants(|e| matches!(e, Effect::EnemyFrail)).is_some() {
            self.mended.clear();
        } else if self.frail.is_empty() {
            self.mended.retain(|e| alive.contains(e));
            for &e in &alive {
                if !self.mended.insert(e) {
                    continue;
                }
                let Some((set, at)) = health_of(m, n, e) else { continue };
                if pair(m, at) == Some((FRAIL, FRAIL)) {
                    if let Some(max) = max_of(m, n, set).filter(|&x| x > FRAIL) {
                        put_pair(m, at, max, max);
                    }
                }
            }
        }

        // Weaker enemies: each cut once to the share of its maximum health, and moved by the
        // same ratio when the share moves; the fight goes on from there. Frail wins, and while
        // any frail record is left (being put back) nothing is cut: its health is not its own.
        let frail = wants(|e| matches!(e, Effect::EnemyFrail)).is_some() || !self.frail.is_empty();
        if let (Some(t), false) = (wants(|e| matches!(e, Effect::EnemyHealth)), frail) {
            let share = t.value.clamp(0.1, 1.0);
            for &e in &alive {
                let done = self.weaker.get(&e).copied();
                if let Some((class, b0, s0)) = done {
                    if (s0 - share).abs() < 1e-3 {
                        continue;
                    }
                    // The share moved: its health by the same ratio, so a hit taken stays taken
                    // (half its health lost at 10 % is half lost at 100 %).
                    let Some((_, at)) = health_of(m, n, e) else { continue };
                    let Some((_, c)) = pair(m, at) else { continue };
                    let to = (c * share / s0).min(b0);
                    if !put_pair(m, at, to, to) {
                        errors.push("weaker enemies: write failed".into());
                        continue;
                    }
                    self.weaker.insert(e, (class, b0, share));
                    continue;
                }
                let Some((set, at)) = health_of(m, n, e) else { continue };
                let Some((_, c)) = pair(m, at) else { continue };
                // The maximum, not the health: that may be cut already (by frail, or a hit).
                let Some(max) = max_of(m, n, set) else { continue };
                let base = done.map_or(max, |(_, b0, _)| b0);
                let to = base * share;
                if c > to && !put_pair(m, at, to, to) {
                    errors.push("weaker enemies: write failed".into());
                    continue;
                }
                if let Some(class) = class_of(e) {
                    self.weaker.insert(e, (class, base, share));
                }
            }
        }

        if let Some(t) = wants(|e| matches!(e, Effect::WeaponXp)) {
            match a.inventory() {
                Ok(inv) => {
                    for s in stacks(m, n, inv).into_iter().filter(|s| s.class.contains("WeaponItem")) {
                        let Some((total, within)) = weapon_xp(m, s.item) else {
                            self.weapon_xp.remove(&s.item);
                            continue;
                        };
                        let last = *self.weapon_xp.entry(s.item).or_insert(total);
                        if total > last {
                            let bonus = ((total - last) as f32 * (t.value - 1.0)).round().max(0.0) as u32;
                            let mut b = [0u8; 8];
                            b[..4].copy_from_slice(&(total + bonus).to_le_bytes());
                            b[4..].copy_from_slice(&(within + bonus).to_le_bytes());
                            if bonus > 0 && !m.write(s.item + W_TOTAL, &b) {
                                errors.push("weapon xp: write failed".into());
                            }
                            self.weapon_xp.insert(s.item, total + bonus);
                        } else {
                            self.weapon_xp.insert(s.item, total);
                        }
                    }
                }
                Err(e) => errors.push(format!("inventory: {e}")),
            }
        }

        if wants(|e| matches!(e, Effect::Ghost)).is_some() {
            match (a.hero(), alive.first().copied()) {
                (Ok(hero), enemy) => {
                    if let Some((t, f)) = team_at(m, n, hero) {
                        if self.team.is_none_or(|(h, ..)| h != hero) {
                            if let (Some(ot), Some(of)) = (byte(m, t), byte(m, f)) {
                                self.team = Some((hero, ot, of));
                                save_ghost(self.team);
                            }
                        }
                        // The enemies' team as they hold it; 2 on 24045435 when none is near.
                        let theirs = enemy
                            .and_then(|e| team_at(m, n, e))
                            .and_then(|(et, ef)| Some((byte(m, et)?, byte(m, ef)?)))
                            .unwrap_or((2, 2));
                        if !m.write(t, &[theirs.0]) || !m.write(f, &[theirs.1]) {
                            errors.push("ghost: write failed".into());
                        }
                    }
                }
                (Err(e), _) => errors.push(format!("ghost: {e}")),
            }
        }

        if wants(|e| matches!(e, Effect::Untouchable)).is_some() {
            if let Ok(hero) = a.hero() {
                // The hero's primitive components, found once per hero.
                if self.overlaps.as_ref().is_none_or(|(h, _)| *h != hero) {
                    match crate::gobjects::discover(m, a.base()) {
                        Ok(objects) => {
                            let bits = objects
                                .all(m)
                                .into_iter()
                                .filter(|&o| mem::read_u64(m, o + crate::names::OUTER) == Some(hero))
                                .filter(|&o| n.is_a(m, o, "PrimitiveComponent"))
                                .filter_map(|o| bool_bit(m, n, o, "bGenerateOverlapEvents"))
                                .filter_map(|(at, mask)| Some((at, mask, byte(m, at)?)))
                                .collect();
                            self.overlaps = Some((hero, bits));
                        }
                        Err(e) => errors.push(format!("untouchable: {e}")),
                    }
                }
                if let Some((_, bits)) = &self.overlaps {
                    for &(at, mask, _) in bits {
                        if let Some(now) = byte(m, at) {
                            if now & mask != 0 && !m.write(at, &[now & !mask]) {
                                errors.push("untouchable: write failed".into());
                            }
                        }
                    }
                }
            }
        }

        let classes: Vec<&str> = active
            .iter()
            .flat_map(|t| t.effects().iter())
            .filter_map(|e| if let Effect::Stock(c) = e { Some(*c) } else { None })
            .collect();
        if !classes.is_empty() {
            match a.inventory() {
                Ok(inv) => {
                    for s in stacks(m, n, inv).into_iter().filter(|s| classes.iter().any(|c| s.class.contains(c))) {
                        // Single items (notes, the compass) are not stock.
                        if s.max < STOCK_FLOOR {
                            continue;
                        }
                        let floor = self.stock.entry(s.item).or_insert(s.count.max(STOCK_FLOOR).min(s.max));
                        if s.count < *floor && !m.write(s.count_at, &floor.to_le_bytes()) {
                            errors.push("stock: write failed".into());
                        }
                    }
                }
                Err(e) => errors.push(format!("inventory: {e}")),
            }
        }
        errors
    }

    /// Put back what the cheats no longer in `keep` overwrote, on the targets still in
    /// play, and forget the rest.
    pub fn release(&mut self, a: &dyn Reach, keep: &[Active]) {
        let (m, n) = (a.memory(), a.names());
        let kept = |f: fn(&Effect) -> bool| keep.iter().any(|t| t.effects().iter().any(f));
        let alive: Vec<u64> = a.enemies();
        // Still that enemy: listed, and of the class it was recorded with.
        let same = |e: u64, class: u64| alive.contains(&e) && mem::read_u64(m, e + CLASS) == Some(class);
        if !kept(|e| matches!(e, Effect::EnemyTime)) {
            for (e, (class, orig)) in self.time.drain() {
                if same(e, class) {
                    if let Some(p) = n.field(m, e, "CustomTimeDilation") {
                        m.write(e + p.offset as u64, &orig.to_le_bytes());
                    }
                }
            }
        }
        if !kept(|e| matches!(e, Effect::EnemyFrail)) {
            for (e, f) in self.frail.drain() {
                // and the Health attribute is still the one the record was taken from
                if same(e, f.class) && f.holds(m) && health_of(m, n, e).map(|(_, at)| at) == Some(f.at) {
                    // never frail's own value back
                    let max = max_of(m, n, f.set).unwrap_or(f.base);
                    let (b, c) = if f.current <= FRAIL { (max, max) } else { (f.base, f.current) };
                    put_pair(m, f.at, b, c);
                }
            }
        }
        // Weaker enemies back to their strength by the ratio they were cut by: a hit taken
        // stays taken (half lost at 10 % is half lost after).
        if !kept(|e| matches!(e, Effect::EnemyHealth)) {
            for (e, (class, base, share)) in self.weaker.drain() {
                if !same(e, class) || share >= 1.0 {
                    continue;
                }
                if let Some((_, at)) = health_of(m, n, e) {
                    if let Some((_, c)) = pair(m, at).filter(|&(_, c)| c > 0.0) {
                        let to = (c / share).min(base);
                        put_pair(m, at, to, to);
                    }
                }
            }
        }
        if !kept(|e| matches!(e, Effect::Stock(_))) {
            self.stock.clear();
        }
        if !kept(|e| matches!(e, Effect::WeaponXp)) {
            self.weapon_xp.clear();
        }
        let hero = a.hero().ok();
        if !kept(|e| matches!(e, Effect::Ghost)) {
            self.release_ghost(a);
        }
        if !kept(|e| matches!(e, Effect::Untouchable)) {
            if let Some((h, bits)) = self.overlaps.take() {
                if Some(h) == hero {
                    for (at, mask, was) in bits {
                        if let Some(now) = byte(m, at) {
                            m.write(at, &[(now & !mask) | (was & mask)]);
                        }
                    }
                }
            }
        }
    }

    /// The ghost's originals back on the hero they were taken from, and the record (in
    /// memory and on disk) gone. Another hero (a new game, another level's pawn) is not
    /// written: its team is its own.
    fn release_ghost(&mut self, a: &dyn Reach) {
        let (m, n) = (a.memory(), a.names());
        if let Some((h, t, f)) = self.team.take() {
            if a.hero().ok() == Some(h) {
                if let Some((ta, fa)) = team_at(m, n, h) {
                    m.write(ta, &[t]);
                    m.write(fa, &[f]);
                }
            }
        }
        save_ghost(None);
    }

    /// The game went away: every target with it.
    pub fn forget(&mut self) {
        self.time.clear();
        self.frail.clear();
        self.weaker.clear();
        self.stock.clear();
        self.weapon_xp.clear();
        self.team = None;
        save_ghost(None);
        self.overlaps = None;
    }

    /// Start from a record a killed panel left (the ghost's originals).
    pub fn new() -> Extras {
        Extras { team: load_ghost(), ..Default::default() }
    }

    /// Write every stack of a class to `v`, each up to its own maximum. How many.
    pub fn set_stock(&mut self, a: &dyn Reach, class: &str, v: u32) -> Result<usize, String> {
        let (m, n) = (a.memory(), a.names());
        let inv = a.inventory()?;
        let mut done = 0;
        for s in stacks(m, n, inv).into_iter().filter(|s| s.class.contains(class)) {
            let v = v.clamp(1, s.max);
            if !m.write(s.count_at, &v.to_le_bytes()) {
                return Err(tr!("WRITE_FAILED").into());
            }
            self.stock.remove(&s.item);
            done += 1;
        }
        if done == 0 {
            return Err(trf!("NO_IN_THE_INVENTORY_PICK_ONE", class = class));
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ghost_record_reads_back() {
        assert_eq!(parse_ghost("1f2e3d4c 1 1\n"), Some((0x1f2e3d4c, 1, 1)));
        assert_eq!(parse_ghost("nonsense"), None);
        assert_eq!(parse_ghost("12 300 1"), None, "not a byte");
    }
    use std::cell::RefCell;

    /// Plain bytes from address 0x1000.
    struct Bytes(RefCell<Vec<u8>>);
    impl Memory for Bytes {
        fn read(&self, at: u64, buf: &mut [u8]) -> bool {
            let b = self.0.borrow();
            let i = (at - 0x1000) as usize;
            if i + buf.len() > b.len() {
                return false;
            }
            buf.copy_from_slice(&b[i..i + buf.len()]);
            true
        }
        fn write(&self, at: u64, data: &[u8]) -> bool {
            let i = (at - 0x1000) as usize;
            self.0.borrow_mut()[i..i + data.len()].copy_from_slice(data);
            true
        }
    }

    fn weapon(level: u32, cap: u32, total: u32, within: u32, next: u32) -> Bytes {
        let mut b = vec![0u8; 0x170];
        for (o, v) in [(W_LEVEL, level), (W_CAP, cap), (W_TOTAL, total), (W_WITHIN, within), (W_NEXT, next)] {
            b[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
        }
        Bytes(RefCell::new(b))
    }

    #[test]
    fn the_sword_as_seen_reads_and_a_capped_weapon_does_not() {
        // The sword before and after the kill, as `doctor watch` saw them.
        assert_eq!(weapon_xp(&weapon(0, 3, 90, 90, 260), 0x1000), Some((90, 90)));
        assert_eq!(weapon_xp(&weapon(1, 3, 265, 5, 520), 0x1000), Some((265, 5)));
        // After a ×3 kill, as seen in play: past the next threshold until the game's
        // next grant levels it — still read, so the next kill is multiplied too.
        assert_eq!(weapon_xp(&weapon(1, 3, 580, 320, 520), 0x1000), Some((580, 320)));
        // The twin axes at their grade's cap: next is 0 — nothing to grow.
        assert_eq!(weapon_xp(&weapon(3, 3, 1560, 0, 0), 0x1000), None);
        // A layout that moved: no cap where one should be.
        assert_eq!(weapon_xp(&weapon(0, 0, 90, 90, 260), 0x1000), None);
    }
}
