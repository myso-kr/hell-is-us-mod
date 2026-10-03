//! Phase 3 (.spec/GUIDE.md §27): the puzzles near the hero with their answers kept
//! hidden until asked for (F6), the vault notebook (F7), the enemies left (F8).

use super::super::theme::INLINE;
use super::*;
use crate::puzzles::{Answer, Puzzle};
use crate::actors::Sub;
use crate::tables::VaultState;

/// A puzzle kind's icon on the map and in the lists.
fn puzzle_sort(k: crate::puzzles::Kind) -> Sub {
    match k {
        crate::puzzles::Kind::Placement => Sub::LymbicLock,
        _ => Sub::Puzzle,
    }
}

/// A stable id for a place the guide is sent to, from a GUID (bit 63: a survey id).
fn id_of(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish() | 1 << 63
}

/// A puzzle's answer in words.
fn answer(p: &Puzzle) -> Vec<String> {
    match &p.answer {
        Answer::Dials(dials) => dials
            .iter()
            .enumerate()
            .map(|(i, d)| match d.turns() {
                0 => trf!("DIAL_RIGHT", n = i + 1),
                k => trf!("DIAL_TURN_STEPS_NOW", n = i + 1, k = k, now = d.now + 1, want = d.want + 1),
            })
            .collect(),
        Answer::Code(code) => vec![trf!("CODE_IS", code = code)],
        Answer::Items(items) => {
            vec![trf!("NEEDS", items = items.iter().map(|i| crate::goals::item_label(i)).collect::<Vec<_>>().join(" · "))]
        }
    }
}

/// How big a puzzle is: dials and their places, the code's length, the items.
fn shape(a: &Answer) -> String {
    match a {
        Answer::Dials(d) => {
            let mut places: Vec<String> = d.iter().map(|x| x.places.to_string()).collect();
            places.dedup();
            trf!("DIALS_PLACES", n = d.len(), places = places.join("/"))
        }
        Answer::Code(c) => trf!("DIGIT_CODE", n = c.chars().count()),
        Answer::Items(i) => trf!("ITEM_COUNT", n = i.len()),
    }
}

/// A listed puzzle's answer: the dials' places in order (from 1), the code, the items.
fn listed_answer(a: &Answer) -> String {
    match a {
        Answer::Dials(d) => trf!("DIAL_POSITIONS_FROM_THE_LEFT_COUNTING", list = d.iter().map(|x| (x.want + 1).to_string()).collect::<Vec<_>>().join(" · ")),
        Answer::Code(c) => trf!("CODE_IS", code = c),
        Answer::Items(i) => trf!("NEEDS", items = i.iter().map(|x| crate::goals::item_label(x)).collect::<Vec<_>>().join(" · ")),
    }
}

/// A vault code as its symbols (assets/symbols), their names on hover.
fn symbol_row(t: &mut Tui, code: &[u8]) {
    w(t, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            for &c in code {
                crate::ui::svg::symbol(ui, c, 30.0, OK);
            }
        })
    });
}

/// The symbols a vault door's dials want (from 1), when the puzzle is a vault's.
fn vault_dials(class: &str, a: &Answer) -> Option<Vec<u8>> {
    match a {
        Answer::Dials(d) if class.starts_with("VOFK_") => Some(d.iter().map(|x| x.want + 1).collect()),
        _ => None,
    }
}

