//! GUObjectArray: every live UObject, by index. Subsystems and the save state live
//! outside reflection's reach from GEngine, so this is how they are found.
//!
//! `FUObjectArray` sits in the image's writable data: four 32-bit counters, then
//! `FChunkedFixedUObjectArray` — `Objects` (FUObjectItem**), `PreAllocatedObjects`,
//! `MaxElements`, `NumElements`, `MaxChunks`, `NumChunks`. Items are 0x18 bytes with
//! the object first, 64K to a chunk. Found as the one place shaped like that whose
//! first items hold objects whose `InternalIndex` (+0xC) is their own slot.

use crate::mem::{self, Memory};
use crate::names::{self, Names, CLASS};

const ITEM: u64 = 0x18;
const CHUNK: u64 = 64 * 1024;
/// UObjectBase::InternalIndex.
const INDEX: u64 = 0xC;

pub struct Objects {
    /// The `Objects` field: the chunk table pointer's address.
    pub at: u64,
}

/// FUObjectItem::SerialNumber: a slot's object is the same one while its pointer and serial are.
const SERIAL: usize = 0x10;

/// Every live object with its class, kept between readings: a reading reads the slot table (a
/// read a chunk) and the class of only the slots whose object changed. The passes over every
/// object (obstacles, quests) each read every object's class themselves before — 3,000 reads a
/// step between them (measured).
#[derive(Default)]
pub struct Census {
    /// Per slot: the object, its serial, its class (0 when unreadable).
    slots: Vec<(u64, u32, u64)>,
    read: Option<std::time::Instant>,
}

impl Census {
    /// Read the table again if `fresh` has passed since the last reading.
    pub fn refresh(&mut self, m: &dyn Memory, objects: &Objects, fresh: std::time::Duration) {
        if self.read.is_some_and(|t| t.elapsed() < fresh) {
            return;
        }
        self.read = Some(std::time::Instant::now());
        let Some((chunks, num)) = objects.table(m) else { return };
        self.slots.resize(num as usize, (0, 0, 0));
        let paged = mem::Paged::new(m);
        for c in 0..num.div_ceil(CHUNK) {
            let Some(chunk) = mem::read_u64(m, chunks + c * 8).filter(|&p| mem::plausible(p)) else { continue };
            let count = (num - c * CHUNK).min(CHUNK);
            let mut buf = vec![0u8; (count * ITEM) as usize];
            if !m.read(chunk, &mut buf) {
                continue;
            }
            for (i, b) in buf.chunks_exact(ITEM as usize).enumerate() {
                let obj = u64::from_le_bytes(b[..8].try_into().unwrap());
                let serial = u32::from_le_bytes(b[SERIAL..SERIAL + 4].try_into().unwrap());
                let slot = &mut self.slots[(c * CHUNK) as usize + i];
                if slot.0 != obj || slot.1 != serial {
                    let class = if mem::plausible(obj) {
                        mem::read_u64(&paged, obj + CLASS).filter(|&p| mem::plausible(p)).unwrap_or(0)
                    } else {
                        0
                    };
                    *slot = (obj, serial, class);
                }
            }
        }
    }

    /// Every live object and its class, in address order (neighbours share pages).
    pub fn pairs(&self) -> Vec<(u64, u64)> {
        let mut out: Vec<(u64, u64)> =
            self.slots.iter().filter(|s| s.2 != 0 && mem::plausible(s.0)).map(|s| (s.0, s.2)).collect();
        out.sort_unstable();
        out
    }
}

impl Objects {
    fn table(&self, m: &dyn Memory) -> Option<(u64, u64)> {
        let chunks = mem::read_u64(m, self.at).filter(|&p| mem::plausible(p))?;
        let num = mem::read_u32(m, self.at + 0x14)? as u64;
        Some((chunks, num))
    }

    /// Every live object, read a chunk at a time.
    pub fn all(&self, m: &dyn Memory) -> Vec<u64> {
        let Some((chunks, num)) = self.table(m) else { return Vec::new() };
        let mut out = Vec::with_capacity(num as usize);
        for c in 0..num.div_ceil(CHUNK) {
            let Some(chunk) = mem::read_u64(m, chunks + c * 8).filter(|&p| mem::plausible(p)) else { continue };
            let count = (num - c * CHUNK).min(CHUNK);
            let mut buf = vec![0u8; (count * ITEM) as usize];
            if !m.read(chunk, &mut buf) {
                continue;
            }
            out.extend(
                buf.chunks_exact(ITEM as usize)
                    .map(|b| u64::from_le_bytes(b[..8].try_into().unwrap()))
                    .filter(|&o| mem::plausible(o)),
            );
        }
        out
    }

