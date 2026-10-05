//! The requirement graph of the whole game (.spec/GRAPH.md): what each place needs before
//! it can be used, from the survey of every world.
//!
//! A node is an interactable or NPC the survey found. What it needs:
//!
//! - its activators used (`activators`: a receiver — a drain, a door, a payload — is set
//!   off by any of the levers, slots or triggers it names);
//! - its conditions met (`conditions`: another interactable used or in a state, a fact or
//!   tag known or not, all of several);
//! - the items it takes (a slot's or a key door's `puzzle.items`, a Lymbic lock's rods).
//!
//! And what it gives: items, facts and tags (`payload`). From these, `chain` walks back
//! from a place to the first thing that can be done now, and `reach` tells, from nothing
//! known, which places the game can be played to and which it cannot (what is given by the
//! story's scripts, not by a place, shows up there as having no giver).

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// What a node needs.
#[derive(Clone, Debug, PartialEq)]
pub enum Need {
    /// Another node used (an activator pulled, a single-use interactable done).
    Used(String),
    /// A fact or tag known (`true`) or not yet (`false`).
    Fact(String, bool),
    /// An item held.
    Item(String),
    /// Every one of these.
    All(Vec<Need>),
    /// Any one of these.
    Any(Vec<Need>),
}

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub world: String,
    /// The survey's name (`…_UAID_…` with its number dropped) and the full one.
    pub name: String,
    pub class: String,
    pub at: [f32; 3],
    pub guid: Option<String>,
    pub gives_items: Vec<String>,
    pub gives_tags: Vec<String>,
    pub needs: Vec<Need>,
}

/// What the hero has done and holds: the save's state.
pub struct State<'a> {
    /// The save GUIDs with a state: used.
    pub used: &'a HashSet<String>,
    /// Facts and tags known (by name).
    pub known: &'a HashSet<String>,
    pub held: &'a HashSet<String>,
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    /// By full name (what activators and conditions name), per world.
    by_name: HashMap<(String, String), usize>,
    /// Who gives each item, each fact or tag.
    item_givers: HashMap<String, Vec<usize>>,
    tag_givers: HashMap<String, Vec<usize>>,
}

/// A condition from the survey as a need; `None` for what is about where the hero stands
/// (in a trigger, not in combat, a thing at a place): met by going there, not a step.
fn need_of(c: &Value) -> Option<Need> {
    let ty = c["type"].as_str().unwrap_or("");
    let actor = || c["actor"].as_str().map(str::to_string);
    let tags = || -> Vec<String> {
        c["tags"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).map(str::to_string).collect()
    };
    match ty {
        "AndCondition" => {
            let all: Vec<Need> = c["all"].as_array().into_iter().flatten().filter_map(need_of).collect();
            (!all.is_empty()).then_some(Need::All(all))
        }
        t if t.starts_with("DoesHeroHasNotFact") => {
            let t = tags();
            (!t.is_empty()).then(|| Need::All(t.into_iter().map(|x| Need::Fact(x, false)).collect()))
        }
        t if t.starts_with("DoesHeroHasFact") => {
            let t = tags();
            (!t.is_empty()).then(|| Need::All(t.into_iter().map(|x| Need::Fact(x, true)).collect()))
        }
        t if t.starts_with("IsSingleUseInteractableNotActivated") => None,
        t if t.starts_with("IsSingleUseInteractableActivated")
            || t == "InteractableStateCondition"
            || t.contains("OtherElementActivated") =>
        {
            actor().map(Need::Used)
        }
        _ => None,
    }
}

/// A conversation's payloads, through its sub-graphs (survey.rs reads them the same way).
fn flow_payloads(flows: &Value, root: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    let mut seen = HashSet::new();
    while let Some(f) = stack.pop() {
        if !seen.insert(f.clone()) || seen.len() > 200 {
            continue;
        }
        let node = &flows[f.as_str()];
        out.extend(node["payloads"].as_array().cloned().unwrap_or_default());
        stack.extend(node["subgraphs"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).map(str::to_string));
    }
    out
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str())
        .map(|s| s.rsplit('/').next().unwrap_or(s).to_string())
        .collect()
}

