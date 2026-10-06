//! Filming from the worker's side: a take rolled with what it writes found in the game, stopped,
//! and — while a flight carries the hero behind the camera — the hero guarded: kept alive and
//! ignored by the enemies, the player's own cheats back when the take ends.

use super::take_record::Record;
use super::Engine;
use crate::cheats::Active;
use std::time::Instant;

/// The cheats a flight turns on for the hero it carries: health held at its cap, the enemies'
/// side (they leave it be), and no fall damage (carried in the air, it lands when put back).
const GUARD: [&str; 3] = ["god", "ghost", "nofall"];

impl Engine {
    /// Roll a filming take (film.rs): the hero walked along `plan` by the stick's input, the
    /// camera turned as it says. A take already rolling is stopped first.
    pub fn film(
        &mut self,
        plan: crate::film::Plan,
        state: std::sync::Arc<std::sync::Mutex<crate::film::State>>,
        ended: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
    ) -> Result<(), String> {
        self.take = None;
        let least = match plan.mode {
            crate::film::Mode::Walk => 2,
            crate::film::Mode::Flight => 1,
        };
        if plan.path.len() < least {
            return Err(tr!("FILM_NO_ROUTE").into());
        }
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let (m, n) = (&a.game, &a.anchors.names);
        let chain = a.chain()?;
        let hero = chain.hero(m, &a.anchors)?;
        let pc = chain.controller(m, &a.anchors)?;
        let input =
            n.field(m, hero, "ControlInputVector").ok_or_else(|| trf!("NO_PROPERTY", name = "ControlInputVector"))?;
        // The cameras' own settings (exploration, combat, APC), which the game reads every
        // frame; the camera mode's pivot and the hero's body, for a flight.
        let pcm = n.follow(m, pc, "PlayerCameraManager").ok();
        let mut distance = Vec::new();
        let mut fov = Vec::new();
        for name in ["ExplorationConfig", "CombatConfig", "APCConfig"] {
            let Some(c) = pcm.and_then(|p| n.follow(m, p, name).ok()) else { continue };
            distance.extend(n.field(m, c, "DefaultDistanceFromPlayer").map(|p| c + p.offset as u64));
            fov.extend(n.field(m, c, "FieldOfView").map(|p| c + p.offset as u64));
        }
        distance.dedup();
        fov.dedup();
        let pivot = pcm
            .and_then(|p| n.follow(m, p, "CameraModeInstance").ok())
            .and_then(|mode| n.field(m, mode, "PivotToViewTarget").map(|p| mode + p.offset as u64 + 0x20));
        let body = crate::mem::read_u64(m, hero + chain.root)
            .filter(|&r| crate::mem::plausible(r))
            .and_then(|r| n.field(m, r, "RelativeRotation").map(|p| r + p.offset as u64));
        let blend = pcm.and_then(|p| n.follow(m, p, "CameraModeInstance").ok()).and_then(|mode| {
            let i = n.field(m, mode, "PenetrationBlendInTime")?;
            let o = n.field(m, mode, "PenetrationBlendOutTime")?;
            Some((mode + i.offset as u64, mode + o.offset as u64))
        });
        // a flight carries the hero behind the camera (the game draws the land finely only near
        // it); a walk lifts it on when it is caught on something
        let hero_root = crate::mem::read_u64(m, hero + chain.root).filter(|&r| crate::mem::plausible(r)).map(|root| {
            let mc = n.follow(m, hero, "CharacterMovement").ok();
            let at = |o: u64, name: &str| n.field(m, o, name).map(|v| o + v.offset as u64);
            crate::film::HeroRoot {
                location: root + chain.location,
                world: root + crate::obstacles::COMPONENT_TO_WORLD + 0x20,
                velocity: mc.and_then(|mc| at(mc, "Velocity")),
                mode: mc.and_then(|mc| at(mc, "MovementMode")),
                scale: n.follow(m, hero, "Mesh").ok().and_then(|mesh| at(mesh, "RelativeScale3D")),
            }
        });
        let wiring = crate::film::Wiring {
            input: hero + input.offset as u64,
            rotation: pc + chain.rotation,
            pose: chain.pose_source(m, &a.anchors)?,
            distance,
            fov,
            pivot,
            body,
            blend,
            safety: pcm
                .and_then(|p| n.follow(m, p, "CameraModeInstance").ok())
                .and_then(|mode| n.field(m, mode, "bValidateSafeLoc").map(|f| mode + f.offset as u64)),
            zoom: pcm.and_then(|p| n.follow(m, p, "CameraModeInstance").ok()).and_then(|mode| {
                let interp = n.field(m, mode, "CameraToPivotTranslationInterpolator")?;
                Some(crate::film::ZOOM_AT.map(|o| mode + interp.offset as u64 + o))
            }),
            scene: a.obstacles(),
            hero_root,
        };
        let flight = plan.mode == crate::film::Mode::Flight;
        // what the take changes, kept on disk until it has put it back (take_record.rs); a take
        // started over one not yet ended keeps the first's record: the values read now may be the
        // first take's own
        if Record::load().is_none() {
            if let Some(r) = Record::capture(a, flight) {
                r.save();
            }
        }
        self.take = Some(crate::film::roll(plan, wiring, state, ended));
        if flight {
            self.guard();
        }
        Ok(())
    }

