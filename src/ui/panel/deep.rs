//! Phase 3 (.spec/FEATURES.md §3): the puzzles near the hero with their answers kept
//! hidden until asked for (F6), the vault notebook (F7), the enemies left (F8).

use super::super::theme::INLINE;
use super::*;
use crate::actors::Sub;
use crate::puzzles::{Answer, Puzzle};
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
            vec![trf!(
                "NEEDS",
                items = items.iter().map(|i| crate::goals::item_label(i)).collect::<Vec<_>>().join(" · ")
            )]
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
        Answer::Dials(d) => trf!(
            "DIAL_POSITIONS_FROM_THE_LEFT_COUNTING",
            list = d.iter().map(|x| (x.want + 1).to_string()).collect::<Vec<_>>().join(" · ")
        ),
        Answer::Code(c) => trf!("CODE_IS", code = c),
        Answer::Items(i) => {
            trf!("NEEDS", items = i.iter().map(|x| crate::goals::item_label(x)).collect::<Vec<_>>().join(" · "))
        }
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
            self.achievements =
                Some((std::time::Instant::now(), crate::game::achievements::load(&crate::i18n::culture())));
        }
        let list = self.achievements.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
        let done = list.iter().filter(|a| a.unlocked).count();
        card(t, &trf!("ACHIEVEMENTS", done = done, all = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("STEAMS_ACHIEVEMENT_CACHE_WAS_NOT_FOUND"));
                return;
            }
            tw::switch(t, &mut self.show_unlocked, tr!("SHOW_UNLOCKED_ONES_TOO"));
            for a in list.iter().filter(|a| self.show_unlocked || !a.unlocked) {
                let id = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    a.api.hash(&mut h);
                    h.finish()
                };
                let secret = a.hidden && !a.unlocked && !self.revealed.contains(&id);
                let progress = a
                    .progress
                    .filter(|(_, of)| *of > 1)
                    .map(|(v, of)| format!("  {}/{of}", v.min(of)))
                    .unwrap_or_default();
                tw::item(t, |t| {
                    t.style(tw::row(INLINE)).add(|t| {
                        let head = if secret {
                            tr!("HIDDEN_ACHIEVEMENT").to_string()
                        } else {
                            format!("{}{}{progress}", if a.unlocked { "✓ " } else { "" }, a.name)
                        };
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
    pub(super) fn catalogue_card(
        &mut self,
        t: &mut Tui,
        state: &mut crate::minimap::MapState,
        snap: Option<&Snapshot>,
    ) {
        let list = snap.map(|s| s.catalogue.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let shown =
            |p: &crate::survey::Placed, placements: bool| placements || p.kind != crate::puzzles::Kind::Placement;
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
            tw::switch(t, &mut self.show_placements, tr!("SHOW_KEY_DOORS_AND_ITEM_PLACEMENTS"));
            let mut rows = mine.clone();
            let far = |p: &crate::survey::Placed| here.map_or(0.0, |h| (p.at[0] - h[0]).hypot(p.at[1] - h[1]));
            rows.sort_by(|a, b| a.1.cmp(&b.1).then(far(&a.0).total_cmp(&far(&b.0))));
            for (p, solved) in rows.iter().take(30) {
                let id = p.id();
                let dist = here.map_or(String::new(), |h| crate::raster::span(h, p.at));
                let head = format!(
                    "{} · {} · {} ({dist}){}",
                    crate::i18n::tr(p.kind.label()),
                    shape(&p.answer),
                    crate::goals::pretty(&p.class),
                    if *solved { " ✓" } else { "" }
                );
                let icon = |ui: &mut egui::Ui| {
                    crate::ui::svg::sort(ui, puzzle_sort(p.kind), 18.0);
                };
                let head = RichText::new(head).color(if *solved { DIM } else { super::super::theme::TEXT }).small();
                let on = (!*solved).then(|| state.target == Some(id));
                if tw::line(t, on, icon, head, |t| reveal(t, &mut self.revealed, id, tr!("SHOW_ANSWER"))) {
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
                let open = self.revealed.contains(&id);
                if open {
                    match vault_dials(&p.class, &p.answer) {
                        Some(code) => symbol_row(t, &code),
                        None => text(t, RichText::new(format!("    {}", listed_answer(&p.answer))).color(OK)),
                    }
                }
            }
            // The other regions: how many are left there.
            let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for (p, _) in list
                .iter()
                .filter(|(p, s)| !s && Some(&p.world) != here_world.as_ref() && shown(p, self.show_placements))
            {
                *elsewhere.entry(p.world.as_str()).or_default() += 1;
            }
            if !elsewhere.is_empty() {
                let parts: Vec<String> =
                    elsewhere.iter().map(|(w, n)| format!("{} {n}", crate::i18n::place(w))).collect();
                note(t, trf!("OTHER_REGIONS", regions = parts.join(" · ")));
            }
        });
    }

    /// The puzzles within 40 m: kind, name, how far; press one to be guided to it, the
    /// answer behind its button.
    pub(super) fn puzzles_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.puzzles.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let world = snap
            .and_then(|s| s.world.clone())
            .map(|w| crate::survey::Survey::world_of(&w).to_string())
            .unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        card(t, &trf!("PUZZLES_NEARBY", n = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("NO_DIAL_KEYPAD_OR_ITEM_PLACEMENT"));
                return;
            }
            note(t, tr!("READS_THE_ANSWER_THE_GAME_HOLDS"));
            for p in list.iter() {
                let far = here.map_or(String::new(), |h| crate::raster::span(h, p.at));
                let name = crate::goals::pretty(&p.class);
                let head =
                    format!("{} · {name} ({far}){}", crate::i18n::tr(p.kind.label()), if p.solved { " ✓" } else { "" });
                let icon = |ui: &mut egui::Ui| {
                    crate::ui::svg::sort(ui, puzzle_sort(p.kind), 18.0);
                };
                let head = RichText::new(head).color(if p.solved { DIM } else { super::super::theme::TEXT });
                let on = (!p.solved).then(|| state.target == Some(p.id));
                if tw::line(t, on, icon, head, |t| reveal(t, &mut self.revealed, p.id, tr!("SHOW_ANSWER"))) {
                    let x = crate::survey::Need {
                        world: world.clone(),
                        id: p.id,
                        label: crate::i18n::tr(p.kind.label()).to_string(),
                        what: String::new(),
                        at: p.at,
                        done: false,
                    };
                    guide_to(state, &goals, &x);
                }
                let open = self.revealed.contains(&p.id);
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
                let icon = |ui: &mut egui::Ui| {
                    crate::ui::svg::sort(ui, Sub::Vault, 18.0);
                };
                let shut = v.state != VaultState::Opened;
                let colour = if shut { super::super::theme::TEXT } else { DIM };
                let door = v.door.clone().filter(|_| shut);
                let on = door.as_ref().map(|_| state.target == Some(id));
                let revealed = &mut self.revealed;
                let end = |t: &mut Tui| {
                    if shut {
                        reveal(t, revealed, id, tr!("SHOW_CODE"));
                    }
                };
                if tw::line(t, on, icon, RichText::new(format!("{name} · {region} — {status}")).color(colour), end) {
                    if let Some((world, at)) = door {
                        let x = crate::survey::Need {
                            world,
                            id,
                            label: name.clone(),
                            what: String::new(),
                            at,
                            done: false,
                        };
                        guide_to(state, &goals, &x);
                    }
                }
                let open = self.revealed.contains(&id);
                if open {
                    if v.state == VaultState::Known {
                        text(
                            t,
                            RichText::new(format!("    {}", crate::i18n::game_text(&v.vault.clue))).color(DIM).small(),
                        );
                    }
                    symbol_row(t, &v.vault.code);
                    note(t, tr!("AT_THE_DOORS_DIALS_THE_PUZZLES"));
                }
            }
        });
    }

    /// The Hollows left for "every Hollow" (Legend of the Phol): per region, its
    /// timeloops; this region's line guides to the nearest one left.
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
                let line = trf!(
                    "GROUPS_ENEMIES",
                    place = crate::i18n::place(&h.world),
                    left = h.left,
                    all = h.all,
                    enemies = h.enemies_left
                );
                tw::item(t, |t| {
                    let icon = |ui: &mut egui::Ui| {
                        crate::ui::svg::sort(ui, Sub::EnemyGroup, 18.0);
                    };
                    let colour = if h.left == 0 {
                        DIM
                    } else if mine {
                        super::super::theme::TITLE
                    } else {
                        super::super::theme::TEXT
                    };
                    // Here, the line guides to the nearest group left.
                    let near = here.filter(|_| mine && h.left > 0).and_then(|p| {
                        h.places.iter().min_by(|a, b| {
                            (a[0] - p[0]).hypot(a[1] - p[1]).total_cmp(&(b[0] - p[0]).hypot(b[1] - p[1]))
                        })
                    });
                    let id = near.map(|at| id_of(&format!("hollow{at:?}")));
                    let on = id.map(|id| state.target == Some(id));
                    if tw::line(t, on, icon, RichText::new(line).color(colour), |_| {}) {
                        if let (Some(at), Some(id)) = (near, id) {
                            let x = crate::survey::Need {
                                world: h.world.clone(),
                                id,
                                label: tr!("ENEMY_GROUP_LEFT").to_string(),
                                what: String::new(),
                                at: *at,
                                done: false,
                            };
                            guide_to(state, &goals, &x);
                        }
                    }
                    for (lp, l, a) in h.timeloops.iter().filter(|(_, l, _)| *l > 0) {
                        note(
                            t,
                            trf!(
                                "TIMELOOP_LEFT",
                                name = lp.trim_end_matches("_BP").trim_end_matches("_BP2"),
                                left = l,
                                all = a
                            ),
                        );
                    }
                });
            }
        });
    }
}

