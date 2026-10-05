//! What the overlay's windows show, worked out from the snapshot: the goals with the
//! player's pins added, the compass marks, the tracker's needs line.

use super::route::ROUTE_AHEAD;
use crate::goals::{Gate, Goal, Tier};
use crate::guide::target::flat;
use crate::minimap::MapState;
use crate::raster::{Drawn, Pin};
use std::sync::Arc;

/// The things the maps show, worked out again only for a new snapshot's things (once a
/// worker step) or a changed consent, not every frame: `with_survey`, and without
/// "where hidden things are" the enemies alone.
#[derive(Default)]
pub struct Shown {
    from: Option<(Arc<Vec<crate::actors::Thing>>, bool)>,
    things: Arc<Vec<crate::actors::Thing>>,
}

impl Shown {
    pub fn things(&mut self, s: &crate::engine::Snapshot, places: bool) -> Arc<Vec<crate::actors::Thing>> {
        let fresh = self.from.as_ref().is_some_and(|(t, p)| Arc::ptr_eq(t, &s.things) && *p == places);
        if !fresh {
            let all = with_survey(&s.things, s);
            let kept =
                if places { all } else { all.into_iter().filter(|t| t.kind() == crate::actors::Kind::Enemy).collect() };
            self.things = Arc::new(kept);
            self.from = Some((s.things.clone(), places));
        }
        self.things.clone()
    }
}

/// `with_pins`, worked out again only when the goals, the world or the pins change:
/// it copied every goal, several times a frame.
#[derive(Default)]
pub struct Pinned {
    from: Option<(Arc<Vec<Goal>>, String, u64)>,
    goals: Arc<Vec<Goal>>,
}

impl Pinned {
    pub fn get(&mut self, goals: &Arc<Vec<Goal>>, state: &MapState, world: &str) -> Arc<Vec<Goal>> {
        let key = pins_key(state, world);
        let fresh = self.from.as_ref().is_some_and(|(g, w, k)| Arc::ptr_eq(g, goals) && w == world && *k == key);
        if !fresh {
            self.goals = Arc::new(with_pins(goals, state, world));
            self.from = Some((goals.clone(), world.to_string(), key));
        }
        self.goals.clone()
    }
}

/// What `with_pins` adds, as a hash: the world's pins and the places followed.
fn pins_key(state: &MapState, world: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for m in state.markers.get(world).into_iter().flatten() {
        (m.id(world), m.title(), m.at.map(f32::to_bits)).hash(&mut h);
    }
    for t in &state.tracks {
        (&t.world, t.id, t.at.map(f32::to_bits), &t.label, t.place).hash(&mut h);
    }
    h.finish()
}

/// The things to draw: the scanned ones, and for the hero's world what the survey
/// knows is left — enemy groups not beaten, puzzles not solved (dials and codes),
/// vault doors not opened.
pub fn with_survey(things: &[crate::actors::Thing], s: &crate::engine::Snapshot) -> Vec<crate::actors::Thing> {
    use crate::actors::{Sub, Thing};
    let mut out = things.to_vec();
    let Some(world) = s.world.as_deref().map(crate::survey::Survey::world_of) else { return out };
    for h in s.hollows.iter().filter(|h| h.world == world) {
        out.extend(h.places.iter().map(|&at| Thing { sub: Sub::EnemyGroup, at }));
    }
    out.extend(
        s.catalogue
            .iter()
            .filter(|(p, solved)| !solved && p.world == world && p.kind != crate::puzzles::Kind::Placement)
            .map(|(p, _)| Thing { sub: Sub::Puzzle, at: p.at }),
    );
    // The ways out the survey knows, where none of the scan's stands (the APC's door far
    // off, a save point not loaded).
    let scanned: Vec<crate::actors::Thing> =
        things.iter().copied().filter(|t| t.kind() == crate::actors::Kind::Save).collect();
    out.extend(
        s.exits
            .iter()
            .filter(|(sub, at)| {
                !scanned.iter().any(|t| t.sub == *sub && (t.at[0] - at[0]).hypot(t.at[1] - at[1]) < 500.0)
            })
            .map(|&(sub, at)| Thing { sub, at }),
    );
    out.extend(
        s.vaults
            .iter()
            .filter(|v| v.state != crate::tables::VaultState::Opened)
            .filter_map(|v| v.door.as_ref().filter(|(w, _)| w == world))
            .map(|(_, at)| Thing { sub: Sub::Vault, at: *at }),
    );
    out
}

/// Whether an unsolved puzzle is within 15 m of the hero.
pub fn puzzle_near(s: &crate::engine::Snapshot) -> bool {
    let Some((p, _)) = s.pose else { return false };
    s.puzzles
        .iter()
        .any(|q| !q.solved && ((q.at[0] as f64 - p[0]).powi(2) + (q.at[1] as f64 - p[1]).powi(2)).sqrt() < 1500.0)
}

