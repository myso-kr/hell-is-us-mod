//! A take's way from the card's settings: the guide's route, the points walked on the navmesh, a
//! recording as it was walked — or any of them flown above the floor.

use super::flight::FLIGHT_LIFT;
use super::{Setup, Source};

/// A take's route from the hero: the guide's route as drawn (`route`); the setup's points, each
/// leg walked on the navmesh (straight where it has none); or the chosen recording, reached on
/// the navmesh and then followed point by point.
pub fn path(setup: &Setup, hero: [f32; 3], route: &[[f32; 3]], nav: &crate::navmesh::NavMesh) -> Option<Vec<[f32; 3]>> {
    let leg = |from: [f32; 3], to: [f32; 3], out: &mut Vec<[f32; 3]>| match nav.route(from, to) {
        Some((leg, _)) => out.extend(leg.points.iter().skip(1).map(|q| [q[0], q[1], to[2]])),
        None => out.push(to),
    };
    match setup.source {
        Source::Guide => (route.len() >= 2).then(|| route.to_vec()),
        Source::Points => {
            if setup.points.is_empty() {
                return None;
            }
            let mut path = vec![hero];
            let mut from = hero;
            for &to in &setup.points {
                leg(from, to, &mut path);
                from = to;
            }
            Some(path)
        }
        Source::Recording => {
            let rec = setup.recordings.get(setup.recording)?;
            let first = *rec.points.first()?;
            let mut path = vec![hero];
            leg(hero, first, &mut path);
            path.extend(rec.points.iter().skip(1).copied());
            Some(path)
        }
    }
}

/// How far into `path` (cm, flat) the chosen recording begins: where its first point is.
pub fn recording_starts(setup: &Setup, path: &[[f32; 3]]) -> f32 {
    let Some(first) = setup.recordings.get(setup.recording).and_then(|r| r.points.first()) else { return 0.0 };
    let mut along = 0.0;
    for w in path.windows(2) {
        if (w[0][0] - first[0]).hypot(w[0][1] - first[1]) < 1.0 {
            return along;
        }
        along += (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
    }
    along
}

/// A flight's points from the setup: the guide's route, the points, or the recording, lifted above
/// the floor (the camera's own start is put in front when it rolls).
pub fn flight_path(setup: &Setup, route: &[[f32; 3]]) -> Option<Vec<[f32; 3]>> {
    let lift = |p: &[f32; 3]| [p[0], p[1], p[2] + FLIGHT_LIFT];
    match setup.source {
        Source::Guide => (route.len() >= 2).then(|| route.iter().map(lift).collect()),
        Source::Points => (!setup.points.is_empty()).then(|| setup.points.iter().map(lift).collect()),
        Source::Recording => {
            let rec = setup.recordings.get(setup.recording)?;
            // a recording is dense: one point a metre is plenty to fly
            (rec.points.len() >= 2).then(|| rec.points.iter().step_by(2).map(lift).collect())
        }
    }
}
