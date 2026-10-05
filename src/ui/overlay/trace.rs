//! What the guide is working with, written out so a wrong guide can be read rather than
//! guessed at: `Mods\doctor\guide.jsonl`, one JSON record a line, each with its time
//! (`t`) and kind — an `event` when the auto guide's target changes (from, to, why), a
//! `state` every few seconds — so what led up to a wrong guide is there in order. Kept
//! to the last `KEEP` lines once it passes `MAX_LINES`. The Debug page shows the last
//! state with the recent events (`Shared::trace`).
//!
//! It holds the hero and the world; the auto guide's target with where it comes from
//! (the live scan or the survey), how far and how high, whether blocked, and its route;
//! the nearest goals with why each is or is not picked; what is followed; and the last
//! events: the target changing and why.

use crate::goals::Goal;
use crate::minimap::MapState;
use crate::quests::Quest;
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

/// How often a state line is written.
const EVERY: Duration = Duration::from_secs(5);
/// The file's most lines; past it, the last `KEEP` are kept.
const MAX_LINES: usize = 2000;
const KEEP: usize = 1000;
/// How many goals, nearest first, the trace lists; how many events it keeps.
const CANDIDATES: usize = 12;
const EVENTS: usize = 50;

/// A route as the trace tells it: the goal, how many points, whether some leg goes
/// through something (the way in not found), and how far its end is from the goal (m).
pub type RouteNote = (u64, usize, bool, f32);

#[derive(Default)]
pub struct Trace {
    events: VecDeque<String>,
    /// Lines in the file, once counted.
    lines: Option<usize>,
    auto: Option<u64>,
    written: Option<Instant>,
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1]) / 100.0
}

/// Where a goal comes from: the survey's ids have the top bit set.
fn source(id: u64) -> &'static str {
    if id >> 63 == 1 {
        "survey"
    } else {
        "live"
    }
}

fn goal_json(g: &Goal, here: [f32; 3]) -> Value {
    json!({
        "id": format!("{:016x}", g.id),
        "label": g.label,
        "tier": format!("{:?}", g.tier),
        "source": source(g.id),
        "at": g.at.map(|v| v.round()),
        "distance_m": (flat(g.at, here) * 10.0).round() / 10.0,
        "height_m": ((g.at[2] - here[2]) / 10.0).round() / 10.0,
        "gate": format!("{:?}", g.gate),
        "detail": g.detail,
    })
}

impl Trace {
    /// Note an event, with the time.
    pub fn note(&mut self, text: String) {
        let t = chrono_now();
        self.events.push_back(format!("{t} {text}"));
        while self.events.len() > EVENTS {
            self.events.pop_front();
        }
    }

    /// An event of the guide's own (not a target change), to the list and the file.
    pub fn note_event(&mut self, text: &str, world: &str, here: [f32; 3]) {
        self.note(text.to_string());
        self.append(&json!({
            "t": chrono_now(),
            "kind": "event",
            "what": "guide",
            "text": text,
            "world": world,
            "hero": here.map(|v| v.round()),
        }));
    }