/// Which way `to` lies from `from`, as a UE yaw in degrees.
pub fn bearing(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

/// The goals, and the map pins of this world and the place picked in the panel as
/// goals the guide can be sent to.
pub fn with_pins(goals: &[Goal], state: &MapState, world: &str) -> Vec<Goal> {
    let mut goals = goals.to_vec();
    let place = |id, label, detail: &str, at| Goal {
        tier: Tier::Clue,
        id,
        label,
        detail: detail.into(),
        at,
        quests: vec![],
        tags: vec![],
        keys: vec![],
        gate: Gate::Open,
        named: true,
        reveals: Default::default(),
        first: None,
    };
    for m in state.markers.get(world).into_iter().flatten() {
        goals.push(place(m.id(world), m.title(), tr!("MAP_PIN"), m.at));
    }
    // The places followed that are no goal (a groove, a lock), as goals.
    let places = state.place_goals(&goals, world);
    goals.extend(places.into_iter().map(|g| Goal { detail: tr!("PICKED_IN_THE_PANEL").into(), ..g }));
    goals
}

/// The compass marks: the goals of the tiers shown, everything followed along its route
/// in its colour (the one in focus with its distance), and the pins of this world.
pub fn compass_pins(goals: &[Goal], state: &MapState, world: &str, p: [f32; 3], routes: &[Drawn]) -> Vec<Pin> {
    let followed = |id: u64| routes.iter().find(|d| d.id == id);
    let mut pins: Vec<Pin> = goals
        .iter()
        .filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0 || followed(g.id).is_some())
        .filter(|g| !crate::minimap::is_pin(g.id) || followed(g.id).is_some())
        .map(|g| {
            let drawn = followed(g.id);
            let path = drawn.map(|d| &d.path).filter(|p| p.points.len() >= 2);
            // Followed: pointed at along its route, its distance the route's.
            let (aim, distance) = match path {
                Some(path) => {
                    let next = crate::pathfind::next_point(&path.points, [p[0], p[1]], ROUTE_AHEAD)
                        .unwrap_or([g.at[0], g.at[1]]);
                    ([next[0], next[1], g.at[2]], crate::pathfind::length(&path.points))
                }
                None => (g.at, flat(p, g.at)),
            };
            Pin {
                bearing: bearing(p, aim) - state.north_yaw,
                rgb: drawn.and_then(|d| d.colour).unwrap_or(g.tier.rgb()),
                target: drawn.is_some_and(|d| d.focus),
                followed: drawn.is_some(),
                distance_m: distance / 100.0,
                dz_m: (g.at[2] - p[2]) / 100.0,
            }
        })
        .collect();
    if let Some(markers) = state.markers.get(world) {
        pins.extend(markers.iter().filter(|m| followed(m.id(world)).is_none()).map(|m| Pin {
            bearing: bearing(p, m.at) - state.north_yaw,
            rgb: m.kind.rgb(),
            target: false,
            followed: false,
            distance_m: flat(p, m.at) / 100.0,
            dz_m: (m.at[2] - p[2]) / 100.0,
        }));
    }
    pins
}

/// The tracker's line under the quest: a deadline due now first, then how many
/// places the followed quest still needs, here and elsewhere.
pub fn needs_line(
    followed: Option<&crate::quests::Quest>,
    needs: &[(String, Vec<crate::survey::Need>)],
    deadlines: &[crate::missables::Deadline],
    world: &str,
) -> String {
    let line = followed
        .and_then(|q| needs.iter().find(|(k, _)| *k == q.key))
        .map(|(_, list)| {
            let w = crate::survey::Survey::world_of(world);
            let here = list.iter().filter(|x| !x.done && x.world == w).count();
            let away = list.iter().filter(|x| !x.done && x.world != w).count();
            match (here, away) {
                (0, 0) => String::new(),
                (h, 0) => trf!("NEEDED_HERE", h = h),
                (0, a) => trf!("NEEDED_IN_OTHER_REGIONS_TAKE_THE", a = a),
                (h, a) => trf!("NEEDED_HERE_IN_OTHER_REGIONS", h = h, a = a),
            }
        })
        .unwrap_or_default();
    match crate::missables::alert(deadlines) {
        Some(a) if line.is_empty() => a,
        Some(a) => format!("{a}\n{line}"),
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_follow_unreal_yaw() {
        assert_eq!(bearing([0.0; 3], [100.0, 0.0, 0.0]), 0.0);
        assert_eq!(bearing([0.0; 3], [0.0, 100.0, 0.0]), 90.0);
    }
}
