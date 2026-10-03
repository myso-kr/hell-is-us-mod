//! The game's own tables the guide reads (`Mods\survey`, written by tools/survey
//! `--tables` or a full survey; .spec/GUIDE.md §27):
//! - `spawners.json` — every world's spawners: the "every Hollow" achievement counts
//!   them (F8). A spawner the save keeps a state for (`World.RegionStates`, by its
//!   GUID) has been beaten — checked against the live enemies in play.
//! - `vaults.json` — the six Vaults of Forbidden Knowledge: name, region, clue, the
//!   research entries that unlock them and their four-symbol code (F7).

use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Spawner {
    pub guid: String,
    pub timeloop: Option<String>,
    pub enemies: usize,
    pub at: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vault {
    pub guid: String,
    /// `namespace/key` of the game's texts.
    pub name: String,
    pub region: String,
    pub clue: String,
    pub entries: u32,
    pub code: Vec<u8>,
}

#[derive(Default, Debug)]
pub struct Tables {
    pub spawners: BTreeMap<String, Vec<Spawner>>,
    pub vaults: Vec<Vault>,
}

/// One world's Hollows: spawners and enemies left of all, per timeloop too, and where
/// the left ones are.
#[derive(Clone, Debug, PartialEq)]
pub struct Hollows {
    pub world: String,
    pub left: usize,
    pub all: usize,
    pub enemies_left: usize,
    /// (timeloop actor id, left, all).
    pub timeloops: Vec<(String, usize, usize)>,
    pub places: Vec<[f32; 3]>,
}

/// Where a vault stands with the hero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultState {
    /// Not yet told of: the research it waits on.
    Locked,
    /// Its clue and code are in the datapad.
    Known,
    Opened,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VaultNote {
    pub vault: Vault,
    pub state: VaultState,
    /// Its door, from the survey: (world, where).
    pub door: Option<(String, [f32; 3])>,
}

fn at(v: &Value) -> Option<[f32; 3]> {
    let a = v.as_array()?;
    (a.len() == 3).then(|| [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.0) as f32))
}

impl Tables {
    pub fn load(dir: &Path) -> Tables {
        let read =
            |f: &str| std::fs::read_to_string(dir.join(f)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
        let mut t = Tables::default();
        if let Some(Value::Object(worlds)) = read("spawners.json") {
            for (world, list) in worlds {
                let list = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| {
                        Some(Spawner {
                            guid: s["guid"].as_str()?.to_string(),
                            timeloop: s["timeloop"].as_str().map(str::to_string),
                            enemies: s["entities"].as_array().map_or(1, Vec::len),
                            at: at(&s["at"])?,
                        })
                    })
                    .collect();
                t.spawners.insert(world, list);
            }
        }
        if let Some(Value::Array(vaults)) = read("vaults.json") {
            t.vaults = vaults
                .iter()
                .filter_map(|v| {
                    Some(Vault {
                        guid: v["guid"].as_str()?.to_string(),
                        name: v["name"].as_str().unwrap_or_default().to_string(),
                        region: v["region"].as_str().unwrap_or_default().to_string(),
                        clue: v["clue"].as_str().unwrap_or_default().to_string(),
                        entries: v["entries"].as_u64().unwrap_or(0) as u32,
                        code: v["code"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|c| c.as_u64())
                            .map(|c| c as u8)
                            .collect(),
                    })
                })
                .collect();
        }
        t
    }

    pub fn is_empty(&self) -> bool {
        self.spawners.is_empty() && self.vaults.is_empty()
    }