impl Graph {
    /// Every world's survey in `dir`.
    pub fn load(dir: &Path) -> Graph {
        let mut g = Graph::default();
        // What each conversation gives, through its sub-graphs: an NPC's gifts.
        let flows: Value = std::fs::read_to_string(dir.join("flows.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let Ok(files) = std::fs::read_dir(dir) else { return g };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "json") || path.file_name().is_some_and(|n| n == "flows.json") {
                continue;
            }
            let Some(v) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
            else {
                continue;
            };
            let Some(world) = v["world"].as_str() else { continue };
            for a in v["actors"].as_array().into_iter().flatten() {
                g.add(world, a);
                if let (Some(flow), Some(last)) = (a["flow"].as_str(), g.nodes.last_mut()) {
                    if last.name == a["name"].as_str().unwrap_or("") {
                        for p in flow_payloads(&flows, flow) {
                            last.gives_items.extend(strs(&p["items"]));
                            last.gives_tags.extend(strs(&p["tags"]));
                            last.gives_tags.extend(strs(&p["facts"]));
                        }
                    }
                }
            }
        }
        g.index();
        g
    }

    fn add(&mut self, world: &str, a: &Value) {
        let full = a["name"].as_str().unwrap_or("").to_string();
        // A copy per streaming cell: one node.
        if self.by_name.contains_key(&(world.to_string(), full.clone())) {
            return;
        }
        let at = a["at"].as_array().map(|x| x.iter().map(|c| c.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
        let at = at.filter(|x| x.len() == 3).map_or([0.0; 3], |x| [x[0], x[1], x[2]]);
        let mut needs = Vec::new();
        let acts = strs(&a["activators"]);
        if !acts.is_empty() {
            needs.push(Need::Any(acts.into_iter().map(Need::Used).collect()));
        }
        for c in a["conditions"].as_array().into_iter().flatten() {
            needs.extend(need_of(c));
        }
        let items = strs(&a["puzzle"]["items"]);
        // A choice puzzle's slot takes one of several: no single item it needs.
        if !items.is_empty() && a["puzzle"]["expects"].is_null() {
            needs.push(Need::All(items.into_iter().map(Need::Item).collect()));
        }
        let mut gives_tags = strs(&a["payload"]["tags"]);
        gives_tags.extend(strs(&a["payload"]["facts"]));
        self.by_name.insert((world.to_string(), full.clone()), self.nodes.len());
        self.nodes.push(Node {
            world: world.to_string(),
            name: full,
            class: a["class"].as_str().unwrap_or("").to_string(),
            at,
            guid: a["guid"].as_str().map(str::to_string),
            gives_items: strs(&a["payload"]["items"]),
            gives_tags,
            needs,
        });
    }

    fn index(&mut self) {
        for (i, n) in self.nodes.iter().enumerate() {
            for it in &n.gives_items {
                self.item_givers.entry(it.clone()).or_default().push(i);
            }
            for t in &n.gives_tags {
                self.tag_givers.entry(t.clone()).or_default().push(i);
            }
        }
    }

    pub fn node(&self, world: &str, name: &str) -> Option<usize> {
        self.by_name.get(&(world.to_string(), name.to_string())).copied()
    }

    /// The node nearest `at` in `world`, within `reach` (cm).
    pub fn near(&self, world: &str, at: [f32; 3], reach: f32) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.world == world)
            .map(|(i, n)| (i, (n.at[0] - at[0]).hypot(n.at[1] - at[1])))
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn used(&self, i: usize, s: &State) -> bool {
        self.nodes[i].guid.as_ref().is_some_and(|g| s.used.contains(g))
    }

    fn met(&self, n: &Need, world: &str, s: &State) -> bool {
        match n {
            // What keeps no state in the save (an area trigger walked into) cannot be told
            // done: it holds nothing back.
            Need::Used(a) => self.node(world, a).is_none_or(|i| self.nodes[i].guid.is_none() || self.used(i, s)),
            Need::Fact(t, has) => s.known.contains(t) == *has,
            Need::Item(it) => s.held.contains(it),
            Need::All(v) => v.iter().all(|x| self.met(x, world, s)),
            Need::Any(v) => v.is_empty() || v.iter().any(|x| self.met(x, world, s)),
        }
    }

    /// What must be done before node `i`, and the first of it that can be done now: the
    /// chain from `i` back (`i` first) to that node, `None` when nothing can be found
    /// (what it needs is given by no place: the story's scripts).
    pub fn chain(&self, i: usize, s: &State) -> Option<Vec<usize>> {
        let mut seen = HashSet::new();
        self.walk(i, s, &mut seen, 0)
    }

    fn walk(&self, i: usize, s: &State, seen: &mut HashSet<usize>, depth: usize) -> Option<Vec<usize>> {
        if depth > 24 || !seen.insert(i) {
            return None;
        }
        let n = &self.nodes[i];
        let unmet: Vec<&Need> = n.needs.iter().filter(|x| !self.met(x, &n.world, s)).collect();
        if unmet.is_empty() {
            return Some(vec![i]);
        }
        for need in unmet {
            if let Some(mut rest) = self.first_for(need, &n.world, s, seen, depth) {
                rest.insert(0, i);
                return Some(rest);
            }
        }
        None
    }

    /// The chain to the first thing that meets `need`.
    fn first_for(
        &self,
        need: &Need,
        world: &str,
        s: &State,
        seen: &mut HashSet<usize>,
        depth: usize,
    ) -> Option<Vec<usize>> {
        match need {
            Need::Used(a) => self.walk(self.node(world, a)?, s, seen, depth + 1),
            Need::Item(it) => self
                .item_givers
                .get(it)?
                .iter()
                .filter(|&&g| !self.used(g, s))
                .find_map(|&g| self.walk(g, s, seen, depth + 1)),
            Need::Fact(t, true) => self
                .tag_givers
                .get(t)?
                .iter()
                .filter(|&&g| !self.used(g, s))
                .find_map(|&g| self.walk(g, s, seen, depth + 1)),
            // Not knowing something is not a step to take.
            Need::Fact(_, false) => None,
            Need::All(v) => {
                v.iter().filter(|x| !self.met(x, world, s)).find_map(|x| self.first_for(x, world, s, seen, depth))
            }
            Need::Any(v) => v.iter().find_map(|x| self.first_for(x, world, s, seen, depth)),
        }
    }

    /// From nothing known, what can be done, propagated to a fixed point: a node is doable
    /// when each of its needs is met by doable nodes (an item or fact by a doable giver of
    /// it). The rest, with what keeps them out, is the graph's report.
    pub fn reach(&self) -> Report {
        let mut doable = vec![false; self.nodes.len()];
        let mut known: HashSet<&str> = HashSet::new();
        let mut items: HashSet<&str> = HashSet::new();
        loop {
            let mut changed = false;
            for (i, n) in self.nodes.iter().enumerate() {
                if doable[i] || !n.needs.iter().all(|x| self.can(x, &n.world, &doable, &known, &items)) {
                    continue;
                }
                doable[i] = true;
                changed = true;
                known.extend(n.gives_tags.iter().map(String::as_str));
                items.extend(n.gives_items.iter().map(String::as_str));
            }
            if !changed {
                break;
            }
        }
        let mut no_giver: HashMap<String, usize> = HashMap::new();
        for (_, n) in self.nodes.iter().enumerate().filter(|(i, _)| !doable[*i]) {
            for x in &n.needs {
                self.missing(x, &n.world, &known, &items, &mut no_giver);
            }
        }
        Report {
            nodes: self.nodes.len(),
            doable: doable.iter().filter(|d| **d).count(),
            stuck: doable.iter().enumerate().filter(|(_, d)| !**d).map(|(i, _)| i).collect(),
            no_giver,
        }
    }

    fn can(&self, n: &Need, world: &str, doable: &[bool], known: &HashSet<&str>, items: &HashSet<&str>) -> bool {
        match n {
            Need::Used(a) => self.node(world, a).is_some_and(|i| doable[i]),
            Need::Fact(t, true) => known.contains(t.as_str()),
            Need::Fact(_, false) => true,
            Need::Item(it) => items.contains(it.as_str()),
            Need::All(v) => v.iter().all(|x| self.can(x, world, doable, known, items)),
            Need::Any(v) => v.is_empty() || v.iter().any(|x| self.can(x, world, doable, known, items)),
        }
    }

    fn missing(
        &self,
        n: &Need,
        world: &str,
        known: &HashSet<&str>,
        items: &HashSet<&str>,
        out: &mut HashMap<String, usize>,
    ) {
        match n {
            Need::Fact(t, true) if !known.contains(t.as_str()) && !self.tag_givers.contains_key(t) => {
                *out.entry(format!("fact {t}")).or_default() += 1
            }
            Need::Item(it) if !items.contains(it.as_str()) && !self.item_givers.contains_key(it) => {
                *out.entry(format!("item {it}")).or_default() += 1
            }
            Need::Used(a) if self.node(world, a).is_none() => *out.entry(format!("actor {a}")).or_default() += 1,
            Need::All(v) | Need::Any(v) => v.iter().for_each(|x| self.missing(x, world, known, items, out)),
            _ => {}
        }
    }
}

/// How a node reads to the player: what it gives, what goes there, or its class's words.
pub fn label(n: &Node) -> String {
    if let Some(it) = n.gives_items.first() {
        return crate::goals::item_label(it);
    }
    let takes: Vec<String> = n
        .needs
        .iter()
        .flat_map(|x| match x {
            Need::All(v) => v.clone(),
            other => vec![other.clone()],
        })
        .filter_map(|x| match x {
            Need::Item(it) => Some(crate::goals::item_label(&it)),
            _ => None,
        })
        .collect();
    if !takes.is_empty() {
        return trf!("REQ_PUT_HERE", items = takes.join(", "));
    }
    crate::goals::readable(&n.class)
}

impl Graph {
    /// Each goal in `world` whose place needs something first (`chain`) is held back —
    /// `Gate::Conditional`, its detail saying what comes first — and the first thing of its
    /// chain is made a goal of its quest, so the guide goes there instead (a goal already
    /// there takes the quest on). A chain that ends in what no place gives (the story's
    /// scripts) leaves the goal as it is. Returns the chains, for the trace.
    pub fn gate(&self, goals: &mut Vec<crate::goals::Goal>, world: &str, s: &State) -> Vec<(u64, Vec<String>)> {
        use crate::goals::{Gate, Goal};
        let mut firsts: Vec<(usize, Goal, String)> = Vec::new();
        let mut chains = Vec::new();
        for g in goals.iter_mut() {
            let Some(i) = self.near(world, g.at, NEAR) else { continue };
            let Some(chain) = self.chain(i, s) else { continue };
            if chain.len() < 2 {
                continue;
            }
            let names: Vec<String> = chain.iter().map(|&k| label(&self.nodes[k])).collect();
            let text = names.iter().rev().cloned().collect::<Vec<_>>().join(" → ");
            g.gate = Gate::Conditional;
            g.detail = format!("{} · {}", g.detail, trf!("GRAPH_FIRST", chain = text));
            chains.push((g.id, names));
            firsts.push((*chain.last().unwrap(), g.clone(), text));
        }
        for (first, of, text) in firsts {
            self.step(goals, first, &of, &text);
        }
        chains
    }
}

impl Graph {
    /// The first thing of a chain made a goal of `of`'s quest (or a goal already there
    /// taking its quest on).
    fn step(&self, goals: &mut Vec<crate::goals::Goal>, first: usize, of: &crate::goals::Goal, text: &str) {
        use crate::goals::{Gate, Goal};
        let n = &self.nodes[first];
        match goals.iter_mut().find(|g| (g.at[0] - n.at[0]).hypot(g.at[1] - n.at[1]) <= NEAR) {
            Some(g) => {
                for k in &of.keys {
                    if !g.keys.contains(k) {
                        g.keys.push(k.clone());
                    }
                }
                for t in &of.tags {
                    if !g.tags.contains(t) {
                        g.tags.push(t.clone());
                    }
                }
                g.quests.extend(of.quests.iter().copied().filter(|q| !g.quests.contains(q)).collect::<Vec<_>>());
                if g.tier > of.tier {
                    g.tier = of.tier;
                }
            }
            None => goals.push(Goal {
                tier: of.tier,
                id: step_id(n),
                label: label(n),
                detail: trf!("GRAPH_STEP_FOR", chain = text),
                at: n.at,
                quests: of.quests.clone(),
                tags: of.tags.clone(),
                keys: of.keys.clone(),
                gate: Gate::Open,
                named: true,
            }),
        }
    }

