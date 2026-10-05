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
    /// A door that opens from one side only ("locked from the other side"): where the hero
    /// must stand to open it (the survey's `opens_from`).
    pub opens_from: Option<[f32; 3]>,
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

/// A condition of a conversation's way to a payload, as the survey writes it (`Flows` in the
/// survey tool): a fact or tag known or not, a topic's identity known, all or any of them.
fn need_of_flow(v: &Value) -> Option<Need> {
    if let Some(f) = v["fact"].as_str() {
        return Some(Need::Fact(f.to_string(), v["has"].as_bool().unwrap_or(true)));
    }
    // A topic's question names its subject (the portrait it shows), it does not gate it: the
    // topic opens on its `Conversation.TopicsUnlock…` tag (checked in the flow itself).
    if v["identity"].is_string() {
        return None;
    }
    for (key, all) in [("all", true), ("any", false)] {
        if let Some(xs) = v[key].as_array() {
            let each: Vec<Need> = xs.iter().filter_map(need_of_flow).collect();
            return (!each.is_empty()).then_some(if all { Need::All(each) } else { Need::Any(each) });
        }
    }
    None
}

/// A conversation's payloads, each with what must hold on the way to it from the start,
/// through its topics and sub-graphs. A survey from before the conditions were read has
/// none: every payload as if always given.
fn flow_gated(flows: &Value, root: &str) -> Vec<(Value, Option<Need>)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_string(), None::<Need>)];
    // The person's introduction, said first: the actor names only the conversation's root,
    // and the introduction sits beside it (`X_ConvoRoot_FA` → `X_ConvoIntro_FA`, V2 too).
    let intro = root.replace("_ConvoRoot", "_ConvoIntro");
    if intro != root && !flows[intro.as_str()].is_null() {
        stack.push((intro, None));
    }
    let mut seen = HashSet::new();
    while let Some((f, on_way)) = stack.pop() {
        if !seen.insert(f.clone()) || seen.len() > 400 {
            continue;
        }
        let node = &flows[f.as_str()];
        let both = |a: &Option<Need>, b: Option<Need>| match (a.clone(), b) {
            (None, b) => b,
            (a, None) => a,
            (Some(a), Some(b)) => Some(Need::All(vec![a, b])),
        };
        match node["gated"].as_array() {
            Some(gated) => {
                for g in gated {
                    out.push((g["payload"].clone(), both(&on_way, need_of_flow(&g["need"]))));
                }
                for s in node["gated_subgraphs"].as_array().into_iter().flatten() {
                    if let Some(a) = s["asset"].as_str() {
                        stack.push((a.to_string(), both(&on_way, need_of_flow(&s["need"]))));
                    }
                }
            }
            None => {
                out.extend(node["payloads"].as_array().into_iter().flatten().map(|p| (p.clone(), on_way.clone())));
                for s in node["subgraphs"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
                    stack.push((s.to_string(), on_way.clone()));
                }
            }
        }
    }
    out
}

