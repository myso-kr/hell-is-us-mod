//! What the overlay's windows show, worked out from the snapshot: the goals with the
//! player's pins added, the compass marks, the tracker's needs line.

use super::route::ROUTE_AHEAD;
use crate::goals::{Gate, Goal, Tier};
use crate::guide::target::flat;
use crate::minimap::MapState;
use crate::pathfind::Path;
use crate::raster::Pin;

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
        goals.push(place(m.id(world), m.title(), tr!("지도 핀"), m.at));
    }
    if let Some((w, id, at, label)) = state.adhoc.clone() {
        if crate::survey::Survey::world_of(world) == w && !goals.iter().any(|g| g.id == id) {
            goals.push(place(id, label, tr!("패널에서 고른 곳"), at));
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
                    let next = crate::pathfind::next_point(&path.points, [p[0], p[1]], ROUTE_AHEAD).unwrap_or([g.at[0], g.at[1]]);
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
                (h, 0) => trf!("필요한 것: 이 지역 {h}곳", h = h),
                (0, a) => trf!("필요한 것: 다른 지역 {a}곳 — 장갑차로 이동", a = a),
                (h, a) => trf!("필요한 것: 이 지역 {h}곳 · 다른 지역 {a}곳", h = h, a = a),
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
