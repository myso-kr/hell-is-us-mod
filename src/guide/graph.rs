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
    /// Its activators, in the order the level lists them (the order to use them in, when
    /// `logic.order`).
    pub activators: Vec<String>,
    /// How its activators must be used, when it says (an order or position puzzle).
    pub logic: Option<Logic>,
    /// Gives what it gives by the story's scripts (a quest listener), not by being used:
    /// no place to go for it.
    pub scripted: bool,
}

/// How a receiver's activators must be used (`MultiActivatorsActivationAction`,
/// `MultiActivatorsStateAction`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Logic {
    /// In the order listed.
    pub order: bool,
    /// Every one of them, not any.
    pub all: bool,
    /// Within this long (s).
    pub timer: Option<f32>,
    /// The position each must be turned to.
    pub solution: Vec<i64>,
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
        // An unset tag reads `None`: no requirement.
        c["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_str())
            .filter(|t| *t != "None")
            .map(str::to_string)
            .collect()
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
        // An object brought to a place (pushed, carried, turned there): that object used.
        t if t.starts_with("IsAnotherSceneComponentAtLocation") => actor().map(Need::Used),
        // A state of 0 is "not used yet": nothing to do first. The level does not write a
        // default value, so no state is 0 too (Jeljin's dial locks, usable while the gazebo
        // panel is still shut).
        "InteractableStateCondition" if c["state"].as_i64().unwrap_or(0) == 0 => None,
        t if t.starts_with("IsSingleUseInteractableActivated")
            || t == "InteractableStateCondition"
            || t.contains("OtherElementActivated") =>
        {
            actor().map(Need::Used)
        }
        _ => None,
    }
}

/// The items a need asks held, through its alls and anys.
fn items_of(n: &Need) -> Vec<String> {
    match n {
        Need::Item(it) => vec![it.clone()],
        Need::All(v) | Need::Any(v) => v.iter().flat_map(items_of).collect(),
        _ => Vec::new(),
    }
}

/// What a payload gives as facts and tags: its own, and the base facts of each Datapad entry
/// (identity) it gives whole (`identities.json`).
fn payload_facts(p: &Value, identities: &Value) -> Vec<String> {
    let mut out = strs(&p["tags"]);
    out.extend(strs(&p["facts"]));
    for id in strs(&p["identities"]) {
        out.extend(strs(&identities[id.as_str()]["facts"]));
    }
    out
}

