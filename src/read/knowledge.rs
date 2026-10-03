//! What the hero knows: the facts and tags they have, and the investigations open —
//! read from the current save state (.spec/GUIDE.md §2).
//!
//! The game keeps one `CharlieSaveGame` per slot; the one with the latest
//! `SaveDate` is the playthrough in progress. Under `Player`:
//! `Knowledge.KnownFacts` (soft paths to FactData assets), `Knowledge.FactTags`
//! (a GameplayTagContainer) and `Datums.QuestStates` (soft paths to QuestData).
//!
//! Everything is kept as FName indices: an asset's soft path names it by the same
//! FName its object carries, so "is this fact known" is a set lookup.

use crate::mem::{self, Memory};
use crate::names::Names;
use std::collections::HashSet;

/// FSoftObjectPtr: a weak pointer (8), then FSoftObjectPath's package FName (8) and
/// asset FName — whose comparison index is here.
const SOFT_ASSET: u64 = 0x10;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Knowledge {
    pub facts: HashSet<u32>,
    pub tags: HashSet<u32>,
    /// Open investigations, by their QuestData's name index — and readable.
    pub quests: HashSet<u32>,
    pub quest_names: Vec<String>,
}

/// A TArray's data and count, at `at`.
fn array(m: &dyn Memory, at: u64) -> Option<(u64, u64)> {
    let data = mem::read_u64(m, at)?;
    let num = mem::read_u32(m, at + 8)? as u64;
    (num == 0 || (mem::plausible(data) && num < 100_000)).then_some((data, num))
}

/// The element size of the array property at `path` under `obj`.
fn element_size(n: &Names, m: &dyn Memory, field: u64) -> Option<u64> {
    let inner = n.inner_of(m, field)?;
    mem::read_u32(m, inner + n.layout.size).map(|s| s as u64)
}

/// The name indices found at `offset` in each element of the array at `path`.
fn names_in(n: &Names, m: &dyn Memory, obj: u64, path: &[&str], offset: u64) -> Option<Vec<u32>> {
    let (at, p) = n.path(m, obj, path)?;
    let size = element_size(n, m, p.field)?;
    let (data, num) = array(m, at)?;
    if num == 0 {
        return Some(Vec::new());
    }
    let mut buf = vec![0u8; (num * size) as usize];
    m.read(data, &mut buf).then_some(())?;
    Some(
        buf.chunks_exact(size as usize)
            .map(|e| u32::from_le_bytes(e[offset as usize..offset as usize + 4].try_into().unwrap()))
            .filter(|&i| i != 0)
            .collect(),
    )
}

/// The save in progress among the slots: the latest `SaveDate`.
pub fn current(n: &Names, m: &dyn Memory, saves: &[u64]) -> Option<u64> {
    saves
        .iter()
        .filter_map(|&s| {
            let (at, _) = n.path(m, s, &["SaveDate"])?;
            Some((mem::read_u64(m, at)?, s))
        })
        .max()
        .map(|(_, s)| s)
}

pub fn read(n: &Names, m: &dyn Memory, save: u64) -> Option<Knowledge> {
    let facts = names_in(n, m, save, &["Player", "Knowledge", "KnownFacts"], SOFT_ASSET)?;
    let tags = names_in(n, m, save, &["Player", "Knowledge", "FactTags", "GameplayTags"], 0)?;
    let quests = names_in(n, m, save, &["Player", "Datums", "QuestStates"], SOFT_ASSET).unwrap_or_default();
    Some(Knowledge {
        facts: facts.into_iter().collect(),
        tags: tags.into_iter().collect(),
        quest_names: quests.iter().filter_map(|&i| n.get(m, i)).collect(),
        quests: quests.into_iter().collect(),
    })
}
