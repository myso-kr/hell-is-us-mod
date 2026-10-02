//! The way from GEngine to the player — learned from the game's own reflection, then
//! walked as a plain pointer chain.
//!
//! Every link is a property found by name: `GameEngine.GameInstance`,
//! `GameInstance.LocalPlayers[0]`, `Player.PlayerController`, `Controller.Pawn`,
//! then the pawn's one property that holds an `AbilitySystemComponent`, and that
//! component's `SpawnedAttributes`. Learning reads hundreds of fields, so it happens
//! once per attach; every step after that walks the offsets it found.
//!
//! The hero gate: nothing is written unless the controlled pawn is the hero — its
//! class is, or derives from, `CharlieCharacterHero`. In menus, loading screens and
//! anywhere the player is not in control of the hero, the gate is closed.

use crate::anchors::Anchors;
use crate::mem::{self, Memory};
use crate::names::{self, Names};

/// The class the player's pawn must derive from.
pub const HERO: &str = "CharlieCharacterHero";

/// Offsets from the GEngine global, learned once.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    /// From the engine object to the field holding the local PlayerController:
    /// GameInstance, LocalPlayers (data), [0], PlayerController.
    pub to_controller: Vec<u64>,
    /// APlayerController → Pawn.
    pub pawn: u64,
    /// Pawn → its AbilitySystemComponent.
    pub asc: u64,
    /// ASC → SpawnedAttributes (a TArray).
    pub sets: u64,
    /// Pawn → RootComponent; where the player stands is under it.
    pub root: u64,
    /// SceneComponent → RelativeLocation (three doubles).
    pub location: u64,
    /// Controller → ControlRotation (pitch, yaw, roll as doubles).
    pub rotation: u64,
    /// The pawn's class names when learned, nearest first.
    pub classes: Vec<String>,
}

fn offset(n: &Names, m: &dyn Memory, obj: u64, name: &str) -> Result<u64, String> {
    n.field(m, obj, name).map(|p| p.offset as u64).ok_or_else(|| format!("no property {name}"))
}

/// The first element of a `TArray` property, followed.
fn first(n: &Names, m: &dyn Memory, obj: u64, name: &str) -> Result<(u64, u64), String> {
    let off = offset(n, m, obj, name)?;
    let data = mem::read_u64(m, obj + off).filter(|&p| mem::plausible(p)).ok_or(format!("{name} is empty"))?;
    let item = mem::read_u64(m, data).filter(|&p| mem::plausible(p)).ok_or(format!("{name}[0] is empty"))?;
    Ok((off, item))
}

/// The pawn's property that holds an AbilitySystemComponent.
pub fn find_asc(n: &Names, m: &dyn Memory, pawn: u64) -> Result<(u64, u64), String> {
    let class = mem::read_u64(m, pawn + names::CLASS).unwrap_or(0);
    for c in n.lineage(m, class) {
        for p in n.properties(m, c).into_iter().filter(|p| p.size == 8) {
            let Some(v) = mem::read_u64(m, pawn + p.offset as u64).filter(|&v| mem::plausible(v)) else { continue };
            if n.is_a(m, v, "AbilitySystemComponent") {
                return Ok((p.offset as u64, v));
            }
        }
    }
    Err("the pawn holds no AbilitySystemComponent".into())
}

pub fn learn(m: &dyn Memory, a: &Anchors) -> Result<Chain, String> {
    let n = &a.names;
    let engine = a.engine(m)?;
    let gi_off = offset(n, m, engine, "GameInstance")?;
    let gi = n.follow(m, engine, "GameInstance")?;
    let (lp_off, lp) = first(n, m, gi, "LocalPlayers")?;
    let pc_off = offset(n, m, lp, "PlayerController")?;
    let pc = n.follow(m, lp, "PlayerController").map_err(|e| format!("{e} — still in the start-up screens?"))?;
    let pawn_off = offset(n, m, pc, "Pawn")?;
    let pawn = n.follow(m, pc, "Pawn").map_err(|e| format!("{e} — load a save first"))?;
    let classes = n.class_names(m, pawn);
    if !classes.iter().any(|c| c == HERO) {
        return Err(format!("the pawn is a {}, not the hero — load a save first", classes.join(" < ")));
    }
    let (asc_off, asc) = find_asc(n, m, pawn)?;
    let sets = offset(n, m, asc, "SpawnedAttributes")?;
    let root = offset(n, m, pawn, "RootComponent")?;
    let root_obj = n.follow(m, pawn, "RootComponent")?;
    let location = offset(n, m, root_obj, "RelativeLocation")?;
    let rotation = offset(n, m, pc, "ControlRotation")?;
    Ok(Chain {
        to_controller: vec![gi_off, lp_off, 0, pc_off],
        pawn: pawn_off,
        asc: asc_off,
        sets,
        root,
        location,
        rotation,
        classes,
    })
}

