//! The tracker's context lines (.spec/JOURNEY.md §2): what the place the hero is in asks
//! of them now, above the quests, each only when it applies —
//!
//! - a fight with Hazes: how many Walkers one keeps alive, and how far the Haze is;
//! - a key item just picked up: the locks it now opens (`key_note`, kept by the overlay);
//! - close to someone: an item they want that the hero holds, or a secret they tell;
//! - at the APC: what each other region still holds for the quests under way;
//! - and always, what is left around the hero: items, puzzles, enemy groups, goals.
//!
//! They sit in the tracker's column at the top right, which the game leaves free: its
//! own HUD is at the top left (the hero's status), the left and right middle
//! (notifications), the centre (prompts) and along the bottom (wheels, subtitles).

use crate::actors::{Kind, Sub, Thing};
use crate::engine::Snapshot;
use crate::goals::Goal;
use crate::raster::Rgba;

/// One line: its text and the colour of its mark.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub colour: Rgba,
}

/// What "around the hero" is for the ledger (m).
const LEDGER_M: f32 = 120.0;
/// How close someone with an item to take or a secret to tell is said (m).
const NEAR_M: f32 = 25.0;
/// How close to the APC's door its plan shows (m).
const APC_M: f32 = 15.0;
/// How close a Walker a Haze keeps alive is a fight (m).
const FIGHT_M: f32 = 30.0;

const HAZE: Rgba = Rgba(190, 140, 255, 255);
const KEY: Rgba = Rgba(0x5A, 0x9C, 0xE6, 255);
const PERSON: Rgba = Rgba(62, 143, 224, 255);
const APC: Rgba = Rgba(232, 194, 58, 255);
const LEFT: Rgba = Rgba(0x9A, 0xA6, 0xB3, 255);
const CLEAR: Rgba = Rgba(122, 204, 150, 255);

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1]) / 100.0
}

/// The lines for the hero at `here` in `world`. `things` are those the maps show (with
/// the survey's, as consent allows); `places`: the player agreed to see where hidden
/// things are, which the ledger and the people near tell.
pub fn lines(
    s: &Snapshot,
    things: &[Thing],
    goals: &[Goal],
    here: [f32; 3],
    world: &str,
    places: bool,
    key_note: Option<&str>,
) -> Vec<Line> {
    let mut out = Vec::new();
    // A fight: the Walkers near that a Haze keeps alive, and the nearest Haze.
    let near_links: Vec<&crate::actors::HazeLink> =
        s.haze_links.iter().filter(|(_, walker)| flat(*walker, here) <= FIGHT_M).collect();
    if let Some(haze) = near_links.iter().map(|(h, _)| flat(*h, here)).min_by(|a, b| a.total_cmp(b)) {
        out.push(Line { text: trf!("CTX_HAZE", n = near_links.len(), m = haze.round() as u32), colour: HAZE });
    }
    if let Some(note) = key_note {
        out.push(Line { text: note.to_string(), colour: KEY });
    }
    // Standing at a shut door: what opens it (graph.rs `door_here`).
    if let Some((door, first)) = &s.door_here {
        out.push(Line { text: door_line(door, first), colour: KEY });
    }
    if places {
        // Someone near who wants an item the hero holds.
        for x in s.handovers.iter().filter(|x| !x.done && flat(x.at, here) <= NEAR_M).take(2) {
            let m = flat(x.at, here).round() as u32;
            out.push(Line { text: trf!("CTX_HANDOVER", what = x.what, who = x.label, m = m), colour: PERSON });
        }
        // Someone near who tells a secret.
        if let Some(m) = things
            .iter()
            .filter(|t| t.sub == Sub::NpcSecret)
            .map(|t| flat(t.at, here))
            .filter(|m| *m <= NEAR_M)
            .min_by(|a, b| a.total_cmp(b))
        {
            out.push(Line { text: trf!("CTX_SECRET_NEAR", m = m.round() as u32), colour: PERSON });
        }
    }
    // At the APC: the other regions with the most left for the quests under way.
    if things.iter().any(|t| t.sub == Sub::Apc && flat(t.at, here) <= APC_M) {
        let here_world = crate::survey::Survey::world_of(world);
        let mut by_world: Vec<(String, usize)> = Vec::new();
        for x in s.needs.iter().flat_map(|(_, list)| list.iter()).filter(|x| !x.done && x.world != here_world) {
            match by_world.iter_mut().find(|(w, _)| *w == x.world) {
                Some((_, n)) => *n += 1,
                None => by_world.push((x.world.clone(), 1)),
            }
        }
        by_world.sort_by_key(|w| std::cmp::Reverse(w.1));
        let list: Vec<String> =
            by_world.iter().take(3).map(|(w, n)| format!("{} {n}", crate::i18n::place(w))).collect();
        let text = if list.is_empty() {
            tr!("CTX_APC_NOTHING").to_string()
        } else {
            trf!("CTX_APC", list = list.join(" · "))
        };
        out.push(Line { text, colour: APC });
    }
    // What is left around the hero.
    if places {
        let near = |t: &&Thing| flat(t.at, here) <= LEDGER_M;
        let items = things.iter().filter(near).filter(|t| t.kind() == Kind::Item).count();
        let puzzles = things
            .iter()
            .filter(near)
            .filter(|t| matches!(t.sub, Sub::Puzzle | Sub::LymbicLock | Sub::Vault | Sub::Translation))
            .count();
        let groups = things.iter().filter(near).filter(|t| t.sub == Sub::EnemyGroup).count();
        let aims = goals.iter().filter(|g| flat(g.at, here) <= LEDGER_M).count();
        let mut parts = Vec::new();
        for (n, key) in [(items, "CTX_ITEMS"), (puzzles, "CTX_PUZZLES"), (groups, "CTX_GROUPS"), (aims, "CTX_GOALS")] {
            if n > 0 {
                parts.push(format!("{} {n}", crate::i18n::text(key)));
            }
        }
        let m = LEDGER_M as u32;
        out.push(if parts.is_empty() {
            Line { text: trf!("CTX_LEDGER_CLEAR", m = m), colour: CLEAR }
        } else {
            Line { text: trf!("CTX_LEDGER", m = m, list = parts.join(" · ")), colour: LEFT }
        });
    }
    out
}