/// A conversation's payloads, through its sub-graphs (survey.rs reads them the same way).
#[allow(dead_code)]
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
                // A boss that gives a main quest's facts is fought in that quest: not before
                // it has begun (Senedra's boss gives Quest03's keystone).
                let mut needs = Vec::new();
                if kind == "boss" {
                    let mut quests: Vec<String> =
                        gives_tags.iter().filter_map(|t| t.find("Quest0").map(|i| t[i..i + 7].to_string())).collect();
                    quests.sort();
                    quests.dedup();
                    needs.extend(quests.into_iter().map(|q| Need::Fact(format!("{q}_Started_StatusFact_DA"), true)));
                }
                let name = format!("{world}:{kind}");
                g.by_name.insert((world.to_string(), name.clone()), g.nodes.len());
                g.nodes.push(Node {
                    world: world.to_string(),
                    name,
                    class: class.to_string(),
                    at,
                    gives_items: strs(&p["items"]),
                    gives_tags,
                    needs,
                    ..Default::default()
                });
            }
            for a in v["actors"].as_array().into_iter().flatten() {
                g.add(world, a, &identities);
                // What a person says: a node for each thing their conversation gives, needing
                // what must hold on the way to it (a quest begun, a topic known, a tag), not
                // everything at once from the start (GRAPH.md §13).
                let Some(flow) = a["flow"].as_str() else { continue };
                let Some(last) = g.nodes.last().cloned() else { continue };
                if last.name != a["name"].as_str().unwrap_or("") {
                    continue;
                }
                let mut said = HashSet::new();
                for (p, need) in flow_gated(&flows, flow) {
                    if !said.insert(format!("{p}{need:?}")) {
                        continue;
                    }
                    let name = format!("{}#say{}", last.name, said.len() - 1);
                    // A topic opened by a tag is marked spoken once talked through (the game's
                    // conversations set `<tag>.Spoken`; later topics ask for it).
                    let mut gives_tags = payload_facts(&p, &identities);
                    for t in need.as_ref().map(facts_of).unwrap_or_default() {
                        if t.starts_with("Conversation.TopicsUnlock.") && !t.contains(".Spoken") {
                            gives_tags.push(format!("{t}.Spoken"));
                        }
                    }
                    g.by_name.insert((world.to_string(), name.clone()), g.nodes.len());
                    g.nodes.push(Node {
                        world: world.to_string(),
                        name,
                        class: "Say".into(),
                        at: last.at,
                        gives_items: strs(&p["items"]),
                        gives_tags,
                        needs: need.into_iter().collect(),
                        ..Default::default()
                    });
                }
            }
        }
        // A quest listener names the tags its blueprint uses, set or waited for alike
        // (Jova's grieving father waits for `Act01Complete` to change the burial). One that
        // something else gives, it waits for; only what nothing else gives is its own outcome
        // (a boss killed, a photo taken).
        let by_others: HashSet<String> =
            g.nodes.iter().filter(|n| !n.scripted).flat_map(|n| n.gives_tags.iter().cloned()).collect();
        for n in g.nodes.iter_mut().filter(|n| n.scripted && n.class.contains("QuestListener")) {
            n.gives_tags.retain(|t| !by_others.contains(t));
        }
        // A region is travelled to by the APC once its transition is known (the region's
        // `WMA_<world>_Travel_BifrostTransitionFact`, a base fact of its travel identity that
        // conversations and notes give): until then nothing in it can be done. A region with
        // no such fact given anywhere (the first one, those walked into) has no gate.
        let given: HashSet<String> = g.nodes.iter().flat_map(|n| n.gives_tags.iter().cloned()).collect();
        let worlds: HashSet<String> = g.nodes.iter().map(|n| n.world.clone()).collect();
        for w in worlds {
            let travel = format!("WMA_{w}_Travel_BifrostTransitionFact_DA");
            if !given.contains(&travel) {
                continue;
            }
            for n in g.nodes.iter_mut().filter(|n| n.world == w) {
                n.needs.push(Need::Fact(travel.clone(), true));
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
        // Packed level instances (`…_PLI_C`) are baked art, not actors: one names a floor as its
        // activator (a designer's slip in Senedra's cells); nothing can set it off.
        let acts: Vec<String> = strs(&a["activators"]).into_iter().filter(|x| !x.contains("_PLI_C")).collect();
        let class = a["class"].as_str().unwrap_or("");
        let l = &a["logic"];
        let logic = (!l.is_null()).then(|| Logic {
            order: l["order"].as_bool().unwrap_or(false),
            all: l["all"].as_bool().unwrap_or(false),
            timer: l["timer"].as_f64().map(|t| t as f32),
            solution: l["solution"].as_array().into_iter().flatten().filter_map(|x| x.as_i64()).collect(),
        });
        if !acts.is_empty() {
            // In order, all of them, or each to its position: every one; a multi-activator
            // receiver every one too (the forge's four keystones, its two keys), though the
            // survey reads no logic for most; else any one (an elevator's call levers).
            let every = logic.as_ref().is_some_and(|l| l.order || l.all || !l.solution.is_empty())
                || class.contains("MultiActivator") && class.contains("Receiver");
            let each: Vec<Need> = acts.iter().cloned().map(Need::Used).collect();
            needs.push(if every { Need::All(each) } else { Need::Any(each) });
        }
        for c in a["conditions"].as_array().into_iter().flatten() {
            needs.extend(need_of(c));
        }
        let items = strs(&a["puzzle"]["items"]);
        // A slot takes its item. A choice puzzle's slot that counts one as right takes that one
        // (the Eye of God's orbs); a decoy (`expects` empty) is left empty: nothing to bring
        // (Vyssa's fifteen wrong flower spots; needing any of their items closed a loop that
        // left 1627 nodes stuck).
        match a["puzzle"]["expects"].as_str() {
            Some(e) if !e.is_empty() => needs.push(Need::Item(e.rsplit('/').next().unwrap_or(e).to_string())),
            Some(_) => {}
            None if !items.is_empty() => needs.push(Need::All(items.into_iter().map(Need::Item).collect())),
            None => {}
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
            opens_from: a["opens_from"]
                .as_array()
                .filter(|x| x.len() == 3)
                .map(|x| std::array::from_fn(|k| x[k].as_f64().unwrap_or(0.0) as f32)),
        });
    }

    fn index(&mut self) {
        // A slot on a pillar that rises (the Eye of God's last slot sits on the pillar its orbs
        // raise): usable once the pillar is up. Only pillars: a keystone slot also sits by the
        // panel it sets off itself, and needing that would close a loop.
        let pillars: Vec<(String, String, [f32; 3])> = self
            .nodes
            .iter()
            .filter(|n| n.class.contains("Pillar") && n.class.contains("Receiver"))
            .map(|n| (n.world.clone(), n.name.clone(), n.at))
            .collect();
        for n in self.nodes.iter_mut().filter(|n| n.class.contains("Placement")) {
            if let Some((_, pillar, _)) = pillars.iter().find(|(w, _, at)| {
                *w == n.world
                    && (at[0] - n.at[0]).hypot(at[1] - n.at[1]) < PILLAR_NEAR
                    && (at[2] - n.at[2]).abs() < PILLAR_NEAR
            }) {
                n.needs.push(Need::Used(pillar.clone()));
            }
        }
        // A main quest's facts are given within it: what gives them needs it begun, where
        // something begins it (the Eye of God's orbs and Act 3's end wait for Quest06, begun
        // when all four keystones are in; a place behind a door the data does not tell of).
        // Not what begins it.
        let started: HashSet<String> = self
            .nodes
            .iter()
            .flat_map(|n| n.gives_tags.iter())
            .filter(|t| t.ends_with("_Started_StatusFact_DA"))
            .cloned()
            .collect();
        for n in self.nodes.iter_mut() {
            if n.gives_tags.iter().any(|t| t.ends_with("_Started_StatusFact_DA")) {
                continue;
            }
            let mut quests: Vec<String> = n
                .gives_tags
                .iter()
                .filter_map(|t| t.find("Quest0").map(|i| t[i..].chars().take(7).collect::<String>()))
                .filter(|q| q != "Quest01")
                .collect();
            quests.sort();
            quests.dedup();
            for q in quests {
                let begun = format!("{q}_Started_StatusFact_DA");
                if started.contains(&begun) {
                    n.needs.push(Need::Fact(begun, true));
                }
            }
        }
        // A person "used" (a cinematic after a conversation): something their conversation says
        // known, which the save holds (a person keeps no state of their own).
        let said: HashMap<(String, String), Vec<String>> = {
            let mut m: HashMap<(String, String), Vec<String>> = HashMap::new();
            for n in self.nodes.iter().filter(|n| n.class == "Say") {
                if let (Some((who, _)), Some(first)) = (n.name.split_once("#say"), n.gives_tags.first()) {
                    m.entry((n.world.clone(), who.to_string())).or_default().push(first.clone());
                }
            }
            m
        };
        fn person(need: &mut Need, world: &str, said: &HashMap<(String, String), Vec<String>>) {
            match need {
                Need::Used(a) => {
                    if let Some(tags) = said.get(&(world.to_string(), a.clone())) {
                        *need = Need::Any(tags.iter().map(|t| Need::Fact(t.clone(), true)).collect());
                    }
                }
                Need::All(v) | Need::Any(v) => v.iter_mut().for_each(|x| person(x, world, said)),
                _ => {}
            }
        }
        for n in self.nodes.iter_mut() {
            let world = n.world.clone();
            n.needs.iter_mut().for_each(|x| person(x, &world, &said));
        }
        // A device that needs the state of the receiver it sets off (an elevator's call lever
        // needs the elevator at the other floor): where the receiver is, not what comes
        // first. Kept, it closes a loop (lever → elevator → lever) no chain gets out of.
        let mut drop: Vec<(usize, String)> = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            for r in self.nodes.iter().filter(|r| r.world == n.world && r.activators.contains(&n.name)) {
                drop.push((i, r.name.clone()));
            }
        }
        for (i, receiver) in drop {
            fn strip(need: &mut Need, receiver: &str) -> bool {
                match need {
                    Need::Used(a) => a == receiver,
                    Need::All(v) | Need::Any(v) => {
                        v.retain_mut(|x| !strip(x, receiver));
                        v.is_empty()
                    }
                    _ => false,
                }
            }
            self.nodes[i].needs.retain_mut(|x| !strip(x, &receiver));
        }
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
        // What is still needed and given by nothing in the game's data: the game's code gives
        // it (a lore entry's topic opened, `LoreTopic.…`). A node of its own, so no chain stops
        // there unexplained; `doctor graph` lists them (GRAPH.md §13).
        let fight_tags: HashSet<&String> = fights.iter().map(|f| &f.2).collect();
        let mut by_code: Vec<(String, [f32; 3], String)> = Vec::new();
        for n in &self.nodes {
            for t in n.needs.iter().flat_map(facts_of) {
                if !given.contains(&t)
                    && !fight_tags.contains(&t)
                    && !by_code.iter().any(|f| f.0 == n.world && f.2 == t)
                {
                    by_code.push((n.world.clone(), n.at, t));
                }
            }
        }
        for (world, at, tag) in by_code {
            let name = format!("{world}:code:{tag}");
            self.by_name.insert((world.clone(), name.clone()), self.nodes.len());
            self.nodes.push(Node {
                world,
                name,
                class: "CodeGives".into(),
                at,
                gives_tags: vec![tag],
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

    /// The shut door the hero stands at (within `DOOR_HERE` across and `DOOR_HERE_DZ` up or
    /// down), and the chain of what opens it: the door first, the first thing to do last. A
    /// door is told by its name; one already used, or that needs nothing now, is none. The
    /// guide names a door only when a route runs into it; standing at one, the player was told
    /// nothing of what it takes (seen in play: a door that takes a hammer from a boss's tomb).
    pub fn door_here(&self, world: &str, at: [f32; 3], s: &State) -> Option<(usize, Vec<usize>)> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.world == world && n.class.contains("Door"))
            .map(|(i, n)| (i, (n.at[0] - at[0]).hypot(n.at[1] - at[1]), (n.at[2] - at[2]).abs()))
            .filter(|&(_, d, dz)| d <= DOOR_HERE && dz <= DOOR_HERE_DZ)
            .filter(|&(i, ..)| !self.used(i, s))
            .filter_map(|(i, d, _)| self.chain(i, s).filter(|c| c.len() > 1).map(|c| (d, i, c)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i, c)| (i, c))
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
/// One step of a puzzle's answer: its line, and how the device looks set right, where the
/// game shows that by a picture rather than a number (ui/svg.rs `face`).
#[derive(Clone, Debug, PartialEq)]
pub struct AnswerStep {
    pub text: String,
    pub face: Option<Face>,
}

/// How a device set right looks in the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// A sconce lever (`…Sconce_LeverDualPosition…`): lit, or left out. The solution's 1 and 0
    /// (the Sconces of Knowledge, the Forge Foyer).
    Sconce(bool),
    /// The Hermit's plinths (Acasa Marshes): each turned to show its glyph inward, to the
    /// platform's middle — by the plinth's side of it, as the guides have it: east the up and
    /// down arrows, north the bow, west the ring open at the top, south the claws.
    Hermit(Glyph),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Arrows,
    Bow,
    Ring,
    Claws,
}

impl Face {
    pub fn label(self) -> String {
        match self {
            Face::Sconce(true) => tr!("FACE_SCONCE_LIT").to_string(),
            Face::Sconce(false) => tr!("FACE_SCONCE_OUT").to_string(),
            Face::Hermit(g) => trf!(
                "FACE_INWARD",
                glyph = match g {
                    Glyph::Arrows => tr!("GLYPH_ARROWS"),
                    Glyph::Bow => tr!("GLYPH_BOW"),
                    Glyph::Ring => tr!("GLYPH_RING"),
                    Glyph::Claws => tr!("GLYPH_CLAWS"),
                }
            ),
        }
    }
}

/// How the device `class` of the puzzle `receiver` (standing at `at`, the puzzle at `centre`)
/// looks set to `position`, where the game shows it by a picture.
fn face(receiver: &str, class: &str, centre: [f32; 3], at: [f32; 3], position: Option<i64>) -> Option<Face> {
    if class.contains("LeverDualPosition") {
        return position.map(|p| Face::Sconce(p != 0));
    }
    if receiver.contains("HermitSecret") {
        let (east, north) = (at[0] - centre[0], -(at[1] - centre[1]));
        let g = if east.abs() >= north.abs() {
            if east > 0.0 {
                Glyph::Arrows
            } else {
                Glyph::Ring
            }
        } else if north > 0.0 {
            Glyph::Bow
        } else {
            Glyph::Claws
        };
        return Some(Face::Hermit(g));
    }
    None
}

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
    pub fn answer(&self, i: usize) -> Option<Vec<AnswerStep>> {
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
                    let device = self.node(&n.world, a).map(|j| &self.nodes[j]);
                    let at = device.map_or(n.at, |d| d.at);
                    let m = ((at[0] - n.at[0]).hypot(at[1] - n.at[1]) / 100.0).round() as u32;
                    let place = format!("{} {}m", bearing(n.at, at), m);
                    let class = device.map_or("", |d| d.class.as_str());
                    match (l.solution.get(k), face(&n.class, class, n.at, at, l.solution.get(k).copied())) {
                        // as the game shows it: a sconce lit or not, a plinth's glyph
                        (_, Some(f)) => {
                            AnswerStep { text: format!("{}. {place}: {}", k + 1, f.label()), face: Some(f) }
                        }
                        (Some(p), None) => AnswerStep {
                            text: trf!("GRAPH_ANSWER_POSITION", k = k + 1, place = place, p = p),
                            face: None,
                        },
                        (None, None) => {
                            AnswerStep { text: trf!("GRAPH_ANSWER_STEP", k = k + 1, place = place), face: None }
                        }
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
    /// A one-sided door: the side it opens from. From the other side its step is not one
    /// to take up (the overlay).
    pub opens_from: Option<[f32; 3]>,
}

impl Graph {
    /// The doors and gates of `world` the hero can pass: where each stands, and for a
    /// one-sided door still shut the side it opens from (navmesh.rs `bridged`). The game's
    /// navmesh has every door closed, so these are ways it does not know: one opened, one
    /// that opens with nothing more than a press (or whose needs are met now), and a
    /// one-sided door from the side it opens from. A door that needs something not had
    /// yet (a key, a lever, a puzzle) is a wall until it does.
    pub fn passable(&self, world: &str, s: &State) -> Vec<([f32; 3], Option<[f32; 3]>)> {
        (0..self.nodes.len())
            .filter_map(|i| {
                let n = &self.nodes[i];
                if n.world != world || !(n.class.contains("Door") || n.class.contains("Gate")) {
                    return None;
                }
                if self.used(i, s) {
                    return Some((n.at, None));
                }
                if let Some(from) = n.opens_from {
                    return Some((n.at, Some(from)));
                }
                // Openable now: its chain is itself.
                (!n.scripted && self.chain(i, s).is_some_and(|c| c == [i])).then_some((n.at, None))
            })
            .collect()
    }

    /// The elevators of `world` that run now, each as its stops (where its call levers stand,
    /// lowest first): an elevator's receiver (`…ElevatorTwoFloors…Receiver…`) is used by the
    /// levers that call it to each floor. One that takes an item first (a gear to fit) is left
    /// out until it is held. The navmesh has no way between an elevator's floors
    /// (navmesh.rs `lifts`): without them a floor reached by one was no floor at all.
    pub fn lifts(&self, world: &str, s: &State) -> Vec<Vec<[f32; 3]>> {
        fn flat(n: &Need, out: &mut Vec<Need>) {
            match n {
                Need::All(v) | Need::Any(v) => v.iter().for_each(|x| flat(x, out)),
                other => out.push(other.clone()),
            }
        }
        let mut out = Vec::new();
        for n in self
            .nodes
            .iter()
            .filter(|n| n.world == world && n.class.contains("Elevator") && n.class.contains("Receiver"))
        {
            let mut needs = Vec::new();
            n.needs.iter().for_each(|x| flat(x, &mut needs));
            if needs.iter().any(|x| matches!(x, Need::Item(it) if !s.held.contains(it))) {
                continue;
            }
            let mut stops: Vec<[f32; 3]> = needs
                .iter()
                .filter_map(|x| match x {
                    Need::Used(a) => self.node(world, a).map(|j| &self.nodes[j]),
                    _ => None,
                })
                .filter(|l| l.class.contains("Elevator"))
                .map(|l| l.at)
                .collect();
            stops.sort_by(|a, b| a[2].total_cmp(&b[2]));
            // one stop a floor: levers at one stop (call and ride) are one
            stops.dedup_by(|a, b| (a[2] - b[2]).abs() < LIFT_FLOOR);
            if stops.len() >= 2 {
                out.push(stops);
            }
        }
        out
    }

    /// The one-sided doors of `world` still shut: where each stands and the side it opens
    /// from (navmesh.rs `one_way`).
    pub fn one_way(&self, world: &str, s: &State) -> Vec<[[f32; 3]; 2]> {
        (0..self.nodes.len())
            .filter(|&i| self.nodes[i].world == world && !self.used(i, s))
            .filter_map(|i| Some([self.nodes[i].at, self.nodes[i].opens_from?]))
            .collect()
    }
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
            // Not a goal there that is held back itself (it waits on this same step).
            let goal = match goals.iter().find(|g| {
                g.gate != crate::goals::Gate::Conditional && (g.at[0] - f.at[0]).hypot(g.at[1] - f.at[1]) <= NEAR
            }) {
                // A one-sided door: to the side it opens from.
                Some(g) => crate::goals::Goal { at: self.stand(first), ..g.clone() },
                None => crate::goals::Goal {
                    tier: crate::goals::Tier::Quest,
                    id: step_id(f),
                    label: label(f),
                    detail: match f.opens_from {
                        Some(_) => {
                            format!("{} · {}", trf!("GRAPH_STEP_FOR", chain = text), tr!("GRAPH_OPENS_FROM_OTHER_SIDE"))
                        }
                        None => trf!("GRAPH_STEP_FOR", chain = text),
                    },
                    at: self.stand(first),
                    quests: vec![],
                    tags: vec![],
                    keys: vec![],
                    gate: crate::goals::Gate::Open,
                    named: true,
                    reveals: Default::default(),
                    first: None,
                },
            };
            out.push(DoorStep { at: n.at, label: label(n), goal, chain: text, opens_from: n.opens_from });
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
    pub answer: Vec<AnswerStep>,
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
/// Levers of one elevator less than this apart in height (cm) stand at one stop.
const LIFT_FLOOR: f32 = 300.0;

/// How near a door the hero stands for `door_here` (cm): across, and up or down.
const DOOR_HERE: f32 = 400.0;
const DOOR_HERE_DZ: f32 = 300.0;

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
    /// `exit`: where the hero leaves `world` for another region (the APC's door, else a save
    /// point): a chain whose first thing is in another region sends the hero there.
    pub fn gate(
        &self,
        goals: &mut Vec<crate::goals::Goal>,
        world: &str,
        s: &State,
        exit: Option<[f32; 3]>,
    ) -> Vec<(u64, Vec<String>)> {
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
            // In another region: its place is in that region's coordinates, not this one's —
            // routed here, it was pulled to the deepest floor (a crypt) as the nearest to a point
            // a kilometre down (seen in play). The way there is this region's exit.
            if self.nodes[first].world != world {
                let Some(exit) = exit else { continue };
                let step = self.away(goals, first, &of, &text, exit);
                if let Some(g) = goals.iter_mut().find(|g| g.id == of.id) {
                    g.first = Some(step);
                }
                continue;
            }
            let step = self.step(goals, first, &of, &text);
            if let Some(g) = goals.iter_mut().find(|g| g.id == of.id) {
                g.first = Some(step);
            }
        }
        chains
    }
}

impl Graph {
    /// The first thing of a chain, in another region, made a goal at this region's exit: "To
    /// {region}: {it}".
    fn away(
        &self,
        goals: &mut Vec<crate::goals::Goal>,
        first: usize,
        of: &crate::goals::Goal,
        text: &str,
        exit: [f32; 3],
    ) -> u64 {
        use crate::goals::{Gate, Goal};
        let n = &self.nodes[first];
        let id = step_id(n) ^ 0x5a5a_0000;
        if goals.iter().any(|g| g.id == id) {
            return id;
        }
        let region = crate::i18n::place(crate::survey::Survey::world_of(&n.world));
        goals.push(Goal {
            tier: of.tier,
            id,
            label: trf!("GRAPH_GO_TO_REGION", region = region, what = label(n)),
            detail: trf!("GRAPH_STEP_FOR", chain = text),
            at: exit,
            quests: of.quests.clone(),
            tags: of.tags.clone(),
            keys: of.keys.clone(),
            gate: Gate::Open,
            named: true,
            reveals: of.reveals,
            first: None,
        });
        id
    }

    /// The first thing of a chain made a goal of `of`'s quest (or a goal already there
    /// taking its quest on).
    fn step(&self, goals: &mut Vec<crate::goals::Goal>, first: usize, of: &crate::goals::Goal, text: &str) -> u64 {
        use crate::goals::{Gate, Goal};
        let n = &self.nodes[first];
        let stand = self.stand(first);
        // A goal already there, unless it is held back itself (a quest's payload beside the
        // puzzle that gives it: then the step would point at what waits on it).
        match goals.iter_mut().find(|g| {
            g.id != of.id && g.gate != Gate::Conditional && (g.at[0] - n.at[0]).hypot(g.at[1] - n.at[1]) <= NEAR
        }) {
            Some(g) => {
                // Where the hero works it: a one-sided door's side, a puzzle's devices.
                if g.at != stand {
                    if n.opens_from.is_some() {
                        g.detail = format!("{} · {}", g.detail, tr!("GRAPH_OPENS_FROM_OTHER_SIDE"));
                    }
                    g.at = stand;
                }
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
                g.id
            }
            None => {
                let id = step_id(n);
                if let Some(g) = goals.iter().find(|g| g.id == id) {
                    return g.id;
                }
                goals.push(Goal {
                    tier: of.tier,
                    id: step_id(n),
                    label: label(n),
                    detail: match n.opens_from {
                        Some(_) => {
                            format!("{} · {}", trf!("GRAPH_STEP_FOR", chain = text), tr!("GRAPH_OPENS_FROM_OTHER_SIDE"))
                        }
                        None => trf!("GRAPH_STEP_FOR", chain = text),
                    },
                    at: stand,
                    quests: of.quests.clone(),
                    tags: of.tags.clone(),
                    keys: of.keys.clone(),
                    gate: Gate::Open,
                    named: true,
                    // An order or position puzzle's device gives its answer away; else what the
                    // goal it is for gives away.
                    reveals: if self.puzzle_kind(first).is_some() { crate::goals::Reveal::Answers } else { of.reveals },
                    first: None,
                });
                id
            }
        }
    }

    /// Where the hero stands to work node `i`: a one-sided door's open side; a receiver
    /// worked through its devices (levers, statues, slots), among them on their floor —
    /// not the receiver itself, which can hang in the air above the room (the Forge
    /// Foyer's, 4 m over its six levers: its ring was off the top of the screen); else
    /// where it is.
    pub fn stand(&self, i: usize) -> [f32; 3] {
        let n = &self.nodes[i];
        if let Some(from) = n.opens_from {
            return from;
        }
        let parts: Vec<[f32; 3]> =
            n.activators.iter().filter_map(|a| self.node(&n.world, a)).map(|j| self.nodes[j].at).collect();
        if parts.is_empty() {
            return n.at;
        }
        let k = parts.len() as f32;
        let mid = parts.iter().fold([0.0; 3], |s, p| [s[0] + p[0] / k, s[1] + p[1] / k, s[2] + p[2] / k]);
        // Too far apart to be one room (devices across a region): the nearest to the middle.
        let spread = parts.iter().map(|p| (p[0] - mid[0]).hypot(p[1] - mid[1])).fold(0.0, f32::max);
        if spread > STAND_SPREAD {
            return *parts
                .iter()
                .min_by(|a, b| (a[0] - mid[0]).hypot(a[1] - mid[1]).total_cmp(&(b[0] - mid[0]).hypot(b[1] - mid[1])))
                .unwrap();
        }
        mid
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
                let step = self.step(goals, *chain.last().unwrap(), &of, &text);
                goals[i].first = Some(step);
            }
        }
        under.len()
    }
}