impl Chain {
    pub fn controller(&self, m: &dyn Memory, a: &Anchors) -> Result<u64, String> {
        let at = mem::resolve(m, a.gengine, &self.to_controller).map_err(|e| e.to_string())?;
        mem::read_u64(m, at).filter(|&p| mem::plausible(p)).ok_or_else(|| "no player controller".into())
    }

    /// The pawn, if it is the hero — the gate.
    pub fn hero(&self, m: &dyn Memory, a: &Anchors) -> Result<u64, String> {
        let pc = self.controller(m, a)?;
        let pawn = mem::read_u64(m, pc + self.pawn)
            .filter(|&p| mem::plausible(p))
            .ok_or("no pawn — loading, or a cinematic?")?;
        if !a.names.is_a(m, pawn, HERO) {
            return Err("the controlled pawn is not the hero".into());
        }
        Ok(pawn)
    }

    /// The hero's `SpawnedAttributes` TArray.
    pub fn attribute_sets(&self, m: &dyn Memory, a: &Anchors) -> Result<u64, String> {
        let pawn = self.hero(m, a)?;
        let asc = mem::read_u64(m, pawn + self.asc).filter(|&p| mem::plausible(p)).ok_or("no ability system")?;
        Ok(asc + self.sets)
    }

    /// The name of the world the hero is in — the pawn's outer is its level, the
    /// level's outer its world. Trails and markers are kept per world.
    pub fn world(&self, m: &dyn Memory, a: &Anchors) -> Result<String, String> {
        let pawn = self.hero(m, a)?;
        let level = mem::read_u64(m, pawn + names::OUTER).filter(|&p| mem::plausible(p)).ok_or("no level")?;
        let world = mem::read_u64(m, level + names::OUTER).filter(|&p| mem::plausible(p)).ok_or("no world")?;
        a.names.object(m, world).ok_or_else(|| "world name unreadable".into())
    }