/// The facts a need asks known, through its alls and anys.
fn facts_of(n: &Need) -> Vec<String> {
    match n {
        Need::Fact(t, true) => vec![t.clone()],
        Need::All(v) | Need::Any(v) => v.iter().flat_map(facts_of).collect(),
        _ => Vec::new(),
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
        // What each Datapad entry gives whole: its base facts.
        let identities: Value = std::fs::read_to_string(dir.join("identities.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        // What each conversation gives, through its sub-graphs: an NPC's gifts.
        let flows: Value = std::fs::read_to_string(dir.join("flows.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let Ok(files) = std::fs::read_dir(dir) else { return g };
        for f in files.flatten() {
            let path = f.path();
            let skip = ["flows.json", "identities.json", "spawners.json", "recipes.json", "vaults.json"];
            if path.extension().is_none_or(|e| e != "json")
                || path.file_name().is_some_and(|n| skip.iter().any(|s| n == *s))
            {
                continue;
            }
            let Some(v) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
            else {
                continue;
            };
            let Some(world) = v["world"].as_str() else { continue };
            // What the world itself gives: on first entering it, and its boss fight won.
            for (kind, class) in [("enter", "WorldFirstEntered"), ("boss", "BossFightWon")] {
                let p = &v["gives"][kind];
                if p.is_null() {
                    continue;
                }
                let at =
                    p["at"].as_array().map(|x| x.iter().map(|c| c.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
                let at = at.filter(|x| x.len() == 3).map_or([0.0; 3], |x| [x[0], x[1], x[2]]);
                let gives_tags = payload_facts(p, &identities);
                let name = format!("{world}:{kind}");
                g.by_name.insert((world.to_string(), name.clone()), g.nodes.len());
                g.nodes.push(Node {
                    world: world.to_string(),
                    name,
                    class: class.to_string(),
                    at,
                    gives_items: strs(&p["items"]),
                    gives_tags,
                    ..Default::default()
                });
            }
            for a in v["actors"].as_array().into_iter().flatten() {
                g.add(world, a, &identities);
                if let (Some(flow), Some(last)) = (a["flow"].as_str(), g.nodes.last_mut()) {
                    if last.name == a["name"].as_str().unwrap_or("") {
                        for p in flow_payloads(&flows, flow) {
                            last.gives_items.extend(strs(&p["items"]));
                            last.gives_tags.extend(payload_facts(&p, &identities));
                        }
                    }
                }
            }
        }
        g.index();
        g
    }

    fn add(&mut self, world: &str, a: &Value, identities: &Value) {
        let full = a["name"].as_str().unwrap_or("").to_string();
        // A copy per streaming cell: one node.
        if self.by_name.contains_key(&(world.to_string(), full.clone())) {
            return;
        }
        let at = a["at"].as_array().map(|x| x.iter().map(|c| c.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
        let at = at.filter(|x| x.len() == 3).map_or([0.0; 3], |x| [x[0], x[1], x[2]]);
        let mut needs = Vec::new();
        let acts = strs(&a["activators"]);
        let l = &a["logic"];
        let logic = (!l.is_null()).then(|| Logic {
            order: l["order"].as_bool().unwrap_or(false),
            all: l["all"].as_bool().unwrap_or(false),
            timer: l["timer"].as_f64().map(|t| t as f32),
            solution: l["solution"].as_array().into_iter().flatten().filter_map(|x| x.as_i64()).collect(),
        });
        if !acts.is_empty() {
            // In order, all of them, or each to its position: every one; else any one.
            let every = logic.as_ref().is_some_and(|l| l.order || l.all || !l.solution.is_empty());
            let each: Vec<Need> = acts.iter().cloned().map(Need::Used).collect();
            needs.push(if every { Need::All(each) } else { Need::Any(each) });
        }
        for c in a["conditions"].as_array().into_iter().flatten() {
            needs.extend(need_of(c));
        }
        let items = strs(&a["puzzle"]["items"]);
        // A choice puzzle's slot takes one of several: no single item it needs.
        if !items.is_empty() && a["puzzle"]["expects"].is_null() {
            needs.push(Need::All(items.into_iter().map(Need::Item).collect()));
        }
        let mut gives_tags = payload_facts(&a["payload"], identities);
        // A quest listener: what its blueprint sets as the story goes.
        let script = strs(&a["script_tags"]);
        let scripted = !script.is_empty();
        gives_tags.extend(script);
        // Each trade an NPC takes: a node of its own, needing the item, giving the reward.
        for (k, t) in a["trades"].as_array().into_iter().flatten().enumerate() {
            let Some(item) = t["item"].as_str() else { continue };
            let item = item.rsplit('/').next().unwrap_or(item).to_string();
            let tags = payload_facts(&t["payload"], identities);
            let name = format!("{full}#trade{k}");
            self.by_name.insert((world.to_string(), name.clone()), self.nodes.len());
            self.nodes.push(Node {
                world: world.to_string(),
                name,
                class: "Trade".into(),
                at,
                gives_items: strs(&t["payload"]["items"]),
                gives_tags: tags,
                needs: vec![Need::Item(item)],
                ..Default::default()
            });
        }
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
            activators: acts,
            logic,
            scripted,
        });
    }

    fn index(&mut self) {
        // A fight's outcome no place gives (`…EncounterKilled`, `…Encounter.Completed`, a
        // boss killed): the fight's script sets it. A node for it where it is needed, so a
        // chain reads "win the fight", not a dead end.
        let given: HashSet<String> = self.nodes.iter().flat_map(|n| n.gives_tags.iter().cloned()).collect();
        let mut fights: Vec<(String, [f32; 3], String)> = Vec::new();
        for n in &self.nodes {
            for t in n.needs.iter().flat_map(facts_of) {
                let fight = ["Encounter", "Killed", "Boss"].iter().any(|k| t.contains(k));
                if fight && !given.contains(&t) && !fights.iter().any(|f| f.0 == n.world && f.2 == t) {
                    fights.push((n.world.clone(), n.at, t));
                }
            }
        }
        let items: HashSet<String> = self.nodes.iter().flat_map(|n| n.gives_items.iter().cloned()).collect();
        let mut story_items: Vec<(String, [f32; 3], String)> = Vec::new();
        for n in &self.nodes {
            for it in n.needs.iter().flat_map(items_of) {
                if !items.contains(&it) && !story_items.iter().any(|f| f.0 == n.world && f.2 == it) {
                    story_items.push((n.world.clone(), n.at, it));
                }
            }
        }
        for (world, at, item) in story_items {
            let name = format!("{world}:story:{item}");
            self.by_name.insert((world.clone(), name.clone()), self.nodes.len());
            self.nodes.push(Node {
                world,
                name,
                class: "StoryGives".into(),
                at,
                gives_items: vec![item],
                scripted: true,
                ..Default::default()
            });
        }
        for (world, at, tag) in fights {
            let name = format!("{world}:fight:{tag}");
            self.by_name.insert((world.clone(), name.clone()), self.nodes.len());
            self.nodes.push(Node {
                world,
                name,
                class: "FightWon".into(),
                at,
                gives_tags: vec![tag],
                scripted: true,
                ..Default::default()
            });
        }
        for (i, n) in self.nodes.iter().enumerate() {
            for it in &n.gives_items {
                self.item_givers.entry(it.clone()).or_default().push(i);
            }
            for t in &n.gives_tags {
                self.tag_givers.entry(t.clone()).or_default().push(i);
            }
        }
    }

    /// Whether some node gives the fact or tag `t`.
    pub fn gives(&self, t: &str) -> bool {
        self.tag_givers.contains_key(t)
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
        let mut depth = vec![None; self.nodes.len()];
        let mut known: HashSet<&str> = HashSet::new();
        let mut items: HashSet<&str> = HashSet::new();
        // In rounds: what a round makes doable counts from the next, so a node's round is how
        // many steps from the start it is (the story's order, roughly).
        for round in 0.. {
            let new: Vec<usize> = (0..self.nodes.len())
                .filter(|&i| {
                    !doable[i]
                        && self.nodes[i]
                            .needs
                            .iter()
                            .all(|x| self.can(x, &self.nodes[i].world, &doable, &known, &items))
                })
                .collect();
            if new.is_empty() {
                break;
            }
            for i in new {
                doable[i] = true;
                depth[i] = Some(round);
                known.extend(self.nodes[i].gives_tags.iter().map(String::as_str));
                items.extend(self.nodes[i].gives_items.iter().map(String::as_str));
            }
        }
        let mut no_giver: HashMap<String, usize> = HashMap::new();
        for (_, n) in self.nodes.iter().enumerate().filter(|(i, _)| !doable[*i]) {
            for x in &n.needs {
                self.missing(x, &n.world, &known, &items, &mut no_giver);
            }
        }
        Report {
            depth,
            nodes: self.nodes.len(),
            doable: doable.iter().filter(|d| **d).count(),
            stuck: doable.iter().enumerate().filter(|(_, d)| !**d).map(|(i, _)| i).collect(),
            no_giver,
        }
    }

    fn can(&self, n: &Need, world: &str, doable: &[bool], known: &HashSet<&str>, items: &HashSet<&str>) -> bool {
        match n {
            Need::Used(a) => self.node(world, a).is_none_or(|i| doable[i]),
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

/// A compass word for the way from `from` to `to` (north is −Y in Hell Is Us).
fn bearing(from: [f32; 3], to: [f32; 3]) -> &'static str {
    let (east, north) = (to[0] - from[0], -(to[1] - from[1]));
    let deg = east.atan2(north).to_degrees().rem_euclid(360.0);
    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][((deg + 22.5) / 45.0) as usize % 8]
}

impl Graph {
    /// What kind of puzzle node `i` is, without its answer: "in order, 4 activators,
    /// 20 s" — for the guide, which keeps answers to the Puzzles page.
    pub fn puzzle_kind(&self, i: usize) -> Option<String> {
        let n = &self.nodes[i];
        let l = n.logic.as_ref()?;
        let count = n.activators.len();
        let mut s = if l.order {
            trf!("GRAPH_IN_ORDER", n = count)
        } else if !l.solution.is_empty() {
            trf!("GRAPH_POSITIONS", n = count)
        } else if l.all {
            trf!("GRAPH_ALL_OF", n = count)
        } else {
            return None;
        };
        if let Some(t) = l.timer {
            s += &trf!("GRAPH_WITHIN", s = t.round() as u32);
        }
        Some(s)
    }

    /// The answer of an order or position puzzle: each activator, numbered in its order,
    /// with its way and distance from the receiver, and the position it must be at.
    pub fn answer(&self, i: usize) -> Option<Vec<String>> {
        let n = &self.nodes[i];
        let l = n.logic.as_ref()?;
        if !(l.order || !l.solution.is_empty()) {
            return None;
        }
        Some(
            n.activators
                .iter()
                .enumerate()
                .map(|(k, a)| {
                    let at = self.node(&n.world, a).map(|j| self.nodes[j].at).unwrap_or(n.at);
                    let m = ((at[0] - n.at[0]).hypot(at[1] - n.at[1]) / 100.0).round() as u32;
                    let place = format!("{} {}m", bearing(n.at, at), m);
                    match l.solution.get(k) {
                        Some(p) => trf!("GRAPH_ANSWER_POSITION", k = k + 1, place = place, p = p),
                        None => trf!("GRAPH_ANSWER_STEP", k = k + 1, place = place),
                    }
                })
                .collect(),
        )
    }
}

/// A door, lock, panel or slot of a world that is still shut, and the first thing to do to
/// open it (a goal, made if need be): what the guide goes to when a route runs through it.
#[derive(Clone, Debug, PartialEq)]
pub struct DoorStep {
    pub at: [f32; 3],
    pub label: String,
    /// Its chain's first step as a goal (the goal there, or one made for it), for the guide
    /// to take up when a route runs through the barrier.
    pub goal: crate::goals::Goal,
    /// The chain, first thing first ("the owl key → the door").
    pub chain: String,
}

/// What a class's name says stands in the way until used.
const BARRIERS: [&str; 8] = ["Door", "KeyLocked", "LymbicLock", "Panel", "Gate", "Placement", "Keypad", "DialPuzzle"];

impl Graph {
    /// The barriers of `world` not opened yet whose opening needs something first, each with
    /// the goal of its chain's first step (added to `goals` when there is none there).
    pub fn door_steps(&self, goals: &[crate::goals::Goal], world: &str, s: &State) -> Vec<DoorStep> {
        let mut out = Vec::new();
        for i in 0..self.nodes.len() {
            let n = &self.nodes[i];
            if n.world != world || self.used(i, s) || !BARRIERS.iter().any(|b| n.class.contains(b)) {
                continue;
            }
            // What opens it, first thing first; nothing needed: the barrier itself, to open.
            let Some(chain) = self.chain(i, s) else { continue };
            let first = *chain.last().unwrap();
            if self.nodes[first].scripted {
                continue;
            }
            let text = chain.iter().rev().map(|&k| label(&self.nodes[k])).collect::<Vec<_>>().join(" → ");
            let f = &self.nodes[first];
            // The goal there, or one made for the step (not put in the goals: only taken up
            // when a route runs through the barrier).
            let goal = match goals.iter().find(|g| (g.at[0] - f.at[0]).hypot(g.at[1] - f.at[1]) <= NEAR) {
                Some(g) => g.clone(),
                None => crate::goals::Goal {
                    tier: crate::goals::Tier::Quest,
                    id: step_id(f),
                    label: label(f),
                    detail: trf!("GRAPH_STEP_FOR", chain = text),
                    at: f.at,
                    quests: vec![],
                    tags: vec![],
                    keys: vec![],
                    gate: crate::goals::Gate::Open,
                    named: true,
                    reveals: Default::default(),
                },
            };
            out.push(DoorStep { at: n.at, label: label(n), goal, chain: text });
        }
        out
    }
}

/// An order or position puzzle of a world (the Puzzles page): where, what kind, solved,
/// and its answer (shown on asking).
#[derive(Clone, Debug, PartialEq)]
pub struct LogicPuzzle {
    pub id: u64,
    pub world: String,
    pub at: [f32; 3],
    pub label: String,
    pub kind: String,
    pub answer: Vec<String>,
    pub solved: bool,
}

impl Graph {
    /// The order and position puzzles of `world`.
    pub fn logic_puzzles(&self, world: &str, s: &State) -> Vec<LogicPuzzle> {
        (0..self.nodes.len())
            .filter(|&i| self.nodes[i].world == world)
            .filter_map(|i| {
                let n = &self.nodes[i];
                Some(LogicPuzzle {
                    id: step_id(n),
                    world: n.world.clone(),
                    at: n.at,
                    label: place_label(n),
                    kind: self.puzzle_kind(i)?,
                    answer: self.answer(i)?,
                    solved: self.used(i, s),
                })
            })
            .collect()
    }
}

/// How a node reads to the player: what it gives, what goes there, or its class's words.
pub fn label(n: &Node) -> String {
    if n.class == "FightWon" {
        return tr!("GRAPH_FIGHT").to_string();
    }
    if n.scripted {
        return tr!("GRAPH_STORY").to_string();
    }
    if n.class == "Trade" {
        let wants: Vec<String> = n
            .needs
            .iter()
            .filter_map(|x| match x {
                Need::Item(it) => Some(crate::goals::item_label(it)),
                _ => None,
            })
            .collect();
        return trf!("GRAPH_TRADE", item = wants.join(", "));
    }
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
    // What kind of thing it is, in words, where the class says it; else its class's words.
    place_label(n)
}

/// A node's place, from its class's words without the words every such class has
/// (`Eye Of God Multi Activators Receiver` → `Eye Of God`), with its kind's word when the
/// class says one (`Lake Cynon · Lymbic lock`).
pub fn place_label(n: &Node) -> String {
    const GENERIC: [&str; 22] = [
        "Multi",
        "Activators",
        "Activator",
        "Receiver",
        "Interact",
        "Interactable",
        "Logic",
        "Gate",
        "Inactive",
        "Payload",
        "Panel",
        "Lymbic",
        "Lock",
        "1st",
        "Gen",
        "Puzzle",
        "Dial",
        "Keypad",
        "Placement",
        "Trigger",
        "No",
        "Actions",
    ];
    let words: Vec<String> =
        crate::goals::readable(&n.class).split(' ').filter(|w| !GENERIC.contains(w)).map(str::to_string).collect();
    let place = words.join(" ").trim().to_string();
    let kind = KINDS.iter().find(|(part, _)| n.class.contains(part)).map(|(_, key)| crate::i18n::text(key));
    match (place.is_empty(), kind) {
        (false, Some(k)) => format!("{place} · {k}"),
        (false, None) => place,
        (true, Some(k)) => k,
        (true, None) => crate::goals::readable(&n.class),
    }
}

/// What a class's name says a thing is (the first that fits), and its words' key.
const KINDS: [(&str, &str); 10] = [
    ("LymbicLock_1stGen", "LYMBIC_LOCK"),
    ("1stGenLymbicActivator", "GRAPH_LYMBIC_ACTIVATOR"),
    ("LymbicLockPanel", "LYMBIC_LOCK"),
    ("DroneTranslation", "DRONE_TRANSLATION"),
    ("Keypad", "GRAPH_KEYPAD"),
    ("DialPuzzle", "GRAPH_DIAL"),
    ("Lever", "GRAPH_LEVER"),
    ("KeyLocked", "GRAPH_LOCKED_DOOR"),
    ("Door", "DOOR"),
    ("Panel", "GRAPH_PANEL"),
];

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
            let names: Vec<String> = chain
                .iter()
                .map(|&k| match self.puzzle_kind(k) {
                    Some(kind) => format!("{} ({kind})", label(&self.nodes[k])),
                    None => label(&self.nodes[k]),
                })
                .collect();
            let text = names.iter().rev().cloned().collect::<Vec<_>>().join(" → ");
            g.gate = Gate::Conditional;
            g.detail = format!("{} · {}", g.detail, trf!("GRAPH_FIRST", chain = text));
            chains.push((g.id, names));
            let first = *chain.last().unwrap();
            // The story's scripts give it: nowhere to send the hero.
            if !self.nodes[first].scripted {
                firsts.push((first, g.clone(), text));
            }
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
                // An order or position puzzle's device gives its answer away; else what the
                // goal it is for gives away.
                reveals: if self.puzzle_kind(first).is_some() { crate::goals::Reveal::Answers } else { of.reveals },
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

impl Graph {
    /// For each quest (`QuestNN` in its facts' names) of the Datapad's facts (`facts.tsv`):
    /// how many facts it has, and how many some node gives, by what kind of giver — a place,
    /// a conversation, the story (scripts, fights), a world (first entry, boss fight).
    pub fn quest_coverage(&self, facts: &[String]) -> Vec<Coverage> {
        let mut by_quest: std::collections::BTreeMap<String, Vec<&String>> = Default::default();
        for f in facts {
            if let Some(i) = f.find("Quest") {
                let digits: String = f[i + 5..].chars().take_while(|c| c.is_ascii_digit()).collect();
                if digits.len() == 2 {
                    by_quest.entry(format!("Quest{digits}")).or_default().push(f);
                }
            }
        }
        by_quest
            .into_iter()
            .map(|(q, fs)| {
                let mut kinds: HashMap<&'static str, usize> = HashMap::new();
                let mut none = Vec::new();
                for f in &fs {
                    let kind = self.tag_givers.get(f.as_str()).and_then(|g| g.first()).map(|&i| {
                        let n = &self.nodes[i];
                        if n.class == "WorldFirstEntered" || n.class == "BossFightWon" {
                            "world"
                        } else if n.scripted {
                            "story"
                        } else if n.class == "Trade"
                            || n.class.contains("NPC")
                            || n.class.contains("Convo")
                            || n.class.contains("Quickchat")
                        {
                            "conversation"
                        } else {
                            "place"
                        }
                    });
                    match kind {
                        Some(k) => *kinds.entry(k).or_default() += 1,
                        None => none.push((*f).clone()),
                    }
                }
                (q, fs.len(), kinds, none)
            })
            .collect()
    }
}

/// A quest's facts: the quest, how many, how many each kind of giver gives, those none gives.
pub type Coverage = (String, usize, HashMap<&'static str, usize>, Vec<String>);

impl Graph {
    /// The story's order as the graph has it: for each world, the round its world-map entry
    /// is first given (it can be travelled to); for each main quest, the rounds its facts are
    /// first given, earliest and latest.
    pub fn timeline(&self, r: &Report) -> Timeline {
        let first = |pred: &dyn Fn(&Node) -> bool| {
            self.nodes.iter().enumerate().filter(|(_, n)| pred(n)).filter_map(|(i, _)| r.depth[i]).min()
        };
        let mut worlds: Vec<String> = self.nodes.iter().map(|n| n.world.clone()).collect();
        worlds.sort();
        worlds.dedup();
        let mut opened: Vec<(String, usize)> = worlds
            .into_iter()
            .map(|w| {
                let fact = format!("WMA_{w}_Travel_WorldLocation");
                let at = first(&|n: &Node| n.gives_tags.iter().any(|t| t.starts_with(&fact))).unwrap_or(0);
                (w, at)
            })
            .collect();
        opened.sort_by_key(|x| x.1);
        let mut quests: Vec<(String, usize, usize)> = (1..=6)
            .filter_map(|q| {
                let key = format!("Quest0{q}");
                let rounds: Vec<usize> = self
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.gives_tags.iter().any(|t| t.contains(&key)))
                    .filter_map(|(i, _)| r.depth[i])
                    .collect();
                Some((key, *rounds.iter().min()?, *rounds.iter().max()?))
            })
            .collect();
        quests.sort_by_key(|x| x.1);
        (opened, quests)
    }
}

/// The graph's order: each world and the round its map entry is first given; each main
/// quest and the first and last rounds of its facts.
pub type Timeline = (Vec<(String, usize)>, Vec<(String, usize, usize)>);

/// What `reach` found.
pub struct Report {
    /// Each node's round from the start (`None`: never).
    pub depth: Vec<Option<usize>>,
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
            g.add("W", &a, &Value::Null);
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
            reveals: Default::default(),
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
    fn a_place_reads_as_its_words_and_its_kind() {
        let n = |class: &str| Node { class: class.into(), ..Default::default() };
        assert_eq!(place_label(&n("EyeOfGodMultiActivatorsReceiver_BP_C")), "Eye Of God");
        assert_eq!(place_label(&n("ForgeFoyerMultiActivatorsReceiver_BP_C")), "Forge Foyer");
        let lock = format!("Lake Cynon · {}", crate::i18n::text("LYMBIC_LOCK"));
        assert_eq!(place_label(&n("LakeCynon_LymbicLock_1stGenReceiver_BP_C")), lock);
    }

    #[test]
    fn everything_in_the_forge_can_be_reached_from_nothing() {
        let r = forge().reach();
        assert_eq!((r.nodes, r.doable), (5, 5));
        assert!(r.no_giver.is_empty());
    }
}
