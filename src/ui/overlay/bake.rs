//! The landscape for the map (relief.rs), baked off the overlay thread when the hero
//! has moved far, the map grew, or the scene changed.

use crate::minimap::{MapState, ReliefMode};
use crate::obstacles::Scene;
use crate::relief::Relief;
use std::sync::Arc;

/// Bake again when the hero is this far from the baked square's centre (cm), or the
/// scene changes; the square reaches this far past the widest map's edge.
const RELIEF_MOVED: f32 = 10_000.0;
const RELIEF_SPARE: f32 = 15_000.0;

/// The landscape baked for the map, and the bake under way.
#[derive(Default)]
pub struct Baking {
    done: Option<Arc<Relief>>,
    /// The scene it was baked from.
    from: Option<Arc<Scene>>,
    pending: Option<std::thread::JoinHandle<(Relief, Arc<Scene>)>>,
}

impl Baking {
    /// The relief to draw around the hero at `p`, starting a new bake when the last
    /// one no longer fits; `None` when the map shows no landscape.
    pub fn relief(&mut self, state: &MapState, p: [f32; 3], scene: &Arc<Scene>) -> Option<Arc<Relief>> {
        if self.pending.as_ref().is_some_and(|h| h.is_finished()) {
            if let Ok((rel, from)) = self.pending.take().unwrap().join() {
                self.done = Some(Arc::new(rel));
                self.from = Some(from);
            }
        }
        let half = state.radius_m.max(state.big_radius_m) * 100.0 + RELIEF_SPARE;
        let stale = self.done.as_ref().is_none_or(|r| {
            let (c, h) = r.extent();
            (p[0] - c[0]).hypot(p[1] - c[1]) > RELIEF_MOVED
                || h < half - RELIEF_SPARE / 2.0
                || h > half * 2.0
                || (p[2] - 90.0 - r.feet).abs() > crate::relief::TINT_MOVED
        }) || self.from.as_ref().is_none_or(|s| !Arc::ptr_eq(s, scene));
        if state.relief != ReliefMode::Off && !scene.terrain.is_empty() && stale && self.pending.is_none() {
            let (scene, centre, feet) = (scene.clone(), [p[0], p[1]], p[2] - 90.0);
            self.pending = Some(std::thread::spawn(move || (Relief::bake(&scene, centre, half, feet), scene)));
        }
        self.done.clone().filter(|_| state.relief != ReliefMode::Off)
    }
}
