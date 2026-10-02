//! Cheats whose targets are not the hero: the enemies in play and the inventory
//! (.spec/CHEATS-RESEARCH.md §4 A).
//!
//! The hero's attributes have one owner and a record on disk (hold.rs). These have
//! many owners that come and go — an enemy spawns, dies, streams out — so what they
//! overwrite is remembered here, per target, in memory, and put back to the targets
//! still in play when the cheat stops. A killed panel leaves enemies as they were
//! last written until they respawn; nothing here outlives them.
//!
//! - Enemy time: each enemy's `CustomTimeDilation` (the hero's own speed-up, turned on
//!   the enemies).
//! - Frail enemies: each enemy's `HealthAttributeSet.Health` held at 1 — one blow kills.
//!   Enemies keep health there, not in the hero's Endurance (probed on 24045435).
//! - Stock: inventory stacks of a class held at no less than when first seen, and
//!   never below 2 — a stack that reaches 0 is removed from the inventory before it
//!   could be put back.
//! - Set stock: a class's stacks written once, up to the item's `QuantityMax`.

use crate::cheats::{Active, Effect};
use crate::engine::Attached;
use crate::mem::{self, Memory};
use crate::names::{Names, CLASS};
use std::collections::HashMap;

/// A stack held by `Stock` never drops below this.
const STOCK_FLOOR: u32 = 2;
/// Frail enemies are held at this much health.
const FRAIL: f32 = 1.0;

#[derive(Default)]
pub struct Extras {
    /// Enemy → its time dilation before the cheat.
    time: HashMap<u64, f32>,
    /// Enemy → (its Health attribute's address, BaseValue, CurrentValue before).
    frail: HashMap<u64, (u64, f32, f32)>,
    /// Inventory stack → the least it is held at.
    stock: HashMap<u64, u32>,
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
fn health_of(m: &dyn Memory, n: &Names, enemy: u64) -> Option<u64> {
    let (_, asc) = crate::player::find_asc(n, m, enemy).ok()?;
    let sets = n.field(m, asc, "SpawnedAttributes")?;
    for set in crate::actors::array(m, asc + sets.offset as u64, 64) {
        if mem::read_u64(m, set + CLASS).and_then(|c| n.object(m, c)).as_deref() == Some("HealthAttributeSet") {
            return n.field(m, set, "Health").map(|p| set + p.offset as u64);
        }
    }
    None
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
    pub fn tick(&mut self, a: &Attached, active: &[Active]) -> Vec<String> {
        let (m, n) = (&a.game, &a.anchors.names);
        let mut errors = Vec::new();
        let wants = |f: fn(&Effect) -> bool| active.iter().find(|t| t.effects().iter().any(f));
        let enemies = || a.enemies();

        if let Some(t) = wants(|e| matches!(e, Effect::EnemyTime)) {
            for e in enemies() {
                let Some(p) = n.field(m, e, "CustomTimeDilation") else { continue };
                let at = e + p.offset as u64;
                if let std::collections::hash_map::Entry::Vacant(v) = self.time.entry(e) {
                    match mem::read_f32(m, at) {
                        Some(orig) => {
                            v.insert(orig);
                        }
                        None => continue,
                    }
                }
                if !m.write(at, &t.value.to_le_bytes()) {
                    errors.push("enemy time: write failed".into());
                }
            }
        }

        if wants(|e| matches!(e, Effect::EnemyFrail)).is_some() {
            for e in enemies() {
                let at = match self.frail.get(&e) {
                    Some(&(at, ..)) => at,
                    None => {
                        let Some(at) = health_of(m, n, e) else { continue };
                        let Some((b, c)) = pair(m, at) else { continue };
                        self.frail.insert(e, (at, b, c));
                        at
                    }
                };
                if pair(m, at).is_some_and(|(_, c)| c > FRAIL) && !put_pair(m, at, FRAIL, FRAIL) {
                    errors.push("frail enemies: write failed".into());
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
    pub fn release(&mut self, a: &Attached, keep: &[Active]) {
        let (m, n) = (&a.game, &a.anchors.names);
        let kept = |f: fn(&Effect) -> bool| keep.iter().any(|t| t.effects().iter().any(f));
        let alive: Vec<u64> = a.enemies();
        if !kept(|e| matches!(e, Effect::EnemyTime)) {
            for (e, orig) in self.time.drain() {
                if alive.contains(&e) {
                    if let Some(p) = n.field(m, e, "CustomTimeDilation") {
                        m.write(e + p.offset as u64, &orig.to_le_bytes());
                    }
                }
            }
        }
        if !kept(|e| matches!(e, Effect::EnemyFrail)) {
            for (e, (at, b, c)) in self.frail.drain() {
                if alive.contains(&e) {
                    put_pair(m, at, b, c);
                }
            }
        }
        if !kept(|e| matches!(e, Effect::Stock(_))) {
            self.stock.clear();
        }
    }

    /// The game went away: every target with it.
    pub fn forget(&mut self) {
        self.time.clear();
        self.frail.clear();
        self.stock.clear();
    }

    /// Write every stack of a class to `v`, each up to its own maximum. How many.
    pub fn set_stock(&mut self, a: &Attached, class: &str, v: u32) -> Result<usize, String> {
        let (m, n) = (&a.game, &a.anchors.names);
        let inv = a.inventory()?;
        let mut done = 0;
        for s in stacks(m, n, inv).into_iter().filter(|s| s.class.contains(class)) {
            let v = v.clamp(1, s.max);
            if !m.write(s.count_at, &v.to_le_bytes()) {
                return Err("write failed".into());
            }
            self.stock.remove(&s.item);
            done += 1;
        }
        if done == 0 {
            return Err(format!("no {class} in the inventory — pick one up first"));
        }
        Ok(done)
    }
}