    /// Every world's Hollows left, by the GUIDs the save keeps.
    pub fn hollows(&self, saved: &HashSet<String>) -> Vec<Hollows> {
        self.spawners
            .iter()
            .map(|(world, list)| {
                let left: Vec<&Spawner> = list.iter().filter(|s| !saved.contains(&s.guid)).collect();
                let mut loops: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
                for s in list {
                    if let Some(t) = &s.timeloop {
                        let e = loops.entry(t).or_default();
                        e.1 += 1;
                        if !saved.contains(&s.guid) {
                            e.0 += 1;
                        }
                    }
                }
                Hollows {
                    world: world.clone(),
                    left: left.len(),
                    all: list.len(),
                    enemies_left: left.iter().map(|s| s.enemies).sum(),
                    timeloops: loops.into_iter().map(|(t, (l, a))| (t.to_string(), l, a)).collect(),
                    places: left.iter().map(|s| s.at).collect(),
                }
            })
            .collect()
    }

    /// The vaults as the research state has them: shown in the datapad, opened (by
    /// GUID), or due by the research entries known (`lore`); `doors` — each world's
    /// vault door.
    pub fn vaults(
        &self,
        known: &HashSet<String>,
        opened: &HashSet<String>,
        lore: usize,
        doors: &[(String, [f32; 3])],
    ) -> Vec<VaultNote> {
        self.vaults
            .iter()
            .map(|v| {
                let state = if opened.contains(&v.guid) {
                    VaultState::Opened
                } else if known.contains(&v.guid) || v.entries > 0 && lore >= v.entries as usize {
                    VaultState::Known
                } else {
                    VaultState::Locked
                };
                // The region's key names the world (`…Universal_Location_SenedraForest`).
                let world = v.region.rsplit("Universal_Location_").next().unwrap_or_default();
                let door = doors.iter().find(|(w, _)| w == world).cloned();
                VaultNote { vault: v.clone(), state, door }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Tables {
        // One folder per call: the tests run at once.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("hiumod-tables-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spawners.json"),
            r#"{"Talju":[{"guid":"A","timeloop":"Talju_TimeLoop_A_BP","entities":["x","y"],"at":[1,2,3]},
                        {"guid":"B","timeloop":null,"entities":["z"],"at":[4,5,6]}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("vaults.json"),
            r#"[{"guid":"V1","name":"UI_Research_ST/Research_Cache_VOFK01_Name","region":"Facts_Shared/Universal_Location_SenedraForest",
                 "clue":"UI_Research_ST/Research_Cache_VOFK01_Clue","entries":6,"code":[7,1,6,2]}]"#,
        )
        .unwrap();
        let t = Tables::load(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        t
    }

    #[test]
    fn hollows_left_are_the_spawners_the_save_has_no_state_for() {
        let t = tables();
        let saved: HashSet<String> = ["A".to_string()].into();
        let h = &t.hollows(&saved)[0];
        assert_eq!((h.left, h.all, h.enemies_left), (1, 2, 1));
        assert_eq!(h.timeloops, vec![("Talju_TimeLoop_A_BP".to_string(), 0, 1)]);
        assert_eq!(h.places, vec![[4.0, 5.0, 6.0]]);
    }

    #[test]
    fn vaults_are_locked_known_or_opened_with_their_door() {
        let t = tables();
        let doors = vec![("SenedraForest".to_string(), [1.0, 2.0, 3.0])];
        let none = HashSet::new();
        let v = t.vaults(&none, &none, 0, &doors);
        assert_eq!(
            (v[0].state, v[0].door.clone()),
            (VaultState::Locked, Some(("SenedraForest".to_string(), [1.0, 2.0, 3.0])))
        );
        assert_eq!(v[0].vault.code, vec![7, 1, 6, 2]);
        let known: HashSet<String> = ["V1".to_string()].into();
        assert_eq!(t.vaults(&known, &none, 0, &doors)[0].state, VaultState::Known);
        assert_eq!(t.vaults(&none, &none, 6, &doors)[0].state, VaultState::Known, "enough research");
        assert_eq!(t.vaults(&none, &none, 5, &doors)[0].state, VaultState::Locked);
        assert_eq!(t.vaults(&known, &known, 0, &doors)[0].state, VaultState::Opened);
    }
}
