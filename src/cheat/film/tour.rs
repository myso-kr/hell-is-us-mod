//! A tour of the world: no guide's route, no points, no recording — the way worked out from the
//! navmesh alone. Places spread over all the hero can walk to (one every `CELL`), the sights among
//! them (save points, people, things to work) drawing it more; from the hero, each next the nearest
//! not yet seen — a sight counting as nearer — walked to on the navmesh, every place passed near on
//! the way counted as seen, until the tour is as long as asked.

/// How far apart the places are taken (cm), how near a way must pass a place for it to be seen
/// (cm), and how much nearer a sight counts (a share of its distance).
pub const CELL: f32 = 4000.0;
const SEEN: f32 = 2000.0;
const SIGHT: f32 = 0.5;

/// The tour from `hero` among `places` and `sights`, at most `budget` cm long, each leg walked by
/// `route` (its points, from the leg's start to its end; `None` when it cannot be walked).
pub fn tour(
    hero: [f32; 3],
    places: &[[f32; 3]],
    sights: &[[f32; 3]],
    budget: f32,
    route: impl Fn([f32; 3], [f32; 3]) -> Option<Vec<[f32; 3]>>,
) -> Vec<[f32; 3]> {
    let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let mut left: Vec<([f32; 3], bool)> =
        places.iter().map(|&p| (p, false)).chain(sights.iter().map(|&p| (p, true))).collect();
    let mut way = vec![hero];
    let mut length = 0.0;
    let mut at = hero;
    let see = |left: &mut Vec<([f32; 3], bool)>, p: [f32; 3]| left.retain(|(q, _)| flat(*q, p) > SEEN);
    see(&mut left, hero);
    while length < budget && !left.is_empty() {
        let cost = |(p, sight): &([f32; 3], bool)| flat(at, *p) * if *sight { SIGHT } else { 1.0 };
        let Some(i) = (0..left.len()).min_by(|&a, &b| cost(&left[a]).total_cmp(&cost(&left[b]))) else { break };
        let (to, _) = left.swap_remove(i);
        let Some(leg) = route(at, to) else { continue };
        for w in leg.windows(2) {
            length += flat(w[0], w[1]);
        }
        for &p in leg.iter().skip(1) {
            see(&mut left, p);
            way.push(p);
        }
        at = to;
    }
    way
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight(a: [f32; 3], b: [f32; 3]) -> Option<Vec<[f32; 3]>> {
        Some(vec![a, b])
    }

    #[test]
    fn a_tour_covers_the_places_nearest_first_and_stops_at_its_length() {
        // a 10 × 10 grid of places 40 m apart
        let places: Vec<[f32; 3]> = (0..100).map(|i| [(i % 10) as f32 * CELL, (i / 10) as f32 * CELL, 0.0]).collect();
        let way = tour([0.0; 3], &places, &[], f32::MAX, straight);
        let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[1] - b[1]);
        // every place seen
        for p in &places {
            assert!(way.iter().any(|q| flat(*p, *q) <= SEEN), "{p:?} not seen");
        }
        // each step to a neighbour, mostly: no criss-crossing the map
        let long = way.windows(2).filter(|w| flat(w[0], w[1]) > CELL * 1.5).count();
        assert!(long <= 10, "{long} long jumps");
        // a budget stops it
        let short = tour([0.0; 3], &places, &[], 10.0 * CELL, straight);
        let len: f32 = short.windows(2).map(|w| flat(w[0], w[1])).sum();
        assert!((10.0 * CELL..=11.0 * CELL).contains(&len), "{len}");
    }

    #[test]
    fn a_sight_draws_the_tour_and_an_unwalkable_place_is_passed_over() {
        let places = [[4000.0, 0.0, 0.0], [-4500.0, 0.0, 0.0]];
        let sight = [[-6000.0, 0.0, 0.0]];
        let way = tour([0.0; 3], &places, &sight, f32::MAX, straight);
        assert_eq!(way[1], [-6000.0, 0.0, 0.0], "the sight first: {way:?}");
        let walled = |a: [f32; 3], b: [f32; 3]| (b[0] >= 0.0).then(|| vec![a, b]);
        let way = tour([0.0; 3], &places, &[], f32::MAX, walled);
        assert_eq!(way, vec![[0.0; 3], [4000.0, 0.0, 0.0]]);
    }
}
