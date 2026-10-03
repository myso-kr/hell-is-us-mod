//! The two things everything else starts from — the name pool and GEngine — found in
//! the running game, never written down.
//!
//! Steam patches this game, and every patch moves both. So instead of a table of
//! offsets per build, both are found the same way every time the tool attaches:
//!
//! - **FNamePool** — the one block in the image's writable data whose first block
//!   starts `None`, `ByteProperty` (names.rs).
//! - **GEngine** — the one global in the image's writable data that points at an
//!   object of class `GameEngine` (or a class below it), not its class default object.
//!
//! Then the FField layout is picked by reading GEngine's class: the layout under
//! which its `GameInstance` property leads to a `GameInstance` is the right one.
//! Anything that is not unique is refused, never guessed.

use crate::mem::{self, Memory};
use crate::names::{self, Layout, Names, LAYOUTS};
use std::collections::HashMap;

pub struct Anchors {
    pub names: Names,
    /// The GEngine global — the address that holds the engine pointer.
    pub gengine: u64,
    pub names_rva: u64,
    pub gengine_rva: u64,
}

impl Anchors {
    pub fn engine(&self, m: &dyn Memory) -> Result<u64, String> {
        mem::read_u64(m, self.gengine).filter(|&p| mem::plausible(p)).ok_or_else(|| tr!("GEngine 이 비어 있음").into())
    }
}

pub fn discover(m: &dyn Memory, base: u64) -> Result<Anchors, String> {
    let names_rva = names::discover(m, base)?;
    let n = Names::new(base + names_rva);
    let gengine = find_engine(m, base, &n)?;
    let engine = mem::read_u64(m, gengine).unwrap_or(0);
    let layout = pick_layout(m, base + names_rva, engine)?;
    Ok(Anchors { names: Names::with_layout(base + names_rva, layout), gengine, names_rva, gengine_rva: gengine - base })
}

/// Every global holding the engine object. Several globals may hold the same engine;
/// two different engines is no answer.
fn find_engine(m: &dyn Memory, base: u64, n: &Names) -> Result<u64, String> {
    let mut verdict: HashMap<u64, bool> = HashMap::new();
    let mut hits: Vec<(u64, u64)> = Vec::new();
    let mut buf = vec![0u8; 0x10000];
    for (start, size) in names::writable_sections(m, base)? {
        let mut at = start;
        while at < start + size {
            let len = (start + size - at).min(buf.len() as u64) as usize;
            let chunk = &mut buf[..len];
            if m.read(at, chunk) {
                for i in (0..len.saturating_sub(7)).step_by(8) {
                    let p = u64::from_le_bytes(chunk[i..i + 8].try_into().unwrap());
                    if !mem::plausible(p) || (base..base + 0x2000_0000).contains(&p) {
                        continue;
                    }
                    let Some(class) = mem::read_u64(m, p + names::CLASS).filter(|&c| mem::plausible(c)) else {
                        continue;
                    };
                    let engine = *verdict.entry(class).or_insert_with(|| {
                        n.lineage(m, class).into_iter().any(|c| n.object(m, c).as_deref() == Some("GameEngine"))
                    });
                    if engine && !n.object(m, p).is_some_and(|name| name.starts_with("Default__")) {
                        hits.push((at + i as u64, p));
                    }
                }
            }
            at += len as u64;
        }
    }
    let mut objects: Vec<u64> = hits.iter().map(|h| h.1).collect();
    objects.sort_unstable();
    objects.dedup();
    match objects[..] {
        [_] => Ok(hits[0].0),
        [] => Err(tr!("게임 이미지에서 GameEngine 을 찾지 못함 (아직 시작 중?)").into()),
        _ => Err(format!("{} different engine objects, refusing to guess", objects.len())),
    }
}