impl Panel {
    /// The Lymbic locks (.spec/JOURNEY.md §3.3): for each one not opened, the rods it
    /// takes — held or not — and how far each missing one's pickup is; pressing a lock or a
    /// missing rod guides there. Locks the rods
    /// held already open come first; this region's before the others.
    pub(super) fn locks_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let locks = snap.map(|s| s.locks.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let pos = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        card(t, tr!("LYMBIC_LOCKS"), |t| {
            if locks.is_empty() {
                note(t, tr!("NO_SURVEY_DB_RUN_DOCTOR_SURVEY"));
                return;
            }
            note(t, tr!("THE_RODS_EACH_LOCK_TAKES"));
            let mut open: Vec<_> = locks.iter().filter(|l| !l.solved).collect();
            open.sort_by_key(|l| (!l.openable(), Some(l.world.as_str()) != here.as_deref(), l.world.clone()));
            let elsewhere = open.iter().filter(|l| !l.openable() && Some(l.world.as_str()) != here.as_deref()).count();
            for l in open.iter().filter(|l| l.openable() || Some(l.world.as_str()) == here.as_deref()) {
                tw::item(t, |t| {
                    // The card is this region's: a lock here goes by its distance, one
                    // elsewhere (shown only when it opens) by its region.
                    let name = match pos.filter(|_| Some(l.world.as_str()) == here.as_deref()) {
                        Some(p) => format!("{} ({})", tr!("LYMBIC_LOCK"), crate::raster::span(p, l.at)),
                        None if Some(l.world.as_str()) == here.as_deref() => tr!("LYMBIC_LOCK").to_string(),
                        None => crate::i18n::place(&l.world),
                    };
                    let head = if l.openable() { trf!("LOCK_OPENS_NOW", place = name) } else { name };
                    let head = RichText::new(head).color(if l.openable() { OK } else { super::super::theme::TEXT });
                    // Pressing a line guides there, the one guided to stays marked — the lock
                    // itself, or one of its missing rods below.
                    let icon = |ui: &mut egui::Ui| {
                        crate::ui::svg::sort(ui, crate::actors::Sub::LymbicLock, 18.0);
                    };
                    if tw::pick_with(t, state.target == Some(l.id), icon, head) {
                        let x = crate::survey::Need {
                            world: l.world.clone(),
                            id: l.id,
                            label: tr!("LYMBIC_LOCK").to_string(),
                            what: String::new(),
                            at: l.at,
                            done: false,
                        };
                        guide_to(state, &goals, &x);
                    }
                    for r in &l.rods {
                        let name = crate::i18n::item(&r.item).unwrap_or_else(|| rod_name(&r.item));
                        let source = r.source.as_ref().filter(|_| !r.held);
                        let line = match source {
                            _ if r.held => trf!("ROD_HELD", rod = name),
                            None => trf!("ROD_MISSING_NOWHERE", rod = name),
                            Some(s) if here.as_deref() != Some(s.world.as_str()) => {
                                trf!("ROD_MISSING_AT", rod = name, place = crate::i18n::place(&s.world))
                            }
                            Some(s) => match pos {
                                Some(p) => {
                                    trf!("ROD_MISSING", rod = format!("{name} ({})", crate::raster::span(p, s.at)))
                                }
                                None => trf!("ROD_MISSING", rod = name),
                            },
                        };
                        // Under the lock's name, past where its icon stands.
                        let indent = |ui: &mut egui::Ui| {
                            ui.allocate_exact_size(egui::vec2(18.0, 1.0), egui::Sense::hover());
                        };
                        let colour = if source.is_some() { super::super::theme::TEXT } else { DIM };
                        let on = source.map(|s| state.target == Some(s.id));
                        if tw::line(t, on, indent, RichText::new(line).color(colour).small(), |_| {}) {
                            if let Some(s) = source {
                                guide_to(state, &goals, s);
                            }
                        }
                    }
                });
            }
            if elsewhere > 0 {
                note(t, trf!("LOCKS_IN_OTHER_REGIONS", count = elsewhere));
            }
        });
    }
}

/// The one button a list line may carry: show what it hides (an answer, a code), or
/// hide it again.
fn reveal(t: &mut Tui, revealed: &mut std::collections::HashSet<u64>, id: u64, show: &str) {
    let open = revealed.contains(&id);
    if w(t, |ui| ui.small_button(if open { tr!("HIDE") } else { show })).clicked() {
        if open {
            revealed.remove(&id);
        } else {
            revealed.insert(id);
        }
    }
}

/// A rod's name when the game's text is not read yet: `LymbicRod_XRay_Rage_Item_DA` → "Rage X".
fn rod_name(item: &str) -> String {
    let mut parts = item.split('_').skip(1);
    let (letter, emotion) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    format!("{emotion} {}", letter.chars().next().unwrap_or('?'))
}