    /// Each goal under deadly water now (`pools`, from the live pass) is held back, and the
    /// way to drain it made a goal: the nearest drain in the world not yet used (a node
    /// whose class says `Drain` or that gives a `WaterLevel` tag), through its chain. The
    /// water itself names no drain in the game's data, so the nearest stands in. Returns
    /// how many goals were under water.
    pub fn flood(
        &self,
        goals: &mut Vec<crate::goals::Goal>,
        world: &str,
        pools: &[crate::obstacles::Pool],
        s: &State,
    ) -> usize {
        let under: Vec<usize> = (0..goals.len()).filter(|&i| pools.iter().any(|p| p.holds(goals[i].at))).collect();
        for &i in &under {
            let g = &mut goals[i];
            g.gate = crate::goals::Gate::Conditional;
            g.detail = format!("{} · {}", g.detail, tr!("GRAPH_FLOODED"));
        }
        for i in under.clone() {
            let of = goals[i].clone();
            let drains = self.nodes.iter().enumerate().filter(|(k, n)| {
                n.world == world
                    && !self.used(*k, s)
                    && (n.class.contains("Drain") || n.gives_tags.iter().any(|t| t.contains("WaterLevel")))
            });
            let mut drains: Vec<(usize, f32)> = drains
                .map(|(k, n)| (k, (n.at[0] - of.at[0]).hypot(n.at[1] - of.at[1])))
                .filter(|(_, d)| *d < 40_000.0)
                .collect();
            drains.sort_by(|a, b| a.1.total_cmp(&b.1));
            if let Some(chain) = drains.iter().find_map(|&(k, _)| self.chain(k, s)) {
                let mut names: Vec<String> = chain.iter().map(|&k| label(&self.nodes[k])).collect();
                names.insert(0, of.label.clone());
                let text = names.iter().rev().cloned().collect::<Vec<_>>().join(" → ");
                self.step(goals, *chain.last().unwrap(), &of, &text);
            }
        }
        under.len()
    }
}

/// How near a goal and a node must be to be the same place (cm).
const NEAR: f32 = 150.0;

/// A step goal's id: its node, apart from the survey's and the live ids.
fn step_id(n: &Node) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ("graph", &n.world, &n.name).hash(&mut h);
    h.finish() | 1 << 63
}

