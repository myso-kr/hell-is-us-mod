//! No fall damage: the game weighs a landing against `FallDamageConfig`'s heights — read from its
//! class default object (`Default__FallDamageConfig`, /Script/Charlie; on 24045435 damage from
//! 4.5 m, scaled 9–12 m, death from 15 m: `doctor inspect cdo=FallDamageConfig`). While on, every
//! height is put out of reach and every damage share at nothing; the values as they were are put
//! back when it goes off. The default object lives as long as the game: found once, and its
//! values written only while it still is one.

use crate::cheats::{Active, Effect};
use crate::extras::Reach;

/// The heights put out of reach (cm), and the fields set to it or to nothing. The one height
/// below which no landing is played is left alone.
const OUT_OF_REACH: f32 = 1.0e7;
const HEIGHTS: [&str; 6] = [
    "LightLandingHeight",
    "HeavyLandingHeight",
    "DeathLandingHeight",
    "LightDamageHeight",
    "ScaledDamageStartHeight",
    "KillHeight",
];
const SHARES: [&str; 2] = ["LightDamageRatio", "MaxDamageRatio"];
const MAX_HEIGHT: &str = "ScaledDamageMaxHeight";

pub struct Falls {
    /// The default object, once found.
    cdo: Option<u64>,
    /// Each field and its value as it was, while on.
    was: Vec<(String, f32)>,
}

fn path() -> std::path::PathBuf {
    crate::paths::data_dir().join("falls.txt")
}

fn save(was: &[(String, f32)]) {
    if was.is_empty() {
        let _ = std::fs::remove_file(path());
    } else {
        let text: String = was.iter().map(|(f, v)| format!("{f} {v}\n")).collect();
        let _ = std::fs::write(path(), text);
    }
}

fn parse(text: &str) -> Vec<(String, f32)> {
    text.lines()
        .filter_map(|l| {
            let (f, v) = l.split_once(' ')?;
            let v: f32 = v.trim().parse().ok().filter(|v: &f32| v.is_finite())?;
            (HEIGHTS.contains(&f) || SHARES.contains(&f) || f == MAX_HEIGHT).then(|| (f.to_string(), v))
        })
        .collect()
}

fn wanted(active: &[Active]) -> bool {
    active.iter().any(|a| {
        crate::cheats::find(a.cheat).is_some_and(|c| c.effects().iter().any(|e| matches!(e, Effect::NoFallDamage)))
    })
}

impl Falls {
    /// Start from a record a panel left with the cheat on.
    pub fn load() -> Falls {
        Falls { cdo: None, was: std::fs::read_to_string(path()).map(|t| parse(&t)).unwrap_or_default() }
    }

    fn find(&mut self, a: &dyn Reach) -> Option<u64> {
        let (m, n) = (a.memory(), a.names());
        let ok = |o: u64| n.object(m, o).as_deref() == Some("Default__FallDamageConfig");
        if let Some(o) = self.cdo.filter(|&o| ok(o)) {
            return Some(o);
        }
        let objects = crate::gobjects::discover(m, a.base()).ok()?;
        self.cdo = objects.all(m).into_iter().find(|&o| ok(o));
        self.cdo
    }

    /// Each tick: held while the cheat is on.
    pub fn tick(&mut self, a: &dyn Reach, active: &[Active]) -> Option<String> {
        if !wanted(active) {
            return None;
        }
        let Some(cdo) = self.find(a) else { return Some(tr!("NO_FALL_DAMAGE_NOT_FOUND").into()) };
        let (m, n) = (a.memory(), a.names());
        let at = |name: &str| n.field(m, cdo, name).map(|p| cdo + p.offset as u64);
        let read = |at: u64| {
            let mut b = [0u8; 4];
            m.read(at, &mut b).then(|| f32::from_le_bytes(b)).filter(|v| v.is_finite())
        };
        let mut writes: Vec<(&str, f32)> = Vec::new();
        writes.extend(HEIGHTS.iter().map(|f| (*f, OUT_OF_REACH)));
        writes.push((MAX_HEIGHT, OUT_OF_REACH * 2.0));
        writes.extend(SHARES.iter().map(|f| (*f, 0.0)));
        let before = self.was.len();
        for (f, v) in writes {
            let Some(p) = at(f) else { continue };
            if !self.was.iter().any(|(q, _)| q == f) {
                match read(p) {
                    Some(old) => self.was.push((f.to_string(), old)),
                    None => continue,
                }
            }
            m.write(p, &v.to_le_bytes());
        }
        if self.was.len() != before {
            save(&self.was);
        }
        None
    }

    /// The values as they were, unless the cheat is still on in `keep`.
    pub fn release(&mut self, a: &dyn Reach, keep: &[Active]) {
        if wanted(keep) || self.was.is_empty() {
            return;
        }
        let Some(cdo) = self.find(a) else { return };
        let (m, n) = (a.memory(), a.names());
        for (f, v) in self.was.drain(..) {
            if let Some(p) = n.field(m, cdo, &f) {
                m.write(cdo + p.offset as u64, &v.to_le_bytes());
            }
        }
        save(&self.was);
    }

    /// The game went away: what was written went with it, and the record with it.
    pub fn forget(&mut self) {
        self.cdo = None;
        self.was.clear();
        save(&self.was);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heights_as_they_were_are_read_back_and_nothing_else() {
        let was = parse("KillHeight 1500\nLightDamageRatio 0.1\nSomethingElse 3\nKillHeight nan\n");
        assert_eq!(was, vec![("KillHeight".to_string(), 1500.0), ("LightDamageRatio".to_string(), 0.1)]);
    }
}