/// The FField layout under which GEngine's `GameInstance` property points at a
/// `GameInstance`.
fn pick_layout(m: &dyn Memory, pool: u64, engine: u64) -> Result<Layout, String> {
    LAYOUTS
        .into_iter()
        .find(|&l| {
            let n = Names::with_layout(pool, l);
            n.follow(m, engine, "GameInstance").is_ok_and(|gi| n.is_a(m, gi, "GameInstance"))
        })
        .ok_or_else(|| tr!("아는 FField 레이아웃으로 GameEngine.GameInstance 를 읽지 못함 — 엔진 구조가 바뀜").into())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::mem::fake::Fake;
    use crate::names::fixture::Pool;

    pub const BASE: u64 = 0x1_4000_0000;
    pub const POOL_RVA: u64 = 0x2000;
    pub const GENGINE_RVA: u64 = 0x2800;
    pub const ENGINE: u64 = 0x2000_0000;
    pub const GAME_INSTANCE: u64 = 0x2100_0000;

    /// A PE image with one writable section (0x2000..0x3000) holding the pool and
    /// GEngine; the engine's `GameInstance` at +0x1D8.
    pub fn image() -> (Fake, Pool) {
        let m = Fake::default();
        m.put(BASE, &[0; 0x400]);
        m.put(BASE + 0x3C, &0x80u32.to_le_bytes());
        m.put(BASE + 0x80, &0x4550u32.to_le_bytes());
        let mut fh = [0u8; 20];
        fh[2..4].copy_from_slice(&1u16.to_le_bytes());
        fh[16..18].copy_from_slice(&0xF0u16.to_le_bytes());
        m.put(BASE + 0x84, &fh);
        let mut sec = [0u8; 40];
        sec[8..12].copy_from_slice(&0x1000u32.to_le_bytes());
        sec[12..16].copy_from_slice(&0x2000u32.to_le_bytes());
        sec[36..40].copy_from_slice(&0xC000_0040u32.to_le_bytes());
        m.put(BASE + 0x80 + 24 + 0xF0, &sec);
        m.put(BASE + 0x2000, &[0; 0x1000]);
        let mut pool = Pool::new(&m, BASE + POOL_RVA, 0x3000_0000);
        m.put(BASE + POOL_RVA + 8, &1u32.to_le_bytes());
        m.put(ENGINE, &[0; 0x200]);
        pool.class(&m, ENGINE, 0x5000_0000, "GameEngine", &[("GameInstance", 0x1D8, 8)]);
        let engine_name = pool.name(&m, "GameEngine_0");
        m.put(ENGINE + names::NAME, &engine_name.to_le_bytes());
        m.put(GAME_INSTANCE, &[0; 0x200]);
        pool.class(&m, GAME_INSTANCE, 0x5100_0000, "CharlieGameInstance", &[]);
        pool.class(&m, 0x2200_0000, 0x5200_0000, "GameInstance", &[]);
        pool.inherit(&m, 0x5100_0000, 0x5200_0000);
        m.ptr(ENGINE + 0x1D8, GAME_INSTANCE);
        m.ptr(BASE + GENGINE_RVA, ENGINE);
        (m, pool)
    }

    #[test]
    fn finds_the_pool_the_engine_and_the_layout() {
        let (m, _) = image();
        let a = discover(&m, BASE).unwrap();
        assert_eq!((a.names_rva, a.gengine_rva), (POOL_RVA, GENGINE_RVA));
        assert_eq!(a.names.layout, LAYOUTS[0]);
        assert_eq!(a.engine(&m), Ok(ENGINE));
    }

    #[test]
    fn the_class_default_object_is_not_the_engine() {
        let (m, mut pool) = image();
        let cdo = 0x2300_0000;
        m.put(cdo, &[0; 0x20]);
        m.ptr(cdo + names::CLASS, 0x5000_0000);
        let idx = pool.name(&m, "Default__GameEngine");
        m.put(cdo + names::NAME, &idx.to_le_bytes());
        m.ptr(BASE + 0x2900, cdo);
        assert_eq!(discover(&m, BASE).unwrap().gengine_rva, GENGINE_RVA);
    }

    #[test]
    fn two_engines_is_no_answer() {
        let (m, _) = image();
        let other = 0x2400_0000;
        m.put(other, &[0; 0x20]);
        m.ptr(other + names::CLASS, 0x5000_0000);
        m.ptr(BASE + 0x2900, other);
        assert!(discover(&m, BASE).is_err());
    }
}
