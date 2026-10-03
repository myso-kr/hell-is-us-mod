//! The shard budget (.spec/JOURNEY.md §3.6): what the three upgrade achievements still
//! cost in shards, against the shards held — worth knowing before the last timeloops
//! close, when the enemies stop coming back and shards become finite.
//!
//! The recipes are the game's (`recipes.json`, tables.rs): an item of one grade and some
//! shards make the next grade. Shards come in a feeling (`Neutral`, `Rage`…) and a tier
//! (G01–G03); three neutral shards infuse into one of a feeling, within a tier.

use std::collections::{BTreeMap, BinaryHeap, HashMap};

use crate::tables::Recipe;

/// Shards by (feeling, tier): `("Rage", 3)` is `Craft_Shards_Rage_G03_DA`.
pub type Shards = BTreeMap<(String, u8), u32>;

/// A shard item's (feeling, tier).
pub fn shard(item: &str) -> Option<(String, u8)> {
    let rest = item.strip_prefix("Craft_Shards_")?.strip_suffix("_DA")?;
    let (feeling, tier) = rest.rsplit_once("_G")?;
    Some((feeling.to_string(), tier.parse().ok()?))
}

/// An item's grade: `TwinAxes_Rage_Grade_04_DA`, `…_Grade_03_Level10_DA`, `…_Grade03_DA`.
pub fn grade(item: &str) -> Option<u8> {
    let at = item.find("Grade")?;
    item[at + 5..].trim_start_matches('_').get(..2)?.parse().ok()
}

/// A weapon's type (`TwinAxes`), for the four Good Vibrations counts.
pub fn weapon_type(item: &str) -> Option<&str> {
    let kind = item.split('_').next()?;
    WEAPONS.contains(&kind).then_some(kind)
}

pub const WEAPONS: [&str; 4] = ["TwinAxes", "Sword1H", "Sword2H", "Polearm"];

pub fn is_gear(item: &str) -> bool {
    item.starts_with("DefensiveGear_")
}

/// How costly shards are, in neutral shards: a feeling's takes three to infuse.
fn weight(s: &Shards) -> u32 {
    s.iter().map(|((f, _), n)| if f == "Neutral" { *n } else { n * 3 }).sum()
}

fn add(a: &mut Shards, b: &Shards) {
    for (k, n) in b {
        *a.entry(k.clone()).or_default() += n;
    }
}

/// The cheapest way from `from` to an item `done` accepts, by the recipes: its shards,
/// or `None` when there is no way. An item `done` already accepts costs nothing.
pub fn cheapest(recipes: &[Recipe], from: &str, done: impl Fn(&str) -> bool) -> Option<Shards> {
    let mut by_from: HashMap<&str, Vec<&Recipe>> = HashMap::new();
    for r in recipes {
        by_from.entry(r.from.as_str()).or_default().push(r);
    }
    // Dijkstra over items, by weight; ties by name keep it the same each run.
    let mut best: HashMap<String, (u32, Shards)> = HashMap::from([(from.to_string(), (0, Shards::new()))]);
    let mut queue = BinaryHeap::from([std::cmp::Reverse((0u32, from.to_string()))]);
    while let Some(std::cmp::Reverse((w, item))) = queue.pop() {
        if best.get(&item).is_some_and(|(b, _)| *b < w) {
            continue;
        }
        let cost = best[&item].1.clone();
        if done(&item) {
            return Some(cost);
        }
        for r in by_from.get(item.as_str()).into_iter().flatten() {
            let step: Shards = r.needs.iter().filter_map(|(i, n)| shard(i).map(|k| (k, *n))).collect();
            let mut next = cost.clone();
            add(&mut next, &step);
            let nw = weight(&next);
            if best.get(&r.to).is_none_or(|(b, _)| nw < *b) {
                best.insert(r.to.clone(), (nw, next));
                queue.push(std::cmp::Reverse((nw, r.to.clone())));
            }
        }
    }
    None
}

/// One item to raise: the one held that is cheapest to bring to the grade.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub item: String,
    pub grade: u8,
    pub to: u8,
    pub cost: Shards,
}

/// An achievement's plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The achievement's Steam API name: the panel shows its name and whether it is done.
    pub api: &'static str,
    pub steps: Vec<Step>,
    /// What it still needs that no held item can become (a weapon type not found yet).
    pub missing: Vec<String>,
    pub cost: Shards,
    /// Per tier, the neutral shards short after infusing (none when the shards held do).
    pub short: Vec<(u8, u32)>,
}

