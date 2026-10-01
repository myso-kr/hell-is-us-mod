//! The local player's GAS attributes — found by name, and refused anything that does
//! not look like one.
//!
//! Each attribute set is a UObject whose class lists its attributes as properties.
//! `Session::open` walks those lists once: set class `ATR_Loot`, property
//! `LootingMultiplier` at +0x90, element size 16. Cheats name what they write, and
//! the names are resolved against the game itself — so a reordered set list, or a
//! set that gained an attribute in an update, moves nothing that matters.
//!
//! Two checks then guard every write. The property must be 16 bytes — the size of an
//! `FGameplayAttributeData`: vtable, padding, BaseValue, CurrentValue. And the bytes
//! at the target must start with the one vtable every attribute in the process
//! shares, established by agreement among all of them.

use crate::mem::{self, Memory};
use crate::names::Names;
use std::collections::HashMap;

/// An attribute, by the game's own names: set class, then property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Attr {
    pub set: &'static str,
    pub name: &'static str,
}

/// The set name that matches any set (`Session::locate`).
pub const ANY: &str = "*";

pub const fn attr(set: &'static str, name: &'static str) -> Attr {
    Attr { set, name }
}

const BASE: u64 = 0x8;
const CURRENT: u64 = 0xC;
/// sizeof(FGameplayAttributeData).
const ATTRIBUTE_SIZE: u32 = 0x10;

/// One attribute set as the game has it right now.
pub struct Set {
    pub object: u64,
    pub class: String,
    /// Every attribute in it: name → offset into the set.
    pub attributes: Vec<(String, u32)>,
}

/// The attribute sets of the pawn being played right now. Resolve a fresh one for
/// every action: the pawn — and everything under it — is replaced on each level load.
pub struct Session<'a> {
    m: &'a dyn Memory,
    pub sets: Vec<Set>,
    index: HashMap<(String, String), u64>,
    /// The one `FGameplayAttributeData` vtable.
    pub vtable: u64,
    /// Plain float properties that are not attributes — the movement component's
    /// `MaxWalkSpeed`, the hero's `CustomTimeDilation`. Same (set, name) keys, with
    /// a label for the owner in place of a set (`add_fields`).
    fields: HashMap<(String, String), u64>,
}

