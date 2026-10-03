//! Holding toggles — and putting back what they changed.
//!
//! Every tick re-resolves the player, because a level load replaces the pawn and
//! everything hanging off it. The originals of anything a toggle overwrites are taken
//! the first time it is seen and written to disk at once, because a hold does not
//! always get to end cleanly: a terminal can kill it outright, and then no exit
//! handler runs. `hiumod restore` puts back whatever the file holds, and the next hold
//! starts from the file rather than mistaking a held value for an original.

use crate::attr::{Attr, Session};
use crate::cheats::{self, Active};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `originals.txt` in the mod's data folder (paths.rs).
pub fn default_path() -> PathBuf {
    crate::paths::data_dir().join("originals.txt")
}

/// attribute → (BaseValue, CurrentValue).
type Record = BTreeMap<Attr, (f32, f32)>;

pub struct Originals {
    values: Record,
    path: Option<PathBuf>,
}

impl Originals {
    /// In memory only — for tests.
    pub fn transient() -> Originals {
        Originals { values: BTreeMap::new(), path: None }
    }

    /// Pick up where an unfinished hold left off. A missing file is an empty start; a
    /// file that does not parse is an error, not an empty start — it may be the only
    /// record of what the game looked like before.
    pub fn load(path: &Path) -> Result<Originals, String> {
        Originals::load_known(path, &cheats::attributes())
    }

