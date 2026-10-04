//! Which goal the guide points at: auto guiding follows the quest — the nearest place
//! that moves it along, past blocked ones to what may open them — and the cycle key
//! walks the shown goals by distance. The overlay settles it each frame (ui/minimap.rs).

use crate::goals::{Gate, Goal, Tier};
use crate::minimap::MapState;
use crate::quests::Quest;

/// Near a blocked goal, a goal this close (cm) may be what opens the way: a clue, a key,
/// a lever, a puzzle.
const HELPER: f32 = 4000.0;

/// Distance on the ground (cm), height left out.
pub fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Whether auto guiding can send the hero to `g` at all: not a place that pays out
/// only when something else happens there (a locked door), and not an item of a main
/// quest not begun.
fn reachable(g: &Goal, journal: &[Quest]) -> bool {
    g.gate != Gate::Conditional
        && !g.keys.iter().any(|k| journal.iter().any(|q| &q.key == k && q.status == crate::quests::Status::NotStarted))
}

/// Whether `g` is what auto guiding wants: a reachable place that moves the followed
/// quest along — or, following the main story with none of its places loaded, any
/// reachable quest goal.
fn wanted(g: &Goal, goals: &[Goal], followed: Option<&Quest>, journal: &[Quest]) -> bool {
    if !reachable(g, journal) {
        return false;
    }
    match followed {
        Some(q) if goals.iter().any(|x| x.serves(q) && reachable(x, journal)) => g.serves(q),
        Some(q) if !matches!(q.kind, crate::quests::Kind::Main(_)) => false,
        _ => g.tier == Tier::Quest,
    }
}

/// Auto guiding follows the quest: the nearest place that moves it along, kept until
/// it is used up, then the next — and a target it no longer wants (the quest was
/// changed, or one of its places came into range) gives way. A target picked by hand
/// is left alone until it is used up.
///
/// A goal whose route had to go through something (a locked door, a puzzle, another
/// way into a cellar) is `blocked`: auto guiding prefers one that can be walked to; when
/// every wanted goal is blocked, it goes to something that can be reached near the
/// nearest of them — a note, a key, a lever: what opens the way, often — and else to
/// the blocked goal itself, as near as the route gets.
pub fn settle_target(
    state: &mut MapState,
    goals: &[Goal],
    here: [f32; 3],
    followed: Option<&Quest>,
    journal: &[Quest],
    blocked: &std::collections::HashSet<u64>,
) {
    if state.auto.is_some_and(|t| !goals.iter().any(|g| g.id == t)) {
        state.auto = None;
        state.held = false;
    }
    if !state.guide_auto || state.held {
        return;
    }
    state.auto = next_goal(goals, here, followed, journal, blocked, &state.skipped, state.auto);
}

/// The goal to guide to for `followed` (`None`, the main story), from the hero at `here`,
/// keeping `now` while it is still wanted: settle_target's choice, and each quest's that
/// is followed besides (track.rs).
pub fn next_goal(
    goals: &[Goal],
    here: [f32; 3],
    followed: Option<&Quest>,
    journal: &[Quest],
    blocked: &std::collections::HashSet<u64>,
    skipped: &std::collections::HashSet<u64>,
    now: Option<u64>,
) -> Option<u64> {
    let near = |a: &&Goal, b: &&Goal| flat(a.at, here).total_cmp(&flat(b.at, here));
    let wanted_all: Vec<&Goal> =
        goals.iter().filter(|g| !skipped.contains(&g.id) && wanted(g, goals, followed, journal)).collect();
    let open = wanted_all.iter().copied().filter(|g| !blocked.contains(&g.id)).min_by(near);
    let pick = open.or_else(|| {
        let stuck = wanted_all.iter().copied().min_by(near)?;
        let helper = goals
            .iter()
            .filter(|g| {
                g.id != stuck.id && !blocked.contains(&g.id) && g.gate == Gate::Open && !skipped.contains(&g.id)
            })
            .filter(|g| flat(g.at, stuck.at) <= HELPER)
            .filter(|g| reachable(g, journal))
            .min_by(|a, b| flat(a.at, stuck.at).total_cmp(&flat(b.at, stuck.at)));
        Some(helper.unwrap_or(stuck))
    });
    // Keep the target while it is still what would be picked, or still wanted and not
    // blocked (nearness alone does not make the guide hop between goals).
    let keep = now.filter(|t| {
        Some(*t) == pick.map(|g| g.id)
            || wanted_all.iter().any(|g| g.id == *t) && !blocked.contains(t) && open.is_some()
    });
    keep.or(pick.map(|g| g.id))
}