    /// Live objects whose class is exactly `class` (not class default objects).
    pub fn of_class(&self, m: &dyn Memory, n: &Names, class: &str) -> Vec<u64> {
        let mut verdict = std::collections::HashMap::<u64, bool>::new();
        self.all(m)
            .into_iter()
            .filter(|&o| {
                let Some(c) = mem::read_u64(m, o + CLASS).filter(|&p| mem::plausible(p)) else { return false };
                *verdict.entry(c).or_insert_with(|| n.object(m, c).as_deref() == Some(class))
            })
            .filter(|&o| !n.object(m, o).is_some_and(|name| name.starts_with("Default__")))
            .collect()
    }
}

pub fn discover(m: &dyn Memory, base: u64) -> Result<Objects, String> {
    let mut hits = Vec::new();
    for (start, size) in names::writable_sections(m, base)? {
        let mut buf = vec![0u8; size as usize];
        if !m.read(start, &mut buf) {
            continue;
        }
        for i in (0..buf.len().saturating_sub(0x20)).step_by(8) {
            let q = u64::from_le_bytes(buf[i..i + 8].try_into().unwrap());
            let d = |o: usize| u32::from_le_bytes(buf[i + o..i + o + 4].try_into().unwrap()) as u64;
            let (max, num, maxc, numc) = (d(0x10), d(0x14), d(0x18), d(0x1C));
            if !mem::plausible(q) || !(64..=8_000_000).contains(&num) || num > max || numc == 0 || numc > maxc {
                continue;
            }
            if maxc > 1024 || numc != num.div_ceil(CHUNK) {
                continue;
            }
            let Some(chunk0) = mem::read_u64(m, q).filter(|&p| mem::plausible(p)) else { continue };
            let agree = (1..32u64)
                .filter(|&k| {
                    mem::read_u64(m, chunk0 + k * ITEM)
                        .filter(|&o| mem::plausible(o))
                        .and_then(|o| mem::read_u32(m, o + INDEX))
                        == Some(k as u32)
                })
                .count();
            if agree >= 28 {
                hits.push(start + i as u64);
            }
        }
    }
    match hits[..] {
        [at] => Ok(Objects { at }),
        [] => Err("GUObjectArray not found".into()),
        _ => Err(format!("{} GUObjectArray candidates, refusing to guess", hits.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchors::tests::{image, BASE};

    /// The anchors image, plus an object array in its writable section at +0x2C00:
    /// one chunk, 80 objects at 0x6000_0000 + i·0x100, each knowing its index.
    fn world() -> crate::mem::fake::Fake {
        let (m, mut pool) = image();
        let chunks = 0x6100_0000u64;
        let chunk0 = 0x6200_0000u64;
        m.ptr(BASE + 0x2C00, chunks);
        m.put(BASE + 0x2C10, &[128u32, 80, 1, 1].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>());
        m.ptr(chunks, chunk0);
        for i in 0..80u64 {
            let o = 0x6000_0000 + i * 0x100;
            m.put(o, &[0; 0x30]);
            m.put(o + INDEX, &(i as u32).to_le_bytes());
            let mut item = [0u8; 0x18];
            item[..8].copy_from_slice(&o.to_le_bytes());
            m.put(chunk0 + i * ITEM, &item);
        }
        // Object 7 is a CharlieSaveGame.
        pool.class(&m, 0x6000_0700, 0x6300_0000, "CharlieSaveGame", &[]);
        let idx = pool.name(&m, "CharlieSaveGame_1");
        m.put(0x6000_0700 + names::NAME, &idx.to_le_bytes());
        m
    }

    #[test]
    fn finds_the_array_and_objects_by_class() {
        let m = world();
        let o = discover(&m, BASE).unwrap();
        assert_eq!(o.at, BASE + 0x2C00);
        assert_eq!(o.all(&m).len(), 80);
        let n = crate::player::tests::anchors(&m).names;
        assert_eq!(o.of_class(&m, &n, "CharlieSaveGame"), [0x6000_0700]);
    }

    #[test]
    fn the_census_reads_a_class_again_only_when_its_slot_changes() {
        use std::time::Duration;
        let m = world();
        let o = discover(&m, BASE).unwrap();
        let mut c = Census::default();
        c.refresh(&m, &o, Duration::ZERO);
        let class = mem::read_u64(&m, 0x6000_0700 + CLASS).unwrap();
        assert!(c.pairs().contains(&(0x6000_0700, class)));
        // The object's class pointer changes, its slot does not: the census keeps what it read.
        m.ptr(0x6000_0700 + CLASS, 0x6400_0000);
        c.refresh(&m, &o, Duration::ZERO);
        assert!(c.pairs().contains(&(0x6000_0700, class)));
        // A new serial in the slot: another object there, read again.
        m.put(0x6200_0000 + 7 * ITEM + SERIAL as u64, &9u32.to_le_bytes());
        c.refresh(&m, &o, Duration::ZERO);
        assert!(c.pairs().contains(&(0x6000_0700, 0x6400_0000)));
    }
}