/// What `reach` found.
pub struct Report {
    pub nodes: usize,
    pub doable: usize,
    /// The nodes that cannot be reached from nothing known.
    pub stuck: Vec<usize>,
    /// What stuck nodes need that no place gives (the story's scripts give it), and how often.
    pub no_giver: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The Lymbic Forge, as the survey has it: a gear given, a slot taking it, a lever
    /// that works once the slot is used, a drain the lever sets off giving a fact, and a
    /// key behind the fact.
    fn forge() -> Graph {
        let mut g = Graph::default();
        for a in [
            json!({"name": "Gear", "class": "GearLarge_Gather", "at": [0, 0, 0], "guid": "g1", "payload": {"items": ["Gear_Item_DA"]}}),
            json!({"name": "Slot", "class": "LargeGearPlacement", "at": [100, 0, 0], "guid": "g2", "puzzle": {"kind": "placement", "items": ["Gear_Item_DA"]}}),
            json!({"name": "Lever", "class": "ForgeMachineLever", "at": [200, 0, 0], "guid": "g3", "conditions": [{"type": "InteractableStateCondition", "actor": "Slot", "state": 1}]}),
            json!({"name": "Drain", "class": "WaterDrain_PayloadInactive", "at": [300, 0, 0], "guid": "g4", "activators": ["Lever"], "payload": {"tags": ["Water2"]}}),
            json!({"name": "Key", "class": "Key_Gather", "at": [400, 0, 0], "guid": "g5", "conditions": [{"type": "DoesHeroHasFactCondition_BP_C", "tags": ["Water2"]}], "payload": {"items": ["Key_Item_DA"]}}),
        ] {
            g.add("W", &a);
        }
        g.index();
        g
    }