    /// Where the hero stands (UE units, centimetres) and which way the camera faces
    /// (yaw, degrees).
    pub fn pose(&self, m: &dyn Memory, a: &Anchors) -> Result<([f64; 3], f64), String> {
        let pc = self.controller(m, a)?;
        let pawn = self.hero(m, a)?;
        let root = mem::read_u64(m, pawn + self.root).filter(|&p| mem::plausible(p)).ok_or("no root component")?;
        let mut b = [0u8; 24];
        if !m.read(root + self.location, &mut b) {
            return Err("location unreadable".into());
        }
        let d = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
        let loc = [d(0), d(1), d(2)];
        let mut r = [0u8; 8];
        if !m.read(pc + self.rotation + 8, &mut r) {
            return Err("rotation unreadable".into());
        }
        let yaw = f64::from_le_bytes(r);
        if loc.iter().chain([&yaw]).any(|v| !v.is_finite()) {
            return Err("pose is not a number".into());
        }
        Ok((loc, yaw))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::anchors::tests::{image, BASE, GAME_INSTANCE};
    use crate::mem::fake::Fake;

    pub const PAWN: u64 = 0x2600_0000;
    pub const ASC: u64 = 0x2700_0000;
    pub const PC: u64 = 0x2500_0000;
    pub const ROOT: u64 = 0x2800_0000;

    /// The anchors fixture, plus a local player, a controller, a hero pawn with an
    /// ability system, and the pawn standing at (100, 200, 300) facing yaw 90.
    pub fn world() -> (Fake, crate::names::fixture::Pool) {
        let (m, mut pool) = image();
        let (lps, lp) = (0x2A00_0000u64, 0x2400_0000u64);
        // GameInstance's own class is the one the anchors fixture made; give it
        // LocalPlayers through a class in between.
        pool.class(&m, 0x2D00_0000, 0x5A00_0000, "GameInstanceWithPlayers", &[("LocalPlayers", 0x38, 16)]);
        pool.inherit(&m, 0x5100_0000, 0x5A00_0000);
        pool.inherit(&m, 0x5A00_0000, 0x5200_0000);
        m.ptr(GAME_INSTANCE + 0x38, lps);
        m.ptr(lps, lp);
        m.put(lp, &[0; 0x100]);
        pool.class(&m, lp, 0x5300_0000, "LocalPlayer", &[("PlayerController", 0x30, 8)]);
        m.ptr(lp + 0x30, PC);
        m.put(PC, &[0; 0x400]);
        pool.class(
            &m,
            PC,
            0x5400_0000,
            "CharliePlayerController",
            &[("Pawn", 0x2F8, 8), ("ControlRotation", 0x320, 24)],
        );
        m.ptr(PC + 0x2F8, PAWN);
        m.put(PC + 0x320, &[0.0f64, 90.0, 0.0].map(f64::to_le_bytes).concat());
        m.put(PAWN, &[0; 0x800]);
        pool.class(&m, PAWN, 0x5500_0000, "StoryHero_BP_C", &[("Mesh", 0x300, 8), ("Ability", 0x688, 8)]);
        pool.class(&m, 0x2B00_0000, 0x5600_0000, HERO, &[("RootComponent", 0x1A0, 8)]);
        pool.inherit(&m, 0x5500_0000, 0x5600_0000);
        m.ptr(PAWN + 0x688, ASC);
        m.ptr(PAWN + 0x1A0, ROOT);
        m.put(ROOT, &[0; 0x200]);
        pool.class(&m, ROOT, 0x5700_0000, "CapsuleComponent", &[("RelativeLocation", 0x128, 24)]);
        m.put(ROOT + 0x128, &[100.0f64, 200.0, 300.0].map(f64::to_le_bytes).concat());
        m.put(ASC, &[0; 0x1100]);
        pool.class(&m, ASC, 0x5800_0000, "CharlieHeroAbilitySystemComponent", &[("SpawnedAttributes", 0x1088, 16)]);
        pool.class(&m, 0x2C00_0000, 0x5900_0000, "AbilitySystemComponent", &[]);
        pool.inherit(&m, 0x5800_0000, 0x5900_0000);
        (m, pool)
    }

    pub fn anchors(m: &Fake) -> Anchors {
        crate::anchors::discover(m, BASE).unwrap()
    }

    #[test]
    fn learns_the_chain_by_name() {
        let (m, _) = world();
        let a = anchors(&m);
        let c = learn(&m, &a).unwrap();
        assert_eq!(c.to_controller, [0x1D8, 0x38, 0, 0x30]);
        assert_eq!((c.pawn, c.asc, c.sets), (0x2F8, 0x688, 0x1088));
        assert_eq!(c.attribute_sets(&m, &a), Ok(ASC + 0x1088));
        assert_eq!(c.pose(&m, &a), Ok(([100.0, 200.0, 300.0], 90.0)));
    }

    #[test]
    fn names_the_world_through_two_outers() {
        let (m, mut pool) = world();
        let a = anchors(&m);
        let c = learn(&m, &a).unwrap();
        let (level, w) = (0x2E00_0000u64, 0x2F00_0000u64);
        m.put(level, &[0; 0x30]);
        m.put(w, &[0; 0x30]);
        m.ptr(PAWN + names::OUTER, level);
        m.ptr(level + names::OUTER, w);
        let idx = pool.name(&m, "Map_Forest_P");
        m.put(w + names::NAME, &idx.to_le_bytes());
        assert_eq!(c.world(&m, &a).as_deref(), Ok("Map_Forest_P"));
    }

    #[test]
    fn the_gate_closes_when_the_pawn_is_not_the_hero() {
        let (m, _) = world();
        let a = anchors(&m);
        let c = learn(&m, &a).unwrap();
        m.ptr(PAWN + names::CLASS, 0x5900_0000); // now "an AbilitySystemComponent"
        assert!(c.hero(&m, &a).is_err());
        m.ptr(PC + 0x2F8, 0);
        assert!(c.hero(&m, &a).is_err());
    }

    #[test]
    fn will_not_learn_from_a_non_hero() {
        let (m, _) = world();
        m.ptr(PAWN + names::CLASS, 0x5900_0000);
        let a = anchors(&m);
        assert!(learn(&m, &a).is_err());
    }
}