pub const GOOD_VIBRATIONS: &str = "WeaponMaxUpgrade_AllWeapons_Once";
pub const ACCESSORIZING: &str = "DefensiveGearMaxUpgrade_Any";
pub const TO_THE_TEETH: &str = "LoadoutMaxGrade";

/// The held items' cheapest steps to `to`, cheapest first.
fn steps(recipes: &[Recipe], held: &[&str], to: u8) -> Vec<Step> {
    let mut out: Vec<Step> = held
        .iter()
        .filter_map(|item| {
            let cost = cheapest(recipes, item, |i| grade(i).is_some_and(|g| g >= to))?;
            Some(Step { item: item.to_string(), grade: grade(item)?, to, cost })
        })
        .collect();
    out.sort_by(|a, b| weight(&a.cost).cmp(&weight(&b.cost)).then(b.grade.cmp(&a.grade)).then(a.item.cmp(&b.item)));
    out
}

/// What the shards held leave short, per tier, in neutral shards: a feeling's shards
/// missing are infused from neutral ones, three each.
pub fn short(cost: &Shards, held: &Shards) -> Vec<(u8, u32)> {
    let mut tiers: Vec<u8> = cost.keys().map(|(_, t)| *t).collect();
    tiers.sort_unstable();
    tiers.dedup();
    tiers
        .into_iter()
        .filter_map(|t| {
            let have = |f: &str| held.get(&(f.to_string(), t)).copied().unwrap_or(0);
            let mut neutral = 0;
            for ((f, _), n) in cost.iter().filter(|((_, tier), _)| *tier == t) {
                if f == "Neutral" {
                    neutral += n;
                } else {
                    neutral += n.saturating_sub(have(f)) * 3;
                }
            }
            let gap = neutral.saturating_sub(have("Neutral"));
            (gap > 0).then_some((t, gap))
        })
        .collect()
}

fn plan(api: &'static str, steps: Vec<Step>, missing: Vec<String>, held: &Shards) -> Plan {
    let mut cost = Shards::new();
    for s in &steps {
        add(&mut cost, &s.cost);
    }
    let short = short(&cost, held);
    Plan { api, steps, missing, cost, short }
}

/// The shards held and the three plans.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Budget {
    pub held: Shards,
    pub plans: Vec<Plan>,
}