/// The next goal, by distance, after the current target — among the tiers shown.
pub fn cycle(state: &mut MapState, goals: &[Goal], here: [f32; 3]) {
    let mut shown: Vec<&Goal> = goals.iter().filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0).collect();
    shown.sort_by(|a, b| flat(a.at, here).total_cmp(&flat(b.at, here)));
    let next = match state.auto.and_then(|t| shown.iter().position(|g| g.id == t)) {
        Some(i) => shown.get(i + 1).or(shown.first()),
        None => shown.first(),
    };
    state.auto = next.map(|g| g.id);
    state.held = state.auto.is_some();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(id: u64, tier: Tier, x: f32) -> Goal {
        Goal {
            tier,
            id,
            label: String::new(),
            detail: String::new(),
            at: [x, 0.0, 0.0],
            quests: vec![],
            tags: vec![],
            keys: vec![],
            gate: Gate::Open,
            named: true,
        }
    }

    #[test]
    fn auto_guides_to_the_nearest_quest_goal_and_lets_go_of_a_vanished_one() {
        let mut s = MapState::default();
        let goals = [goal(1, Tier::Clue, 100.0), goal(2, Tier::Quest, 5000.0), goal(3, Tier::Quest, 900.0)];
        settle_target(&mut s, &goals, [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.auto, Some(3));
        settle_target(&mut s, &goals[..2], [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.auto, Some(2), "3 used up: the next quest goal");
        s.guide_auto = false;
        settle_target(&mut s, &goals[..1], [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.auto, None, "auto off: nothing chosen for the player");
    }

    #[test]
    fn auto_follows_the_followed_quest_and_leaves_a_hand_picked_target() {
        let deed = Quest {
            key: "d".into(),
            kind: crate::quests::Kind::GoodDeed,
            name: String::new(),
            detail: String::new(),
            status: crate::quests::Status::Started,
            progress: None,
            leads: vec![],
            quest: None,
            tags: Some("Secrets.Facts.GoldenWatch".into()),
        };
        let mut hand_over = goal(4, Tier::Secret, 3000.0);
        hand_over.tags = vec!["Secrets.Facts.GoldenWatchCompleted".into()];
        let goals = [goal(3, Tier::Quest, 900.0), hand_over];
        let mut s = MapState::default();
        settle_target(&mut s, &goals, [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.auto, Some(3), "the main story: the nearest quest goal");
        settle_target(&mut s, &goals, [0.0; 3], Some(&deed), &[], &Default::default());
        assert_eq!(s.auto, Some(4), "following the deed: its hand-over, though farther");
        settle_target(&mut s, &goals[..1], [0.0; 3], Some(&deed), &[], &Default::default());
        assert_eq!(s.auto, None, "nothing of the deed loaded: no stand-in");
        let mut door = goal(5, Tier::Quest, 10.0);
        door.gate = Gate::Conditional;
        let mut s2 = MapState::default();
        settle_target(&mut s2, &[door, goal(6, Tier::Quest, 500.0)], [0.0; 3], None, &[], &Default::default());
        assert_eq!(s2.auto, Some(6), "a locked door is not where auto guiding sends the hero");
        // Blocked: the wanted goal is behind a door; a note near it is reachable.
        let mut cellar = goal(7, Tier::Quest, 1000.0);
        cellar.at[2] = -1200.0;
        let note = goal(8, Tier::Clue, 1300.0);
        let blocked: std::collections::HashSet<u64> = [7].into_iter().collect();
        let mut s3 = MapState::default();
        settle_target(&mut s3, &[cellar, note], [0.0; 3], None, &[], &blocked);
        assert_eq!(s3.auto, Some(8), "behind a door: first what is near it and reachable");
        s.auto = Some(3);
        s.held = true;
        settle_target(&mut s, &goals, [0.0; 3], Some(&deed), &[], &Default::default());
        assert_eq!(s.auto, Some(3), "picked by hand: kept");
    }

    #[test]
    fn the_cycle_key_walks_the_shown_goals_by_distance() {
        let mut s = MapState { guide_auto: false, ..MapState::default() };
        let goals = [goal(1, Tier::Clue, 300.0), goal(2, Tier::Quest, 100.0), goal(3, Tier::Secret, 200.0)];
        cycle(&mut s, &goals, [0.0; 3]);
        assert_eq!(s.auto, Some(2));
        cycle(&mut s, &goals, [0.0; 3]);
        assert_eq!(s.auto, Some(3));
        s.goal_tiers = 0b011; // no clues
        cycle(&mut s, &goals, [0.0; 3]);
        assert_eq!(s.auto, Some(2), "wraps, skipping the hidden tier");
    }
}