impl Panel {
    /// Steam's achievements in the game's language: how many, the ones left with
    /// their progress, a hidden one's text only once unlocked or asked for.
    pub(super) fn achievements_card(&mut self, t: &mut Tui) {
        if self.achievements.as_ref().is_none_or(|(at, _)| at.elapsed() >= std::time::Duration::from_secs(10)) {
            self.achievements = Some((std::time::Instant::now(), crate::game::achievements::load(&crate::i18n::culture())));
        }
        let list = self.achievements.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
        let done = list.iter().filter(|a| a.unlocked).count();
        card(t, &trf!("ACHIEVEMENTS", done = done, all = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("STEAMS_ACHIEVEMENT_CACHE_WAS_NOT_FOUND"));
                return;
            }
            w(t, |ui| ui.checkbox(&mut self.show_unlocked, tr!("SHOW_UNLOCKED_ONES_TOO")));
            for a in list.iter().filter(|a| self.show_unlocked || !a.unlocked) {
                let id = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    a.api.hash(&mut h);
                    h.finish()
                };
                let secret = a.hidden && !a.unlocked && !self.revealed.contains(&id);
                let progress = a.progress.filter(|(_, of)| *of > 1).map(|(v, of)| format!("  {}/{of}", v.min(of))).unwrap_or_default();
                tw::item(t, |t| {
                    t.style(tw::row(INLINE)).add(|t| {
                        let head = if secret { tr!("HIDDEN_ACHIEVEMENT").to_string() } else { format!("{}{}{progress}", if a.unlocked { "✓ " } else { "" }, a.name) };
                        text(t, RichText::new(head).color(if a.unlocked { DIM } else { super::super::theme::TEXT }));
                        if secret && w(t, |ui| ui.small_button(tr!("SHOW"))).clicked() {
                            self.revealed.insert(id);
                        }
                    });
                    if !secret {
                        text(t, RichText::new(format!("    {}", a.desc)).color(DIM).small());
                    }
                });
            }
        });
    }

    /// Every puzzle of the worlds (the survey): this region's left first — dials and
    /// codes, and on request the keys and item placements — with the answer behind a
    /// button and a guide to it; the other regions as counts.
    pub(super) fn catalogue_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.catalogue.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        let shown = |p: &crate::survey::Placed, placements: bool| placements || p.kind != crate::puzzles::Kind::Placement;
        let mine: Vec<&(crate::survey::Placed, bool)> = list
            .iter()
            .filter(|(p, _)| Some(&p.world) == here_world.as_ref() && shown(p, self.show_placements))
            .collect();
        let left = mine.iter().filter(|(_, solved)| !solved).count();
        card(t, &trf!("PUZZLE_LIST_LEFT_HERE", left = left), |t| {
            if list.is_empty() {
                note(t, tr!("NO_PUZZLE_LIST_RUN_DOCTOR_SURVEY"));
                return;
            }
            w(t, |ui| ui.checkbox(&mut self.show_placements, tr!("SHOW_KEY_DOORS_AND_ITEM_PLACEMENTS")));
            let mut rows = mine.clone();
            let far = |p: &crate::survey::Placed| here.map_or(0.0, |h| (p.at[0] - h[0]).hypot(p.at[1] - h[1]));
            rows.sort_by(|a, b| a.1.cmp(&b.1).then(far(&a.0).total_cmp(&far(&b.0))));
            for (p, solved) in rows.iter().take(30) {
                let id = p.id();
                let open = self.revealed.contains(&id);
                let dist = here.map_or(String::new(), |_| crate::raster::distance(far(p) / 100.0));
                let head = format!(
                    "{} · {} · {} ({dist}){}",
                    crate::i18n::tr(p.kind.label()),
                    shape(&p.answer),
                    crate::goals::pretty(&p.class),
                    if *solved { " ✓" } else { "" }
                );
                t.style(tw::row(INLINE)).add(|t| {
                    w(t, |ui| crate::ui::svg::sort(ui, puzzle_sort(p.kind), 18.0));
                    text(t, RichText::new(head).color(if *solved { DIM } else { super::super::theme::TEXT }).small());
                    if w(t, |ui| ui.small_button(if open { tr!("HIDE") } else { tr!("SHOW_ANSWER") })).clicked() {
                        if open {
                            self.revealed.remove(&id);
                        } else {
                            self.revealed.insert(id);
                        }
                    }
                    if !*solved && w(t, |ui| ui.small_button(tr!("GUIDE"))).clicked() {
                        let x = crate::survey::Need {
                            world: p.world.clone(),
                            id,
                            label: format!("{} · {}", crate::i18n::tr(p.kind.label()), shape(&p.answer)),
                            what: String::new(),
                            at: p.at,
                            done: false,
                        };
                        guide_to(state, &goals, &x);
                    }
                });
                if open {
                    match vault_dials(&p.class, &p.answer) {
                        Some(code) => symbol_row(t, &code),
                        None => text(t, RichText::new(format!("    {}", listed_answer(&p.answer))).color(OK)),
                    }
                }
            }
            // The other regions: how many are left there.
            let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for (p, _) in list.iter().filter(|(p, s)| !s && Some(&p.world) != here_world.as_ref() && shown(p, self.show_placements)) {
                *elsewhere.entry(p.world.as_str()).or_default() += 1;
            }
            if !elsewhere.is_empty() {
                let parts: Vec<String> = elsewhere.iter().map(|(w, n)| format!("{} {n}", crate::i18n::place(w))).collect();
                note(t, trf!("OTHER_REGIONS", regions = parts.join(" · ")));
            }
        });
    }

    /// The puzzles within 40 m: kind, name, how far; the answer behind a button.
    pub(super) fn puzzles_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.puzzles.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        card(t, &trf!("PUZZLES_NEARBY", n = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("NO_DIAL_KEYPAD_OR_ITEM_PLACEMENT"));
                return;
            }
            note(t, tr!("READS_THE_ANSWER_THE_GAME_HOLDS"));
            for p in list.iter() {
                let far = here.map_or(String::new(), |h| crate::raster::distance((p.at[0] - h[0]).hypot(p.at[1] - h[1]) / 100.0));
                let name = crate::goals::pretty(&p.class);
                let head = format!("{} · {name} ({far}){}", crate::i18n::tr(p.kind.label()), if p.solved { " ✓" } else { "" });
                let open = self.revealed.contains(&p.id);
                t.style(tw::row(INLINE)).add(|t| {
                    w(t, |ui| crate::ui::svg::sort(ui, puzzle_sort(p.kind), 18.0));
                    text(t, RichText::new(head).color(if p.solved { DIM } else { super::super::theme::TEXT }));
                    if w(t, |ui| ui.small_button(if open { tr!("HIDE") } else { tr!("SHOW_ANSWER") })).clicked() {
                        if open {
                            self.revealed.remove(&p.id);
                        } else {
                            self.revealed.insert(p.id);
                        }
                    }
                });
                if open {
                    if let Some(code) = vault_dials(&p.class, &p.answer) {
                        symbol_row(t, &code);
                    }
                    for line in answer(p) {
                        text(t, RichText::new(format!("    {line}")).color(OK));
                    }
                }
            }
        });
    }

    /// The Vaults of Forbidden Knowledge: opened, known (clue, code behind a button,
    /// guide to the door) or waiting on research.
    pub(super) fn vaults_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.vaults.clone()).unwrap_or_default();
        let lore = snap.map_or(0, |s| s.lore_known);
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let opened = list.iter().filter(|v| v.state == VaultState::Opened).count();
        card(t, &trf!("VAULTS_OF_FORBIDDEN_KNOWLEDGE", done = opened, all = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("NO_VAULT_TABLE_RUN_DOCTOR_SURVEY"));
                return;
            }
            note(t, trf!("VAULT_RESEARCH_HINT", n = lore));
            note(t, tr!("RESEARCH_ITEMS_PRESS_THE_RESEARCH_LINE"));
            for v in list.iter() {
                let name = crate::i18n::game_text(&v.vault.name);
                let region = crate::i18n::game_text(&v.vault.region);
                let status = match v.state {
                    VaultState::Opened => tr!("OPENED").to_string(),
                    VaultState::Known => tr!("KNOWN").to_string(),
                    VaultState::Locked => trf!("RESEARCH_PROGRESS", n = lore, need = v.vault.entries),
                };
                let id = id_of(&v.vault.guid);
                let open = self.revealed.contains(&id);
                t.style(tw::row(INLINE)).add(|t| {
                    w(t, |ui| crate::ui::svg::sort(ui, Sub::Vault, 18.0));
                    let colour = if v.state == VaultState::Opened { DIM } else { super::super::theme::TEXT };
                    text(t, RichText::new(format!("{name} · {region} — {status}")).color(colour));
                    if v.state != VaultState::Opened && w(t, |ui| ui.small_button(if open { tr!("HIDE") } else { tr!("SHOW_CODE") })).clicked() {
                        if open {
                            self.revealed.remove(&id);
                        } else {
                            self.revealed.insert(id);
                        }
                    }
                    if let Some((world, at)) = v.door.clone().filter(|_| v.state != VaultState::Opened) {
                        if w(t, |ui| ui.small_button(tr!("GUIDE"))).clicked() {
                            let x = crate::survey::Need { world, id, label: name.clone(), what: String::new(), at, done: false };
                            guide_to(state, &goals, &x);
                        }
                    }
                });
                if open {
                    if v.state == VaultState::Known {
                        text(t, RichText::new(format!("    {}", crate::i18n::game_text(&v.vault.clue))).color(DIM).small());
                    }
                    symbol_row(t, &v.vault.code);
                    note(t, tr!("AT_THE_DOORS_DIALS_THE_PUZZLES"));
                }
            }
        });
    }

    /// The Hollows left for "every Hollow" (Legend of the Phol): per region, its
    /// timeloops, and a button to the nearest one left here.
    pub(super) fn hollows_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.hollows.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        let (left, all) = list.iter().fold((0, 0), |(l, a), h| (l + h.left, a + h.all));
        card(t, &trf!("ENEMY_GROUPS_LEFT", left = left, all = all), |t| {
            if list.is_empty() {
                note(t, tr!("NO_SPAWNER_TABLE_RUN_DOCTOR_SURVEY"));
                return;
            }
            note(t, tr!("COUNTED_BY_ENEMY_GROUP_SPAWNER_A"));
            for h in list.iter() {
                let mine = here_world.as_deref() == Some(h.world.as_str());
                let line = trf!("GROUPS_ENEMIES", place = crate::i18n::place(&h.world), left = h.left, all = h.all, enemies = h.enemies_left);
                tw::item(t, |t| {
                    t.style(tw::row(INLINE)).add(|t| {
                        w(t, |ui| crate::ui::svg::sort(ui, Sub::EnemyGroup, 18.0));
                        let colour = if h.left == 0 { DIM } else if mine { super::super::theme::TITLE } else { super::super::theme::TEXT };
                        text(t, RichText::new(line).color(colour));
                        if mine && h.left > 0 {
                            if let (Some(p), true) = (here, w(t, |ui| ui.small_button(tr!("NEAREST"))).clicked()) {
                                let near = h.places.iter().min_by(|a, b| (a[0] - p[0]).hypot(a[1] - p[1]).total_cmp(&(b[0] - p[0]).hypot(b[1] - p[1])));
                                if let Some(at) = near {
                                    let x = crate::survey::Need {
                                        world: h.world.clone(),
                                        id: id_of(&format!("hollow{at:?}")),
                                        label: tr!("ENEMY_GROUP_LEFT").to_string(),
                                        what: String::new(),
                                        at: *at,
                                        done: false,
                                    };
                                    guide_to(state, &goals, &x);
                                }
                            }
                        }
                    });
                    for (lp, l, a) in h.timeloops.iter().filter(|(_, l, _)| *l > 0) {
                        note(t, trf!("TIMELOOP_LEFT", name = lp.trim_end_matches("_BP").trim_end_matches("_BP2"), left = l, all = a));
                    }
                });
            }
        });
    }
}
