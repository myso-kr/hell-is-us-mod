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

#[derive(Default)]
pub struct Falls {
    /// The default object, once found.
    cdo: Option<u64>,
    /// Each field's address and its value as it was, while on.
    was: Vec<(u64, f32)>,
}

fn wanted(active: &[Active]) -> bool {
    active.iter().any(|a| {
        crate::cheats::find(a.cheat).is_some_and(|c| c.effects().iter().any(|e| matches!(e, Effect::NoFallDamage)))
    })
}

impl Falls {
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
        let mut writes: Vec<(u64, f32)> = Vec::new();
        writes.extend(HEIGHTS.iter().filter_map(|f| at(f)).map(|p| (p, OUT_OF_REACH)));
        writes.extend(at(MAX_HEIGHT).map(|p| (p, OUT_OF_REACH * 2.0)));
        writes.extend(SHARES.iter().filter_map(|f| at(f)).map(|p| (p, 0.0)));
        for (p, v) in writes {
            if !self.was.iter().any(|(q, _)| *q == p) {
                match read(p) {
                    Some(old) => self.was.push((p, old)),
                    None => continue,
                }
            }
            m.write(p, &v.to_le_bytes());
        }
        None
    }

    /// The values as they were, unless the cheat is still on in `keep`.
    pub fn release(&mut self, a: &dyn Reach, keep: &[Active]) {
        if wanted(keep) || self.was.is_empty() {
            return;
        }
        let (m, n) = (a.memory(), a.names());
        // only into the same default object
        if self.cdo.is_some_and(|o| n.object(m, o).as_deref() == Some("Default__FallDamageConfig")) {
            for (p, v) in self.was.drain(..) {
                m.write(p, &v.to_le_bytes());
            }
        }
        self.was.clear();
    }

    /// The game went away: what was written went with it.
    pub fn forget(&mut self) {
        self.cdo = None;
        self.was.clear();
    }
}