    /// Look at the guide this frame; when the auto guide's target changed, note why;
    /// when due, the trace as JSON (also written to the file).
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        state: &MapState,
        goals: &[Goal],
        here: [f32; 3],
        world: &str,
        followed: Option<&Quest>,
        journal: &[Quest],
        blocked: &HashSet<u64>,
        routes: &[RouteNote],
    ) -> Option<String> {
        let label = |id: Option<u64>| {
            id.map_or("none".to_string(), |id| {
                goals
                    .iter()
                    .find(|g| g.id == id)
                    .map_or(format!("{id:016x}"), |g| format!("{} [{}]", g.label, source(id)))
            })
        };
        let changed = state.auto != self.auto;
        if changed {
            let why = match self.auto {
                None => "nothing before",
                Some(old) if !goals.iter().any(|g| g.id == old) => {
                    "the old one left the goals (taken, done, or found empty)"
                }
                Some(old) if blocked.contains(&old) => "the old one is blocked (its route goes through something)",
                Some(old) if state.skipped.contains(&old) => "the old one was skipped",
                Some(_) => "another goal is wanted more (nearer, or the old one no longer serves the story)",
            };
            self.note(format!("auto: {} -> {} ({why})", label(self.auto), label(state.auto)));
            self.append(&json!({
                "t": chrono_now(),
                "kind": "event",
                "what": "auto",
                "from": label(self.auto),
                "to": label(state.auto),
                "why": why,
                "world": world,
                "hero": here.map(|v| v.round()),
            }));
            self.auto = state.auto;
        }
        if !changed && self.written.is_some_and(|t| t.elapsed() < EVERY) {
            return None;
        }
        self.written = Some(Instant::now());
        let mut near: Vec<&Goal> = goals.iter().collect();
        near.sort_by(|a, b| flat(a.at, here).total_cmp(&flat(b.at, here)));
        let candidates: Vec<Value> = near
            .iter()
            .take(CANDIDATES)
            .map(|g| {
                let mut v = goal_json(g, here);
                v["wanted"] = json!(crate::guide::target::wanted(g, goals, followed, journal));
                v["blocked"] = json!(blocked.contains(&g.id));
                v["skipped"] = json!(state.skipped.contains(&g.id));
                v["followed"] = json!(state.is_followed(g.id));
                v
            })
            .collect();
        let route_of = |id: u64| {
            routes
                .iter()
                .find(|r| r.0 == id)
                .map(|r| json!({"points": r.1, "through_something": r.2, "ends_short_m": r.3}))
        };
        let auto = state.auto.and_then(|id| goals.iter().find(|g| g.id == id)).map(|g| {
            let mut v = goal_json(g, here);
            v["blocked"] = json!(blocked.contains(&g.id));
            v["route"] = route_of(g.id).unwrap_or(Value::Null);
            v["held"] = json!(state.held);
            v
        });
        let tracks: Vec<Value> = state
            .tracks
            .iter()
            .map(|t| {
                let goal = state.goal_of(t);
                json!({
                    "label": t.shown(),
                    "world": t.world,
                    "quest": t.quest,
                    "goal": goal.and_then(|id| goals.iter().find(|g| g.id == id)).map(|g| goal_json(g, here)),
                    "route": goal.and_then(route_of),
                })
            })
            .collect();
        let mut v = json!({
            "t": chrono_now(),
            "kind": "state",
            "world": world,
            "hero": here.map(|v| v.round()),
            "story": followed.map(|q| q.name.clone()),
            "auto_guide": state.guide_auto,
            "auto": auto,
            "tracks": tracks,
            "goals": goals.len(),
            "survey_goals": goals.iter().filter(|g| source(g.id) == "survey").count(),
            "blocked": blocked.len(),
            "nearest": candidates,
        });
        self.append(&v);
        // The Debug page's: the state with the recent events.
        v["events"] = json!(self.events.iter().rev().collect::<Vec<_>>());
        Some(v.to_string())
    }

    /// One line at the end of the file; past `MAX_LINES`, only the last `KEEP` kept.
    fn append(&mut self, v: &Value) {
        use std::io::Write;
        let dir = crate::paths::data_dir().join("doctor");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("guide.jsonl");
        let lines = *self.lines.get_or_insert_with(|| std::fs::read_to_string(&path).map_or(0, |t| t.lines().count()));
        if lines >= MAX_LINES {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let all: Vec<&str> = text.lines().collect();
            let kept = all[all.len().saturating_sub(KEEP)..].join("\n") + "\n";
            let _ = std::fs::write(&path, kept);
            self.lines = Some(KEEP.min(all.len()));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            if writeln!(f, "{v}").is_ok() {
                *self.lines.get_or_insert(0) += 1;
            }
        }
    }
}

/// The local time of day, `HH:MM:SS`.
fn chrono_now() -> String {
    crate::logfile::stamp().get(11..19).unwrap_or_default().replace('-', ":")
}