/// "A shortcut: a 6 m drop — 10 % of health", by the game's fall damage (navmesh.rs).
pub fn drop_line(h: f32) -> Line {
    let m = format!("{:.0}", h / 100.0);
    let key = if h <= crate::navmesh::DROP_HURTS {
        "CTX_DROP_SAFE"
    } else if h <= crate::navmesh::DROP_HURTS_MORE {
        "CTX_DROP_HURTS"
    } else {
        "CTX_DROP_HURTS_MORE"
    };
    let [r, g, b] = crate::navmesh::drop_rgb(h);
    let (text, colour) = (crate::i18n::text(key).replace("{m}", &m), Rgba(r, g, b, 255));
    Line { text, colour }
}

/// "This door (…) opens after: A → B → C …", the first three steps, first thing first.
pub fn door_line(door: &str, first: &[String]) -> String {
    let mut steps: Vec<&str> = first.iter().map(String::as_str).take(3).collect();
    if first.len() > 3 {
        steps.push("…");
    }
    trf!("CTX_DOOR", door = door, steps = steps.join(" → "))
}

/// The rods held now (their data asset names), from the locks' lists: to tell when one is
/// picked up.
pub fn rods_held(s: &Snapshot) -> std::collections::BTreeSet<String> {
    s.locks.iter().flat_map(|l| l.rods.iter()).filter(|r| r.held).map(|r| r.item.clone()).collect()
}

/// What a rod just picked up opens: how many locks it completes, and how far the nearest
/// in this world is; `None` when it opens none yet.
pub fn key_note(s: &Snapshot, rod: &str, here: [f32; 3], world: &str) -> Option<String> {
    let world = crate::survey::Survey::world_of(world);
    let opens: Vec<&crate::survey::Lock> =
        s.locks.iter().filter(|l| l.openable() && l.rods.iter().any(|r| r.item == rod)).collect();
    let name = crate::i18n::item(rod).unwrap_or_else(|| rod.trim_end_matches("_Item_DA").replace('_', " "));
    if opens.is_empty() {
        return Some(trf!("CTX_ROD_NOT_YET", name = name));
    }
    let nearest = opens.iter().filter(|l| l.world == world).map(|l| flat(l.at, here)).min_by(|a, b| a.total_cmp(b));
    Some(match nearest {
        Some(m) => trf!("CTX_ROD_OPENS_HERE", name = name, n = opens.len(), m = m.round() as u32),
        None => trf!("CTX_ROD_OPENS_AWAY", name = name, n = opens.len()),
    })
}