/// The three plans, from the items held (`counts`, by asset name) and the recipes.
pub fn budget(recipes: &[Recipe], counts: &HashMap<String, u32>) -> Budget {
    let held: Shards = counts.iter().filter_map(|(i, n)| shard(i).map(|k| (k, *n))).collect();
    let items: Vec<&str> = counts.keys().map(String::as_str).filter(|i| grade(i).is_some()).collect();
    let weapons: Vec<&str> = items.iter().copied().filter(|i| weapon_type(i).is_some()).collect();
    let gears: Vec<&str> = items.iter().copied().filter(|i| is_gear(i)).collect();

    // Good Vibrations: one of each weapon type at grade 5.
    let (mut vibes, mut missing) = (Vec::new(), Vec::new());
    for kind in WEAPONS {
        let mine: Vec<&str> = weapons.iter().copied().filter(|i| weapon_type(i) == Some(kind)).collect();
        match steps(recipes, &mine, 5).into_iter().next() {
            Some(s) => vibes.push(s),
            None => missing.push(kind.to_string()),
        }
    }
    // Accessorizing: one defensive gear at grade 4.
    let gear = steps(recipes, &gears, 4);
    let accessorizing = gear.iter().take(1).cloned().collect::<Vec<_>>();
    let gear_missing = if gear.is_empty() { vec!["DefensiveGear".to_string()] } else { Vec::new() };
    // To the Teeth: two weapons at grade 5 and two gears at grade 4, worn at once.
    let mut teeth: Vec<Step> = steps(recipes, &weapons, 5).into_iter().take(2).collect();
    teeth.extend(gear.iter().take(2).cloned());
    let mut teeth_missing = Vec::new();
    if teeth.iter().filter(|s| s.to == 5).count() < 2 {
        teeth_missing.push("Weapon".to_string());
    }
    if gear.len() < 2 {
        teeth_missing.push("DefensiveGear".to_string());
    }
    let plans = vec![
        plan(GOOD_VIBRATIONS, vibes, missing, &held),
        plan(ACCESSORIZING, accessorizing, gear_missing, &held),
        plan(TO_THE_TEETH, teeth, teeth_missing, &held),
    ];
    Budget { held, plans }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(from: &str, needs: &[(&str, u32)], to: &str) -> Recipe {
        Recipe {
            kind: String::new(),
            from: from.into(),
            needs: needs.iter().map(|(i, n)| (i.to_string(), *n)).collect(),
            to: to.into(),
        }
    }

    #[test]
    fn names_tell_shards_and_grades() {
        assert_eq!(shard("Craft_Shards_Rage_G03_DA"), Some(("Rage".into(), 3)));
        assert_eq!(shard("TwinAxes_Rage_Grade_04_DA"), None);
        assert_eq!(grade("TwinAxes_Rage_Grade_04_DA"), Some(4));
        assert_eq!(grade("TwinAxes_Rage_Grade_03_Level10_DA"), Some(3));
        assert_eq!(grade("DefensiveGear_RageV01_ChainedHits_Grade03_DA"), Some(3));
        assert_eq!(weapon_type("Sword1H_Neutral_Grade_01_DA"), Some("Sword1H"));
        assert_eq!(weapon_type("DefensiveGear_RageV01_ChainedHits_Grade03_DA"), None);
    }

    #[test]
    fn the_cheapest_way_counts_a_feelings_shards_three_times() {
        let recipes = [
            r("Axe_Neutral_Grade_01_DA", &[("Craft_Shards_Neutral_G01_DA", 75)], "Axe_Rage_Grade_02_DA"),
            r("Axe_Rage_Grade_02_DA", &[("Craft_Shards_Rage_G02_DA", 40)], "Axe_Rage_Grade_03_DA"),
            // 100 neutral is cheaper than 40 of a feeling (120).
            r("Axe_Rage_Grade_02_DA", &[("Craft_Shards_Neutral_G02_DA", 100)], "Axe_Rage_Grade_03_Level10_DA"),
        ];
        let at3 = |i: &str| grade(i).is_some_and(|g| g >= 3);
        let cost = cheapest(&recipes, "Axe_Neutral_Grade_01_DA", at3).unwrap();
        assert_eq!(cost, Shards::from([(("Neutral".into(), 1), 75), (("Neutral".into(), 2), 100)]));
        assert_eq!(cheapest(&recipes, "Axe_Rage_Grade_03_DA", at3), Some(Shards::new()), "already there");
        assert_eq!(cheapest(&recipes, "Bow_Neutral_Grade_01_DA", at3), None, "no recipe");
    }

    #[test]
    fn what_is_short_infuses_a_feeling_from_neutral_shards() {
        // Keyed by feeling first: tiers come out of order and must still count once each.
        let two = Shards::from([(("Ecstasy".into(), 2), 1), (("Ecstasy".into(), 3), 1), (("Neutral".into(), 2), 1)]);
        assert_eq!(short(&two, &Shards::new()), [(2, 4), (3, 3)]);
        let cost = Shards::from([(("Neutral".into(), 3), 25), (("Rage".into(), 3), 50)]);
        let held = Shards::from([(("Neutral".into(), 3), 100), (("Rage".into(), 3), 30)]);
        // 25 neutral + (50 - 30) × 3 = 85 ≤ 100.
        assert_eq!(short(&cost, &held), []);
        let held = Shards::from([(("Neutral".into(), 3), 40), (("Rage".into(), 3), 30)]);
        assert_eq!(short(&cost, &held), [(3, 45)]);
    }

    #[test]
    fn plans_take_the_cheapest_held_items() {
        let recipes = [
            r("Polearm_Rage_Grade_04_DA", &[("Craft_Shards_Neutral_G03_DA", 25)], "Polearm_Rage_Grade_05_DA"),
            r(
                "DefensiveGear_RageV01_X_Grade03_DA",
                &[("Craft_Shards_Neutral_G03_DA", 15)],
                "DefensiveGear_RageV01_X_Grade04_DA",
            ),
        ];
        let counts: HashMap<String, u32> = [
            ("Polearm_Rage_Grade_04_DA", 1),
            ("TwinAxes_Rage_Grade_05_DA", 1),
            ("DefensiveGear_RageV01_X_Grade03_DA", 1),
            ("Craft_Shards_Neutral_G03_DA", 30),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
        let b = budget(&recipes, &counts);
        assert_eq!(b.held, Shards::from([(("Neutral".into(), 3), 30)]));
        let plans = b.plans;
        let vibes = &plans[0];
        assert_eq!(vibes.missing, ["Sword1H", "Sword2H"], "no sword held");
        assert_eq!(vibes.cost, Shards::from([(("Neutral".into(), 3), 25)]), "the axes are at 5 already");
        assert_eq!(vibes.short, []);
        let teeth = &plans[2];
        assert_eq!(teeth.cost, Shards::from([(("Neutral".into(), 3), 40)]));
        assert_eq!(teeth.short, [(3, 10)]);
        assert_eq!(teeth.missing, ["DefensiveGear"], "one gear held, two worn");
    }
}