    /// Whether a take is rolling now.
    pub fn rolling(&self) -> bool {
        self.take.is_some()
    }

    /// Stop the take rolling, if one is (the guard lets go once it has put everything back).
    pub fn cut(&mut self) {
        if let Some(t) = self.take.as_ref() {
            t.stop();
        }
    }

    /// The take ended (finished, stopped, interrupted): dropped, and the cheats back as the
    /// player had them — kept to try again if that fails.
    pub(super) fn film_ended(&mut self) {
        if self.take.as_ref().is_some_and(|t| t.ended()) {
            self.take = None;
            // it put everything back itself
            Record::forget();
        }
        if self.take.is_some() {
            return;
        }
        let Some(g) = self.film_guard.take() else { return };
        let before = g.before.clone();
        match self.set_active(before) {
            Ok(()) => crate::logfile::line(&format!("film: cheats back as they were: [{}]", names(&g.before))),
            Err(e) => {
                crate::logfile::line(&format!("film: the cheats as they were, not yet: {e}"));
                self.film_guard = Some(g);
            }
        }
    }

    /// A take's record left by a panel that ended mid-take, put back (and the record gone), once
    /// the hero is in play; what was done, if there was one.
    pub fn put_back_left_take(&mut self) -> Option<String> {
        if self.take.is_some() {
            return None;
        }
        let r = Record::load()?;
        let a = self.attached.as_ref()?;
        a.gate().ok()?;
        let done = r.put_back(a);
        Record::forget();
        crate::logfile::line(&format!("film: a take left by a panel that ended mid-take, put back: {done}"));
        Some(done)
    }

    /// The flight's guard: the hero, carried behind the camera, kept alive and ignored —
    /// health held at its cap (`god`), the enemies' side (`ghost`) — added to what is on, the
    /// player's own list kept as it is.
    fn guard(&mut self) {
        if self.film_guard.is_none() {
            let before = self.active.clone();
            let added = GUARD.into_iter().filter(|id| !before.iter().any(|a| a.cheat == *id)).collect();
            crate::logfile::line(&format!("film: cheats on: [{}]; the take adds {added:?}", names(&before)));
            self.film_guard = Some(Guard { before, added });
        }
        let now = self.active.clone();
        if let Err(e) = self.set_active(now) {
            crate::logfile::line(&format!("film: the hero's guard: {e}"));
        }
    }

    /// `toggles` with what the guard added while it holds; the player's own list (less only
    /// what the guard added) kept for after.
    pub(super) fn guarded(&mut self, toggles: Vec<Active>) -> Vec<Active> {
        let Some(g) = self.film_guard.as_mut() else { return toggles };
        g.before = toggles.iter().filter(|t| !g.added.contains(&t.cheat)).cloned().collect();
        let mut on = g.before.clone();
        for &id in &g.added {
            on.extend(Active::parse(id).ok());
        }
        on
    }
}

/// While a flight rolls: the cheats the player has on, and those the take added on top.
pub(super) struct Guard {
    before: Vec<Active>,
    added: Vec<&'static str>,
}

fn names(a: &[Active]) -> String {
    a.iter().map(|a| a.cheat).collect::<Vec<_>>().join(", ")
}