    /// `known`: the attributes a record line may name.
    fn load_known(path: &Path, known: &[Attr]) -> Result<Originals, String> {
        let values = match std::fs::read_to_string(path) {
            Ok(text) => parse(&text, known).map_err(|e| format!("{}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        Ok(Originals { values, path: Some(path.to_path_buf()) })
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, render(&self.values)).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Remember `attrs` the first time they are readable, and get them on disk before
    /// anything overwrites them. Later calls keep the first value.
    pub fn take(&mut self, s: &Session, attrs: &[Attr]) -> Result<(), String> {
        let mut added = false;
        for &a in attrs {
            if let std::collections::btree_map::Entry::Vacant(e) = self.values.entry(a) {
                if let Ok(v) = s.get(a) {
                    e.insert(v);
                    added = true;
                }
            }
        }
        if added {
            self.save()?;
        }
        Ok(())
    }

    /// Write every original back. Once all of them are back, the record is deleted;
    /// anything that failed stays recorded for the next `restore`.
    pub fn restore(&mut self, s: &Session) -> Vec<String> {
        self.restore_where(s, |_| true)
    }

    /// Write back only `attrs` — a toggle being switched off while others stay on.
    pub fn restore_only(&mut self, s: &Session, attrs: &[Attr]) -> Vec<String> {
        self.restore_where(s, |a| attrs.contains(&a))
    }

    fn restore_where(&mut self, s: &Session, pick: impl Fn(Attr) -> bool) -> Vec<String> {
        let mut failed = Vec::new();
        self.values.retain(|&a, &mut (b, c)| {
            if !pick(a) {
                return true;
            }
            match s.put_pair(a, b, c) {
                Ok(()) => false,
                Err(e) => {
                    failed.push(format!("{}.{}: {e}", a.set, a.name));
                    true
                }
            }
        });
        failed.extend(self.persist().err());
        failed
    }

    /// Drop the record without writing anything — the process it described is gone,
    /// and with it every value the record would have put back.
    pub fn forget(&mut self) -> Result<(), String> {
        self.values.clear();
        self.persist()
    }

    /// The file follows the map: deleted when empty, rewritten otherwise.
    fn persist(&self) -> Result<(), String> {
        match &self.path {
            Some(p) if self.values.is_empty() => match std::fs::remove_file(p) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", p.display())),
                _ => Ok(()),
            },
            _ => self.save(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// What is on record: attribute → (BaseValue, CurrentValue) before any cheat.
    pub fn entries(&self) -> Vec<(Attr, (f32, f32))> {
        self.values.iter().map(|(a, v)| (*a, *v)).collect()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }
}

/// One line per attribute: `Set Attribute base current`, the floats as their bit
/// patterns so a value goes back exactly as it was read.
fn render(values: &Record) -> String {
    values
        .iter()
        .map(|(a, &(b, c))| {
            format!(
                "{} {} {:08X} {:08X}
",
                a.set,
                a.name,
                b.to_bits(),
                c.to_bits()
            )
        })
        .collect()
}

/// 0.1 recorded set slot and offset rather than names, and only ever these two —
/// the only attributes its holds overwrote.
fn legacy(slot: &str, data: &str) -> Option<Attr> {
    match (slot, data) {
        ("0", "0x90") => Some(crate::attr::attr("ATR_Resistance", "BaseDamageResistanceMultiplier")),
        ("1", "0xA0") => Some(crate::attr::attr("ATR_Movement", "MovementSpeedMultiplier")),
        _ => None,
    }
}

fn parse(text: &str, known: &[Attr]) -> Result<Record, String> {
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let bad = || format!("line {} is not `Set Attribute BASE CURRENT`", n + 1);
        let f: Vec<&str> = line.split_whitespace().collect();
        let [set, name, b, c] = f[..] else { return Err(bad()) };
        let a = legacy(set, name)
            .or_else(|| known.iter().copied().find(|a| a.set == set && a.name == name))
            .ok_or_else(|| trf!("LINE_IS_NO_ATTRIBUTE_THIS_VERSION", line = n + 1, set = set, name = name))?;
        let bits = |s: &str| u32::from_str_radix(s, 16).map(f32::from_bits).map_err(|_| bad());
        out.insert(a, (bits(b)?, bits(c)?));
    }
    Ok(out)
}

/// One tick: take originals, then apply every toggle. A toggle that fails this tick
/// — a loading screen, a menu without a pawn — is reported, not fatal. Failing to
/// record an original is: the toggle would then overwrite something nobody can put
/// back.
pub fn tick(s: &Session, active: &[Active], originals: &mut Originals) -> Result<Vec<String>, String> {
    let mut errors = Vec::new();
    for t in active {
        originals.take(s, &t.restores())?;
        if let Err(e) = t.apply(s) {
            errors.push(format!("{}: {e}", t.cheat));
        }
    }
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attr::tests::{open, world, A, B, C, D};

    #[test]
    fn originals_are_the_first_values_seen_and_go_back() {
        let m = world();
        let s = open(&m).unwrap();
        let mut o = Originals::transient();
        o.take(&s, &[A]).unwrap();
        s.put(A, 9.0).unwrap();
        o.take(&s, &[A]).unwrap();
        assert!(o.restore(&s).is_empty());
        assert_eq!(s.get(A), Ok((1.0, 2.0)));
        assert!(o.is_empty());
    }

    #[test]
    fn a_killed_hold_is_restored_from_disk() {
        let path = std::env::temp_dir().join(format!("hiumod-test-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let m = world();
        let s = open(&m).unwrap();

        let mut first = Originals::load_known(&path, &[D]).unwrap();
        first.take(&s, &[D]).unwrap();
        s.put(D, 1.5).unwrap();
        drop(first); // killed: no restore ran

        // The next run must not take 1.5 as the original.
        let mut next = Originals::load_known(&path, &[D]).unwrap();
        next.take(&s, &[D]).unwrap();
        assert!(next.restore(&s).is_empty());
        assert_eq!(s.get(D), Ok((1.0, 2.0)));
        assert!(!path.exists(), "a complete restore deletes the record");
    }

    #[test]
    fn switching_one_toggle_off_restores_only_its_attributes() {
        let m = world();
        let s = open(&m).unwrap();
        let mut o = Originals::transient();
        o.take(&s, &[A, C]).unwrap();
        s.put(A, 9.0).unwrap();
        s.put(C, 9.0).unwrap();
        assert!(o.restore_only(&s, &[A]).is_empty());
        assert_eq!((s.get(A), s.get(C)), (Ok((1.0, 2.0)), Ok((9.0, 9.0))));
        assert_eq!(o.len(), 1);
    }

    #[test]
    fn the_record_round_trips_exactly() {
        let mut v = BTreeMap::new();
        v.insert(A, (1.0, 0.1));
        v.insert(B, (-0.0, 1e30));
        assert_eq!(parse(&render(&v), &[A, B]).unwrap(), v);
        assert!(parse(
            "ATR_A Alpha nope 3F800000
",
            &[A]
        )
        .is_err());
        assert!(parse(
            "ATR_A Unknown 3F800000 3F800000
",
            &[A]
        )
        .is_err());
        assert!(parse(
            "
",
            &[]
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn a_0_1_record_still_restores() {
        let v = parse(
            "0 0x90 3F800000 3F800000
1 0xA0 3F800000 3F800000
",
            &[],
        )
        .unwrap();
        let mut names: Vec<_> = v.keys().map(|a| a.name).collect();
        names.sort();
        assert_eq!(names, ["BaseDamageResistanceMultiplier", "MovementSpeedMultiplier"]);
        assert!(
            parse(
                "12 0x90 3F800000 3F800000
",
                &[]
            )
            .is_err(),
            "0.1 never recorded this"
        );
    }
}