    #[test]
    fn the_chain_runs_back_to_what_can_be_done_now() {
        let g = forge();
        let (mut used, known, mut held) = (HashSet::new(), HashSet::new(), HashSet::new());
        let key = g.node("W", "Key").unwrap();
        let names = |c: Vec<usize>| c.iter().map(|&i| g.nodes[i].name.clone()).collect::<Vec<_>>();
        let s = State { used: &used, known: &known, held: &held };
        assert_eq!(names(g.chain(key, &s).unwrap()), ["Key", "Drain", "Lever", "Slot", "Gear"]);
        held.insert("Gear_Item_DA".to_string());
        let s = State { used: &used, known: &known, held: &held };
        assert_eq!(names(g.chain(key, &s).unwrap()), ["Key", "Drain", "Lever", "Slot"], "the gear held: to the slot");
        used.insert("g2".to_string());
        let s = State { used: &used, known: &known, held: &held };
        assert_eq!(names(g.chain(key, &s).unwrap()), ["Key", "Drain", "Lever"], "the slot used: to the lever");
    }

    #[test]
    fn a_goal_under_water_gives_way_to_the_drain_chain() {
        use crate::goals::{Gate, Goal, Tier};
        let g = forge();
        // The key under water, with no condition of its own (the Lymbic Forge's scholar).
        let mut goals = vec![Goal {
            tier: Tier::Quest,
            id: 1,
            label: "Key".into(),
            detail: String::new(),
            at: [5000.0, 0.0, -100.0],
            quests: vec![],
            tags: vec![],
            keys: vec!["Q2".into()],
            gate: Gate::Open,
            named: true,
        }];
        let pool = crate::obstacles::Pool {
            hull: vec![[4000.0, -1000.0], [6000.0, -1000.0], [6000.0, 1000.0], [4000.0, 1000.0]],
            bottom: -500.0,
            top: 0.0,
        };
        let none = HashSet::new();
        let s = State { used: &none, known: &none, held: &none };
        assert_eq!(g.flood(&mut goals, "W", &[pool], &s), 1);
        assert_eq!(goals[0].gate, Gate::Conditional, "the key held back");
        let step = goals.iter().find(|x| x.at == [0.0, 0.0, 0.0]).expect("the gear made a goal");
        assert_eq!(step.keys, ["Q2"], "of the key's quest");
    }

    #[test]
    fn everything_in_the_forge_can_be_reached_from_nothing() {
        let r = forge().reach();
        assert_eq!((r.nodes, r.doable), (5, 5));
        assert!(r.no_giver.is_empty());
    }
}