impl<'a> Session<'a> {
    /// `module` is the game's image range: the attribute vtable must live in it.
    /// `arr` is the `SpawnedAttributes` TArray (player.rs).
    pub fn open(m: &'a dyn Memory, module: (u64, u64), names: &Names, arr: u64) -> Result<Session<'a>, String> {
        let (base, size) = module;
        let data = mem::read_u64(m, arr).ok_or("attribute set array unreadable")?;
        let num = mem::read_u32(m, arr + 8).ok_or("attribute set count unreadable")?;
        if !(1..=64).contains(&num) || !mem::plausible(data) {
            return Err(format!("attribute set array looks wrong (num={num}, data=0x{data:X})"));
        }

        let mut sets = Vec::with_capacity(num as usize);
        let mut index = HashMap::new();
        for i in 0..num as u64 {
            let Some(object) = mem::read_u64(m, data + i * 8).filter(|&p| mem::plausible(p)) else { continue };
            let Some(class) = names.class(m, object) else { continue };
            let class_obj = mem::read_u64(m, object + crate::names::CLASS).unwrap_or(0);
            let attributes: Vec<(String, u32)> = names
                .properties(m, class_obj)
                .into_iter()
                .filter(|p| p.size == ATTRIBUTE_SIZE)
                .map(|p| (p.name, p.offset))
                .collect();
            for (name, offset) in &attributes {
                index.insert((class.clone(), name.clone()), object + *offset as u64);
            }
            sets.push(Set { object, class, attributes });
        }
        if index.is_empty() {
            return Err("no attribute could be named — the name pool or class layout does not fit this build".into());
        }

        let vtable = consensus(m, index.values().copied(), base, size)?;
        Ok(Session { m, sets, index, vtable, fields: HashMap::new() })
    }

    /// Where an attribute is. A set of `*` means whichever set has one by that name —
    /// exactly one, or it is refused.
    fn locate(&self, a: Attr) -> Result<u64, String> {
        if a.set != ANY {
            return self
                .index
                .get(&(a.set.to_string(), a.name.to_string()))
                .copied()
                .ok_or_else(|| format!("{}.{} is not in the game", a.set, a.name));
        }
        let hits: Vec<(&String, u64)> =
            self.index.iter().filter(|((_, n), _)| n == a.name).map(|((set, _), at)| (set, *at)).collect();
        match hits[..] {
            [(_, at)] => Ok(at),
            [] => Err(format!("{} is not in the game", a.name)),
            _ => Err(format!(
                "{} is in {} sets ({}) — name the set",
                a.name,
                hits.len(),
                hits.iter().map(|h| h.0.as_str()).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    /// The set that holds an attribute — for `*` attributes, the one it resolved to.
    pub fn set_of(&self, a: Attr) -> Option<&str> {
        let at = self.locate(a).ok()?;
        self.index.iter().find(|(_, v)| **v == at).map(|((set, _), _)| set.as_str())
    }

    fn checked(&self, a: Attr) -> Result<u64, String> {
        let at = self.locate(a)?;
        match mem::read_u64(self.m, at) {
            Some(v) if v == self.vtable => Ok(at),
            _ => Err(format!("{}.{} is not an attribute — refusing", a.set, a.name)),
        }
    }

    /// Add plain float properties of `obj`, under `label`. Only 4-byte properties are
    /// taken; the caller has checked what `obj` is (its class), since there is no
    /// vtable to check a plain float by.
    pub fn add_fields(&mut self, names: &Names, label: &str, obj: u64, wanted: &[&str]) {
        for name in wanted {
            if let Some(p) = names.field(self.m, obj, name).filter(|p| p.size == 4) {
                self.fields.insert((label.to_string(), name.to_string()), obj + p.offset as u64);
            }
        }
    }

    fn field(&self, a: Attr) -> Option<u64> {
        self.fields.get(&(a.set.to_string(), a.name.to_string())).copied()
    }

    pub fn has(&self, a: Attr) -> bool {
        self.field(a).is_some() || self.checked(a).is_ok()
    }

    /// (BaseValue, CurrentValue). A plain field reads as the same value twice.
    pub fn get(&self, a: Attr) -> Result<(f32, f32), String> {
        if let Some(at) = self.field(a) {
            let v = mem::read_f32(self.m, at).ok_or("field unreadable")?;
            return Ok((v, v));
        }
        let at = self.checked(a)?;
        let b = mem::read_f32(self.m, at + BASE);
        let c = mem::read_f32(self.m, at + CURRENT);
        b.zip(c).ok_or_else(|| "attribute unreadable".into())
    }

    pub fn current(&self, a: Attr) -> Result<f32, String> {
        self.get(a).map(|(_, c)| c)
    }

    /// Write both values. Writing only CurrentValue lasts until the next time GAS
    /// recomputes the attribute from its base, which is usually the next frame.
    pub fn put(&self, a: Attr, v: f32) -> Result<(), String> {
        self.put_pair(a, v, v)
    }

    pub fn put_pair(&self, a: Attr, base: f32, current: f32) -> Result<(), String> {
        if !base.is_finite() || !current.is_finite() {
            return Err("refusing a non-finite value".into());
        }
        if let Some(at) = self.field(a) {
            return if mem::write_f32(self.m, at, current) { Ok(()) } else { Err("write failed".into()) };
        }
        let at = self.checked(a)?;
        if mem::write_f32(self.m, at + BASE, base) && mem::write_f32(self.m, at + CURRENT, current) {
            Ok(())
        } else {
            Err("write failed".into())
        }
    }

    /// An attribute given by runtime names — for `list` and `get`.
    pub fn read_named(&self, set: &str, name: &str) -> Option<(f32, f32)> {
        let at = *self.index.get(&(set.to_string(), name.to_string()))?;
        if mem::read_u64(self.m, at)? != self.vtable {
            return None;
        }
        mem::read_f32(self.m, at + BASE).zip(mem::read_f32(self.m, at + CURRENT))
    }
}

/// The vtable at least four in five attributes agree on, provided it lives in the
/// game's own image. Anything less means these are not attributes.
fn consensus(m: &dyn Memory, at: impl Iterator<Item = u64>, base: u64, size: u64) -> Result<u64, String> {
    let mut seen: HashMap<u64, usize> = HashMap::new();
    let mut total = 0;
    for a in at {
        total += 1;
        if let Some(v) = mem::read_u64(m, a) {
            *seen.entry(v).or_default() += 1;
        }
    }
    let (vt, n) = seen.into_iter().max_by_key(|&(_, n)| n).ok_or("no attribute could be read")?;
    if n * 5 < total * 4 || !(base..base + size).contains(&vt) {
        return Err(format!("attributes do not agree on a vtable ({n}/{total} on 0x{vt:X})"));
    }
    Ok(vt)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::mem::fake::Fake;
    use crate::names::fixture::Pool;

    pub const MODULE: (u64, u64) = (0x1_4000_0000, 0x1000_0000);
    pub const VT: u64 = 0x1_4800_0000;
    const POOL_RVA: u64 = 0x2000;

    pub fn names() -> Names {
        Names::new(MODULE.0 + POOL_RVA)
    }

    /// Where `world` keeps the attribute set array.
    pub const ARR: u64 = 0x2000_0010;

    pub fn open(m: &Fake) -> Result<Session<'_>, String> {
        Session::open(m, MODULE, &names(), ARR)
    }

    pub const A: Attr = attr("ATR_A", "Alpha");
    pub const B: Attr = attr("ATR_A", "Beta");
    pub const C: Attr = attr("ATR_B", "Gamma");
    pub const D: Attr = attr("ATR_B", "Delta");

    /// At `ARR`, the TArray of two sets, ATR_A {Alpha, Beta} and
    /// ATR_B {Gamma, Delta}, at 0x90 and 0xA0, every value (1.0, 2.0).
    pub fn world() -> Fake {
        let m = Fake::default();
        let mut pool = Pool::new(&m, MODULE.0 + POOL_RVA, 0x3000_0000);
        m.ptr(MODULE.0 + 0x100, 0x2000_0000);
        m.ptr(0x2000_0010, 0x3100_0000);
        m.put(0x2000_0018, &2u32.to_le_bytes());
        let sets: [(&str, u64, [&str; 2]); 2] =
            [("ATR_A", 0x4000_0000, ["Alpha", "Beta"]), ("ATR_B", 0x5000_0000, ["Gamma", "Delta"])];
        for (i, (class, object, attrs)) in sets.into_iter().enumerate() {
            m.ptr(0x3100_0000 + i as u64 * 8, object);
            let class_obj = 0x6000_0000 + i as u64 * 0x10_0000;
            pool.class(
                &m,
                object,
                class_obj,
                class,
                &[(attrs[0], 0x90, 16), (attrs[1], 0xA0, 16), ("Padding", 0xB0, 4)],
            );
            for data in [0x90, 0xA0] {
                m.ptr(object + data, VT);
                m.f32(object + data + 8, 1.0);
                m.f32(object + data + 12, 2.0);
            }
        }
        m
    }

    #[test]
    fn finds_attributes_by_name_and_writes_both_values() {
        let m = world();
        let s = open(&m).unwrap();
        assert_eq!(s.get(D), Ok((1.0, 2.0)));
        s.put(D, 50.0).unwrap();
        assert_eq!(s.get(D), Ok((50.0, 50.0)));
        assert_eq!(s.get(C), Ok((1.0, 2.0)), "the neighbour is untouched");
    }

    #[test]
    fn a_property_that_is_not_attribute_sized_is_not_an_attribute() {
        let m = world();
        let s = open(&m).unwrap();
        assert!(s.get(attr("ATR_A", "Padding")).is_err());
        assert!(s.get(attr("ATR_A", "Nope")).is_err());
        assert!(s.get(attr("ATR_Z", "Alpha")).is_err());
    }

    #[test]
    fn refuses_a_target_that_does_not_start_with_the_vtable() {
        let m = world();
        let s = open(&m).unwrap();
        m.ptr(0x4000_0090, 0xDEAD_0000);
        assert!(s.put(A, 1.0).is_err());
    }

    #[test]
    fn refuses_to_open_when_attributes_disagree() {
        let m = world();
        m.ptr(0x4000_0090, 0x1_4900_0000);
        m.ptr(0x5000_0090, 0x1_4900_0001);
        assert!(open(&m).is_err());
    }

    #[test]
    fn refuses_a_vtable_outside_the_game_image() {
        let m = world();
        for at in [0x4000_0090u64, 0x4000_00A0, 0x5000_0090, 0x5000_00A0] {
            m.ptr(at, 0x7FF0_0000_0000);
        }
        assert!(open(&m).is_err());
    }

    #[test]
    fn any_set_finds_a_unique_name_and_refuses_a_shared_one() {
        let m = world();
        let s = open(&m).unwrap();
        assert_eq!(s.get(attr(ANY, "Gamma")), Ok((1.0, 2.0)));
        assert_eq!(s.set_of(attr(ANY, "Gamma")), Some("ATR_B"));
        assert!(s.get(attr(ANY, "Nope")).is_err());
    }

    #[test]
    fn plain_fields_read_and_write_as_one_value() {
        let m = world();
        let mut s = open(&m).unwrap();
        // ATR_A's "Padding" is 4 bytes at 0xB0 — a plain field, not an attribute.
        m.f32(0x4000_00B0, 450.0);
        s.add_fields(&names(), "Movement", 0x4000_0000, &["Padding", "Alpha", "Nope"]);
        let f = attr("Movement", "Padding");
        assert!(s.has(f));
        assert!(!s.has(attr("Movement", "Alpha")), "16 bytes is not a plain float");
        assert_eq!(s.get(f), Ok((450.0, 450.0)));
        s.put(f, 900.0).unwrap();
        assert_eq!(s.get(f), Ok((900.0, 900.0)));
        assert!(s.put(f, f32::INFINITY).is_err());
    }

    #[test]
    fn refuses_non_finite_values() {
        let m = world();
        let s = open(&m).unwrap();
        assert!(s.put(A, f32::NAN).is_err());
    }
}