/// How near a goal and a node must be to be the same place (cm).
const NEAR: f32 = 150.0;
/// How near a slot must sit to a pillar to be on it (cm).
const PILLAR_NEAR: f32 = 50.0;
/// A receiver's devices further apart than this from their middle (cm) are not one room.
const STAND_SPREAD: f32 = 2_500.0;

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

    #[test]
    fn a_puzzle_device_shows_as_the_game_shows_it() {
        let c = [0.0, 0.0, 0.0];
        assert_eq!(
            face("X", "MedialSconce_LeverDualPosition_ActivatorInteract_BP_C", c, c, Some(1)),
            Some(Face::Sconce(true))
        );
        assert_eq!(
            face("X", "MedialSconce_LeverDualPosition_ActivatorInteract_BP_C", c, c, Some(0)),
            Some(Face::Sconce(false))
        );
        // the Hermit's plinths by their side of the platform (Unreal: +x east, −y north)
        let hermit = "AcasaMarshesHermitSecretLogicGateReceiver_BP_C";
        assert_eq!(face(hermit, "P", c, [500.0, 0.0, 0.0], Some(2)), Some(Face::Hermit(Glyph::Arrows)));
        assert_eq!(face(hermit, "P", c, [0.0, -500.0, 0.0], Some(2)), Some(Face::Hermit(Glyph::Bow)));
        assert_eq!(face(hermit, "P", c, [-500.0, 0.0, 0.0], Some(2)), Some(Face::Hermit(Glyph::Ring)));
        assert_eq!(face(hermit, "P", c, [0.0, 500.0, 0.0], Some(2)), Some(Face::Hermit(Glyph::Claws)));
        assert_eq!(face("JeljinMausoleum01", "Statue", c, c, Some(3)), None, "unknown: the number stays");
    }
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
            first: None,
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
    fn a_one_sided_door_is_guided_to_from_the_side_it_opens() {
        let mut g = Graph::default();
        let door = json!({"name": "Door", "class": "WroughtIronDoor_OneSidedLock_Interact_BP_C", "at": [0, 0, 0],
            "guid": "d1", "opens_from": [-150, 0, 100]});
        g.add("W", &door, &Value::Null);
        g.index();
        let none = HashSet::new();
        let s = State { used: &none, known: &none, held: &none };
        let steps = g.door_steps(&[], "W", &s);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].at, [0.0, 0.0, 0.0], "the barrier where it stands");
        assert_eq!(steps[0].goal.at, [-150.0, 0.0, 100.0], "its step on the side it opens from");
        // Opened: no longer in the way, and a way through it for the navmesh.
        let used: HashSet<String> = ["d1".to_string()].into();
        let s = State { used: &used, known: &none, held: &none };
        assert!(g.door_steps(&[], "W", &s).is_empty());
        assert_eq!(g.passable("W", &s), [([0.0, 0.0, 0.0], None)]);
        let s = State { used: &none, known: &none, held: &none };
        assert_eq!(g.passable("W", &s), [([0.0, 0.0, 0.0], Some([-150.0, 0.0, 100.0]))], "shut: one way");
    }

    #[test]
    fn a_receiver_is_worked_among_its_devices_on_their_floor() {
        let mut g = Graph::default();
        for a in [
            json!({"name": "L1", "class": "Lever", "at": [0, 0, 0], "guid": "1"}),
            json!({"name": "L2", "class": "Lever", "at": [400, 0, 0], "guid": "2"}),
            json!({"name": "Foyer", "class": "ForgeFoyerMultiActivatorsReceiver_BP_C", "at": [200, 0, 400],
                "guid": "f", "activators": ["L1", "L2"], "logic": {"solution": [1, 0]}}),
        ] {
            g.add("W", &a, &Value::Null);
        }
        g.index();
        let foyer = g.node("W", "Foyer").unwrap();
        assert_eq!(g.stand(foyer), [200.0, 0.0, 0.0], "between the levers, on their floor, not 4 m up");
    }

    #[test]
    fn the_story_order_comes_from_what_is_said_and_where_one_can_travel() {
        // Region A: a note gives "Clue"; a person, once "Clue" is known, opens the way to B.
        // Region B: a lever, reachable only once B can be travelled to.
        let dir = std::env::temp_dir().join(format!("hiumod-graph-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let w = |name: &str, v: Value| std::fs::write(dir.join(name), v.to_string()).unwrap();
        w(
            "A.json",
            json!({"world": "A", "actors": [
                {"name": "Note", "class": "Note_Gather", "at": [0, 0, 0], "guid": "n", "payload": {"tags": ["Clue"]}},
                {"name": "Person", "class": "Convo_Person_BP_C", "at": [500, 0, 0], "flow": "/Game/P/Person_ConvoRoot_FA"}
            ]}),
        );
        w(
            "B.json",
            json!({"world": "B", "actors": [
                {"name": "Lever", "class": "Lever_Activator", "at": [0, 0, 0], "guid": "l", "payload": {"tags": ["Pulled"]}}
            ]}),
        );
        w(
            "flows.json",
            json!({
                "/Game/P/Person_ConvoRoot_FA": {"payloads": [], "subgraphs": [], "gated": [],
                    "gated_subgraphs": [{"asset": "/Game/P/Person_Topic_FA", "need": {"fact": "Clue", "has": true}}]},
                "/Game/P/Person_Topic_FA": {"payloads": [], "subgraphs": [], "gated_subgraphs": [],
                    "gated": [{"payload": {"tags": ["WMA_B_Travel_BifrostTransitionFact_DA"]}, "need": null}]}
            }),
        );
        let g = Graph::load(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let r = g.reach();
        let round = |name: &str| r.depth[g.nodes.iter().position(|n| n.name.starts_with(name)).unwrap()];
        assert_eq!(round("Note"), Some(0));
        assert_eq!(round("Person#say"), Some(1), "said once the clue is known");
        assert_eq!(round("Lever"), Some(2), "in B, once B can be travelled to");
        assert!(r.stuck.is_empty());
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
