//! A take's way from the card's settings: walked on the navmesh, or flown above the floor.

use super::flight::FLIGHT_LIFT;
use super::Setup;

/// A take's route from the hero: the guide's route as drawn (`route`), or through the setup's
/// points, each leg walked on the navmesh (straight where it has none).
pub fn path(setup: &Setup, hero: [f32; 3], route: &[[f32; 3]], nav: &crate::navmesh::NavMesh) -> Option<Vec<[f32; 3]>> {
    if !setup.use_points {
        return (route.len() >= 2).then(|| route.to_vec());
    }
    if setup.points.is_empty() {
        return None;
    }
    let mut path = vec![hero];
    let mut from = hero;
    for &to in &setup.points {
        match nav.route(from, to) {
            Some((leg, _)) => path.extend(leg.points.iter().skip(1).map(|q| [q[0], q[1], to[2]])),
            None => path.push(to),
        }
        from = to;
    }
    Some(path)
}

/// A flight's points from the setup: the 3D map's, or the guide's route, lifted above the floor
/// (the camera's own start is put in front when it rolls).
pub fn flight_path(setup: &Setup, route: &[[f32; 3]]) -> Option<Vec<[f32; 3]>> {
    let lift = |p: &[f32; 3]| [p[0], p[1], p[2] + FLIGHT_LIFT];
    if setup.use_points {
        return (!setup.points.is_empty()).then(|| setup.points.iter().map(lift).collect());
    }
    (route.len() >= 2).then(|| route.iter().map(lift).collect())
}
