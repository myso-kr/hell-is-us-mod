//! What the overlay's windows show, worked out from the snapshot: the goals with the
//! player's pins added, the compass marks, the tracker's needs line.

use super::route::ROUTE_AHEAD;
use crate::goals::{Gate, Goal, Tier};
use crate::guide::target::flat;
use crate::minimap::MapState;
use crate::pathfind::Path;
use crate::raster::Pin;

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
    };
    for m in state.markers.get(world).into_iter().flatten() {
        goals.push(place(m.id(world), m.title(), tr!("MAP_PIN"), m.at));
    }
    if let Some((w, id, at, label)) = state.adhoc.clone() {
        if crate::survey::Survey::world_of(world) == w && !goals.iter().any(|g| g.id == id) {
            goals.push(place(id, label, tr!("PICKED_IN_THE_PANEL"), at));
        }
    }
    goals
}

/// The compass marks: the goals of the tiers shown (the target along its route) and
/// the pins of this world.
pub fn compass_pins(goals: &[Goal], state: &MapState, world: &str, p: [f32; 3], path: &Path) -> Vec<Pin> {
    let mut pins: Vec<Pin> = goals
        .iter()
        .filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0 || Some(g.id) == state.target)
        .filter(|g| !crate::minimap::is_pin(g.id) || Some(g.id) == state.target)
        .map(|g| {
            let target = Some(g.id) == state.target;
            // The target is pointed at along its route, and its distance is the route's.
            let (aim, distance) = match (target, path.points.len() >= 2) {
                (true, true) => {
                    let next = crate::pathfind::next_point(&path.points, [p[0], p[1]], ROUTE_AHEAD)
                        .unwrap_or([g.at[0], g.at[1]]);
                    ([next[0], next[1], g.at[2]], crate::pathfind::length(&path.points))
                }
                _ => (g.at, flat(p, g.at)),
            };
            Pin {
                bearing: bearing(p, aim) - state.north_yaw,
                rgb: g.tier.rgb(),
                target,
                distance_m: distance / 100.0,
                dz_m: (g.at[2] - p[2]) / 100.0,
            }
        })
        .collect();
    if let Some(markers) = state.markers.get(world) {
        pins.extend(markers.iter().filter(|m| state.target != Some(m.id(world))).map(|m| Pin {
            bearing: bearing(p, m.at) - state.north_yaw,
            rgb: m.kind.rgb(),
            target: false,
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
