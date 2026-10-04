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

/// A choice puzzle's slot read live (slots.rs): its card tells it, set by set; listed
/// alone here it showed what the slot accepts (every orb, a stand-in) as its answer.
fn is_choice_slot(class: &str) -> bool {
    class.contains("PuzzleCheck")
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

/// This region's puzzles in the survey, with whether each is solved, but for those
/// `near` already lists (the same kind within 3 m: the two lists name puzzles apart).
fn catalogue_here(
    snap: Option<&Snapshot>,
    placements: bool,
    near: &[(crate::puzzles::Kind, [f32; 3])],
) -> Vec<(crate::survey::Placed, bool)> {
    let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
    let listed = |p: &crate::survey::Placed| {
        near.iter().any(|(k, at)| *k == p.kind && (at[0] - p.at[0]).hypot(at[1] - p.at[1]) < 300.0)
    };
    snap.map(|s| s.catalogue.clone())
        .unwrap_or_default()
        .iter()
        .filter(|(p, _)| Some(&p.world) == here.as_ref())
        .filter(|(p, _)| placements || p.kind != crate::puzzles::Kind::Placement)
        .filter(|(p, _)| p.choice.is_none())
        .filter(|(p, _)| !listed(p))
        .cloned()
        .collect()
}

/// The symbols a vault door's dials want (from 1), when the puzzle is a vault's.
fn vault_dials(class: &str, a: &Answer) -> Option<Vec<u8>> {
    match a {
        Answer::Dials(d) if class.starts_with("VOFK_") => Some(d.iter().map(|x| x.want + 1).collect()),
        _ => None,
    }
}

/// How tall the achievements list grows before it scrolls (px).
const ACHIEVEMENTS_TALL: f32 = 420.0;

impl Panel {
    /// The shard budget for the upgrade achievements (`budget_block`), a card of its own:
    /// it plans which upgrades to make and when to close the timeloops, and buried above
    /// forty achievements it went unseen.
    pub(super) fn budget_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let list = self.achievements_list();
        card(t, tr!("SHARD_BUDGET"), |t| {
            if !self.budget_block(t, &list, snap) {
                note(t, tr!("NO_UPGRADE_ACHIEVEMENTS_LEFT"));
            }
        });
    }

    /// Steam's achievements in the game's language: how many, then each achievement left: its name with progress on the right and
    /// its condition under it; a hidden one's only once unlocked or asked for.
    pub(super) fn achievements_card(&mut self, t: &mut Tui) {
        let list = self.achievements_list();
        let done = list.iter().filter(|a| a.unlocked).count();
        card(t, &trf!("ACHIEVEMENTS", done = done, all = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("STEAMS_ACHIEVEMENT_CACHE_WAS_NOT_FOUND"));
                return;
            }
            // The order as the places list has it, on its own line; the switch under it.
            // Side by side in a card's width they did not fit, and were squeezed.
            tw::order(t, &mut self.achievements_grouped, tr!("STEAM_ORDER"), tr!("BY_KIND"));
            tw::switch(t, &mut self.show_unlocked, tr!("SHOW_UNLOCKED_ONES_TOO"));
            let mut shown: Vec<_> = list.iter().filter(|a| self.show_unlocked || !a.unlocked).collect();
            if self.achievements_grouped {
                // Stable: within a kind, Steam's own order.
                shown.sort_by_key(|a| achievement_kind(&a.api));
            }
            // Forty achievements were a card taller than the window: they scroll inside it.
            tw::scroll_list(t, "achievements", ACHIEVEMENTS_TALL, |t| {
                let mut kind = None;
                for a in shown {
                    if self.achievements_grouped && kind != Some(achievement_kind(&a.api)) {
                        let k = achievement_kind(&a.api);
                        let count = list
                            .iter()
                            .filter(|x| (self.show_unlocked || !x.unlocked) && achievement_kind(&x.api) == k)
                            .count();
                        w(t, |ui| tw::group_heading(ui, ACHIEVEMENT_KINDS[k](), count));
                        kind = Some(k);
                    }
                    let id = id_of(&a.api);
                    let secret = a.hidden && !a.unlocked && !self.revealed.contains(&id);
                    let progress =
                        a.progress.filter(|(_, of)| *of > 1).map(|(v, of)| (v.clamp(0, of) as usize, of as usize));
                    // The name, then its progress as a meter with the count (a done one: a
                    // chip instead); under it the condition, which is what the player needs
                    // to read (a hidden one's only once shown).
                    tw::item(t, |t| {
                        t.style(tw::row(INLINE)).add(|t| {
                            let head = if secret { tr!("HIDDEN_ACHIEVEMENT").to_string() } else { a.name.clone() };
                            let colour = if a.unlocked { DIM } else { super::super::theme::TEXT };
                            text(t, RichText::new(head).color(colour));
                            if let Some((done, all)) = progress.filter(|_| !secret && !a.unlocked) {
                                w(t, |ui| tw::meter(ui, Some(56.0), done, all));
                                let count = RichText::new(format!("{done}/{all}")).monospace().size(11.5).color(DIM);
                                w(t, |ui| ui.label(count));
                            }
                            if a.unlocked {
                                tw::chip(t, "✓", tw::Tone::Ok);
                            }
                            if secret && w(t, |ui| ui.small_button(tr!("SHOW"))).clicked() {
                                self.revealed.insert(id);
                            }
                        });
                        if !secret {
                            note(t, a.desc.clone());
                        }
                    });
                }
            });
        });
    }

    /// Steam's achievements, read again every 10 s.
    pub(super) fn achievements_list(&mut self) -> Vec<crate::game::achievements::Achievement> {
        if self.achievements.as_ref().is_none_or(|(at, _)| at.elapsed() >= std::time::Duration::from_secs(10)) {
            self.achievements =
                Some((std::time::Instant::now(), crate::game::achievements::load(&crate::i18n::culture())));
        }
        self.achievements.as_ref().map(|(_, l)| l.clone()).unwrap_or_default()
    }

    /// The shard budget (.spec/JOURNEY.md §3.6), kept short: the shards held as a size ×
    /// feeling table, then one line per upgrade achievement not yet earned with the
    /// upgrades it still takes and a chip saying whether the shards held cover it. The
    /// why (timeloops, shard sizes, the cost in full) is in hover text. A hidden
    /// achievement's plan waits until it is shown, as its text does.
    /// Whether there was anything to plan.
    fn budget_block(
        &mut self,
        t: &mut Tui,
        list: &[crate::game::achievements::Achievement],
        snap: Option<&Snapshot>,
    ) -> bool {
        use super::super::theme::TEXT;
        let Some(budget) = snap.map(|s| s.budget.clone()).filter(|b| !b.plans.is_empty()) else { return false };
        let plans: Vec<_> = budget
            .plans
            .iter()
            .filter_map(|p| Some((p, list.iter().find(|a| a.api == p.api)?)))
            .filter(|(_, a)| !a.unlocked && (!a.hidden || self.revealed.contains(&id_of(&a.api))))
            .collect();
        if plans.is_empty() {
            return false;
        }
        const FEELINGS: [&str; 5] = ["Neutral", "Ecstasy", "Grief", "Rage", "Terror"];
        let feeling = |f: &str| match f {
            "Neutral" => tr!("NEUTRAL"),
            "Ecstasy" => tr!("ECSTASY"),
            "Grief" => tr!("GRIEF"),
            "Rage" => tr!("RAGE"),
            _ => tr!("TERROR"),
        };
        let size = |tier: u8| match tier {
            1 => tr!("SHARD_SMALL"),
            2 => tr!("SHARD_MEDIUM"),
            _ => tr!("SHARD_LARGE"),
        };
        let shards = |list: &crate::budget::Shards| {
            list.iter()
                .map(|((f, tier), n)| format!("{} {} {n}", size(*tier), feeling(f)))
                .collect::<Vec<_>>()
                .join(" · ")
        };

        // The heading: the title, and the timeloops still open on the right.
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let closed = journal
            .iter()
            .filter(|q| q.kind == crate::quests::Kind::Timeloop && q.status == crate::quests::Status::Completed)
            .count();
        let all = snap.map_or(0, |s| s.secret_totals[2]);
        t.style(tw::row(INLINE)).add(|t| {
            block(t, |ui| {
                ui.label(RichText::new(tr!("SHARDS_HELD")).small().color(DIM)).on_hover_text(tr!("SHARD_SIZES_HOVER"))
            });
            if all > 0 {
                let open = all.saturating_sub(closed);
                let tone = if open > 0 { tw::Tone::Quiet } else { tw::Tone::Wait };
                let label = trf!("TIMELOOPS_OPEN", open = open, all = all);
                w(t, |ui| tw::pill(ui, label, tone).on_hover_text(tr!("TIMELOOPS_OPEN_HOVER")));
            }
        });

        // Shards held: sizes down, feelings across.
        block(t, |ui| {
            egui::Grid::new("shards-held").spacing(egui::vec2(super::super::theme::BLOCK, 2.0)).show(ui, |ui| {
                ui.label("");
                for f in FEELINGS {
                    ui.label(RichText::new(feeling(f)).color(DIM).small());
                }
                ui.end_row();
                for tier in 1..=3u8 {
                    ui.label(RichText::new(size(tier)).color(DIM).small());
                    for f in FEELINGS {
                        let n = budget.held.get(&(f.to_string(), tier)).copied().unwrap_or(0);
                        ui.label(RichText::new(n.to_string()).small().color(if n == 0 { DIM } else { TEXT }));
                    }
                    ui.end_row();
                }
            });
        });

        // One line per achievement: its name and a chip; under it, the upgrades.
        for (p, a) in plans {
            tw::item(t, |t| {
                t.style(tw::row(INLINE)).add(|t| {
                    let name = block(t, |ui| ui.label(RichText::new(&a.name)));
                    if !p.cost.is_empty() {
                        name.on_hover_text(trf!("COST_HOVER", shards = shards(&p.cost)));
                    }
                    if p.short.is_empty() {
                        tw::chip(t, tr!("ENOUGH"), tw::Tone::Ok);
                    } else {
                        let short: Vec<String> =
                            p.short.iter().map(|(tier, n)| format!("{} {n}", size(*tier))).collect();
                        tw::chip(t, trf!("SHORT_BY", shards = short.join(" · ")), tw::Tone::Wait);
                    }
                });
                let steps: Vec<String> = p
                    .steps
                    .iter()
                    .map(|s| {
                        format!("{} {}→{}", crate::i18n::item(&s.item).unwrap_or_else(|| s.item.clone()), s.grade, s.to)
                    })
                    .collect();
                if !steps.is_empty() {
                    note(t, steps.join(" · "));
                }
                let missing: Vec<String> = p
                    .missing
                    .iter()
                    .map(|m| match m.as_str() {
                        "Weapon" => tr!("A_WEAPON").to_string(),
                        "DefensiveGear" => tr!("A_DEFENSIVE_GEAR").to_string(),
                        kind => crate::i18n::item(&format!("{kind}_Neutral_Grade_01_DA")).unwrap_or(kind.to_string()),
                    })
                    .collect();
                if !missing.is_empty() {
                    text(t, RichText::new(trf!("NOT_HELD_YET", items = missing.join(" · "))).small().color(WAIT));
                }
            });
        }
        true
    }

    /// Every puzzle of the worlds (the survey): this region's left first — dials and
    /// codes, and on request the keys and item placements — with the answer behind a
    /// button and a guide to it; the other regions as counts.
    /// The rest of this region's puzzles from the survey (not those `near` lists, by kind
    /// and place), unsolved first then nearest, and the other regions' counts.
    fn catalogue_rows(
        &mut self,
        t: &mut Tui,
        state: &mut crate::minimap::MapState,
        snap: Option<&Snapshot>,
        near: &[(crate::puzzles::Kind, [f32; 3])],
    ) {
        let list = snap.map(|s| s.catalogue.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let shown =
            |p: &crate::survey::Placed, placements: bool| placements || p.kind != crate::puzzles::Kind::Placement;
        let mine = catalogue_here(snap, self.show_placements, near);
        {
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
                // The kind, the answer's shape and how far: the actor's class name told
                // a player nothing and made each line wrap twice.
                let head = format!(
                    "{} · {} ({dist}){}",
                    crate::i18n::tr(p.kind.label()),
                    shape(&p.answer),
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
            let counts = elsewhere.iter().map(|(w, n)| (crate::i18n::place(w), *n)).collect();
            tw::regions(t, tr!("OTHER_REGIONS_CARD"), counts);
        }
    }

    /// This region's puzzles in one card: those within 40 m first (the answer the game
    /// holds, read live), then the rest of the region from the survey. Press one to be
    /// guided to it; the answer is behind its button.
    pub(super) fn puzzles_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list: Vec<Puzzle> =
            snap.map(|s| s.puzzles.iter().filter(|p| !is_choice_slot(&p.class)).cloned().collect()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let world = snap
            .and_then(|s| s.world.clone())
            .map(|w| crate::survey::Survey::world_of(&w).to_string())
            .unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let near: Vec<(crate::puzzles::Kind, [f32; 3])> = list.iter().map(|p| (p.kind, p.at)).collect();
        let left = list.iter().filter(|p| !p.solved).count()
            + catalogue_here(snap, self.show_placements, &near).iter().filter(|(_, solved)| !solved).count();
        card(t, &trf!("PUZZLES_HERE", left = left), |t| {
            block(t, |ui| tw::group_heading(ui, tr!("NEARBY"), list.len()));
            if list.is_empty() {
                note(t, tr!("NO_DIAL_KEYPAD_OR_ITEM_PLACEMENT"));
            } else {
                note(t, tr!("READS_THE_ANSWER_THE_GAME_HOLDS"));
            }
            for p in list.iter() {
                let far = here.map_or(String::new(), |h| crate::raster::span(h, p.at));
                let head = format!(
                    "{} · {} ({far}){}",
                    crate::i18n::tr(p.kind.label()),
                    shape(&p.answer),
                    if p.solved { " ✓" } else { "" }
                );
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
            let rest = catalogue_here(snap, self.show_placements, &near).len();
            block(t, |ui| tw::group_heading(ui, tr!("REST_OF_THIS_REGION"), rest));
            self.catalogue_rows(t, state, snap, &near);
        });
    }

    /// The choice puzzles (slots.rs): items into one of several slots, the Watcher's Nest's
    /// ceramic flowers and the Eye of God's orbs. Each with its sets and how they stand,
    /// the game's riddle on asking; per set, on asking, the game's clue to it (a research
    /// file), then the right slot. A line guides to its set, or once told, to the slot.
    pub(super) fn slot_puzzles_card(
        &mut self,
        t: &mut Tui,
        state: &mut crate::minimap::MapState,
        snap: Option<&Snapshot>,
    ) {
        use crate::slots::State;
        let list = snap.map(|s| s.slot_puzzles.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let yaw = snap.and_then(|s| s.pose).map(|(_, y)| y as f32);
        let world_here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        card(t, tr!("SLOT_PUZZLES"), |t| {
            if list.is_empty() {
                note(t, tr!("NO_PUZZLE_LIST_RUN_DOCTOR_SURVEY"));
                return;
            }
            note(t, tr!("SLOT_REMOVABLE"));
            // This region's first.
            let mut order: Vec<&crate::slots::SlotPuzzle> = list.iter().collect();
            order.sort_by_key(|p| Some(&p.world) != world_here.as_ref());
            for p in order {
                let what: Vec<String> = p.items.iter().map(|i| crate::goals::item_label(i)).collect();
                let (done, all) = p.progress();
                let place = crate::i18n::place(&p.world);
                let head = trf!("SLOT_PUZZLE_HEAD", what = what.join(", "), place = place, done = done, all = all);
                block(t, |ui| ui.label(RichText::new(head).strong()));
                let clue = p.clue();
                if done == all && all > 0 {
                    let after =
                        clue.and_then(|c| c.after).map(|k| format!(" {}", crate::i18n::text(k))).unwrap_or_default();
                    text(t, RichText::new(format!("{}{after}", tr!("SLOT_ALL_DONE"))).color(OK));
                }
                if let Some(c) = clue {
                    let id = crate::slots::riddle_id(p);
                    reveal(t, &mut self.revealed, id, tr!("SLOT_RIDDLE"));
                    if self.revealed.contains(&id) {
                        text(t, RichText::new(crate::i18n::game_text(c.title)).strong());
                        text(t, RichText::new(crate::slots::plain(&crate::i18n::game_text(c.text))).small().color(DIM));
                    }
                }
                for (i, s) in p.sets.iter().enumerate() {
                    let told = self.slot_hints.get(&s.id).copied().unwrap_or(0);
                    let file = clue.and_then(|c| c.for_set(s));
                    let label = file
                        .map(|(title, _)| crate::i18n::game_text(title))
                        .unwrap_or_else(|| trf!("SLOT_SET", n = i + 1));
                    let far = here.map_or(String::new(), |h| format!(" ({})", crate::raster::span(h, s.at())));
                    // A set no slot of which is right says so only once its answer is asked
                    // for: before, it reads like the others, or it would give itself away.
                    let st = s.state();
                    let how = match st {
                        State::Done => tr!("SLOT_DONE"),
                        State::Wrong => tr!("SLOT_WRONG"),
                        State::Open => tr!("SLOT_OPEN"),
                        State::Unseen => tr!("SLOT_UNSEEN"),
                        State::Nothing if told >= 2 => tr!("SLOT_NOTHING"),
                        State::Nothing => tr!("SLOT_UNSEEN"),
                    };
                    let count = trf!("SLOT_SLOTS", n = s.slots.len());
                    let tick = if st == State::Done { " ✓" } else { "" };
                    let head = format!("{label} · {count}{far} · {how}{tick}");
                    let colour = match st {
                        State::Done => DIM,
                        State::Wrong => super::super::theme::WAIT,
                        _ => super::super::theme::TEXT,
                    };
                    let id = s.id | 1 << 63;
                    let on = (st != State::Done).then(|| state.target == Some(id));
                    let icon = |ui: &mut egui::Ui| {
                        crate::ui::svg::sort(ui, Sub::Puzzle, 18.0);
                    };
                    let hints = &mut self.slot_hints;
                    let pressed = tw::line(t, on, icon, RichText::new(head).color(colour), |t| {
                        if told == 0 && file.is_some() && w(t, |ui| ui.small_button(tr!("SLOT_CLUE"))).clicked() {
                            hints.insert(s.id, 1);
                        }
                        if told < 2 && w(t, |ui| ui.small_button(tr!("SHOW_ANSWER"))).clicked() {
                            hints.insert(s.id, 2);
                        }
                        if told > 0 && w(t, |ui| ui.small_button(tr!("HIDE"))).clicked() {
                            hints.remove(&s.id);
                        }
                    });
                    let told = self.slot_hints.get(&s.id).copied().unwrap_or(0);
                    if told >= 1 {
                        if let Some((_, content)) = file {
                            let body = crate::slots::plain(&crate::i18n::game_text(content));
                            text(t, RichText::new(body).small().color(DIM));
                        }
                    }
                    let right = s.answer();
                    if told >= 2 {
                        let line = match right {
                            Some(r) => {
                                let item = match &r.choice {
                                    crate::survey::Choice::Right(item) => crate::goals::item_label(item),
                                    crate::survey::Choice::Decoy => String::new(),
                                };
                                let far = here.map_or(String::new(), |h| crate::raster::span(h, r.at));
                                let mut line = trf!("SLOT_RIGHT", item = item, far = far);
                                // Which of them, as the hero faces now; the game shows it too,
                                // ringed, once the line is chosen and the hero is near.
                                if let Some((n, all)) = yaw.and_then(|y| s.from_left(y)).filter(|(_, all)| *all > 1) {
                                    line += &format!(" · {}", trf!("SLOT_FROM_LEFT", n = n, all = all));
                                }
                                line
                            }
                            None => tr!("SLOT_NOTHING").to_string(),
                        };
                        text(t, RichText::new(format!("    {line}")).color(OK));
                    }
                    if pressed {
                        // To the set; once its answer is told, to the right slot itself.
                        let at = right.filter(|_| told >= 2).map_or(s.at(), |r| r.at);
                        let x = crate::survey::Need {
                            world: p.world.clone(),
                            id,
                            label: label.clone(),
                            what: String::new(),
                            at,
                            done: false,
                        };
                        guide_to(state, &goals, &x);
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
            let hint = RichText::new(trf!("VAULT_RESEARCH_HINT", n = lore)).small().color(DIM);
            block(t, |ui| {
                ui.add(egui::Label::new(hint).wrap()).on_hover_text(tr!("RESEARCH_ITEMS_PRESS_THE_RESEARCH_LINE"))
            });
            for v in list.iter() {
                let name = crate::i18n::game_text(&v.vault.name);
                let region = crate::i18n::game_text(&v.vault.region);
                let (status, tone) = match v.state {
                    VaultState::Opened => (tr!("OPENED").to_string(), tw::Tone::Ok),
                    VaultState::Known => (tr!("KNOWN").to_string(), tw::Tone::Accent),
                    VaultState::Locked => {
                        (trf!("RESEARCH_PROGRESS", n = lore, need = v.vault.entries), tw::Tone::Quiet)
                    }
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
                    tw::chip(t, status, tone);
                    if shut {
                        reveal(t, revealed, id, tr!("SHOW_CODE"));
                    }
                };
                if tw::line(t, on, icon, RichText::new(format!("{name} · {region}")).color(colour), end) {
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
        // Beaten of all, as every other count on the page reads: progress.
        card(t, &trf!("ENEMY_GROUPS_BEATEN", done = all - left, all = all), |t| {
            if list.is_empty() {
                note(t, tr!("NO_SPAWNER_TABLE_RUN_DOCTOR_SURVEY"));
                return;
            }
            note(t, tr!("COUNTED_BY_ENEMY_GROUP_SPAWNER_A"));
            for h in list.iter() {
                let mine = here_world.as_deref() == Some(h.world.as_str());
                let line = trf!(
                    "GROUPS_BEATEN",
                    place = crate::i18n::place(&h.world),
                    done = h.all - h.left,
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
                    let beaten = h.all - h.left;
                    let end = |t: &mut Tui| w(t, |ui| tw::meter(ui, Some(56.0), beaten, h.all));
                    if tw::line(t, on, icon, RichText::new(line).color(colour), end) {
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
                        note(t, trf!("TIMELOOP_LEFT", name = timeloop_name(&h.world, lp), left = l, all = a));
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
                    let head = RichText::new(name);
                    // Pressing a line guides there, the one guided to stays marked — the lock
                    // itself, or one of its missing rods below.
                    let icon = |ui: &mut egui::Ui| {
                        crate::ui::svg::sort(ui, crate::actors::Sub::LymbicLock, 18.0);
                    };
                    // A lock the rods held open carries a chip saying so.
                    let open_now = l.openable();
                    let end = |t: &mut Tui| {
                        if open_now {
                            tw::chip(t, tr!("OPENS_NOW"), tw::Tone::Ok);
                        }
                    };
                    if tw::line(t, Some(state.target == Some(l.id)), icon, head, end) {
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

/// The Puzzles page's hero: what is solved, as rings — this region's dials and keypads,
/// the Lymbic locks and the vaults everywhere.
pub(super) fn puzzles_hero(t: &mut Tui, snap: Option<&Snapshot>) {
    let Some(s) = snap else { return };
    let here = s.world.clone().map(|w| crate::survey::Survey::world_of(&w).to_string());
    let mine: Vec<_> = s
        .catalogue
        .iter()
        .filter(|(p, _)| Some(&p.world) == here.as_ref() && p.kind != crate::puzzles::Kind::Placement)
        .collect();
    let solved = mine.iter().filter(|(_, done)| *done).count();
    let locks = s.locks.iter().filter(|l| l.solved).count();
    let vaults = s.vaults.iter().filter(|v| v.state == VaultState::Opened).count();
    tw::hero(t, |t| {
        t.style(tw::grow(tw::grid(3, super::super::theme::PAD))).add(|t| {
            for (label, d, a) in [
                (tr!("PUZZLES_HERE_RING"), solved, mine.len()),
                (tr!("LYMBIC_LOCKS"), locks, s.locks.len()),
                (tr!("VAULTS_RING"), vaults, s.vaults.len()),
            ] {
                w(t, |ui| ui.vertical_centered(|ui| tw::ring(ui, 96.0, d, a, label)));
            }
        });
    });
}

/// A timeloop's name under its region's line: the letter when the id is the region's own
/// (`AcasaMarshes_TimeLoop_A` → `A`), else the place it is named after with the letter
/// (`ArcasSpire_TimeLoop_A` → `Arcas Spire A`), so two loops of one region stay apart.
fn timeloop_name(world: &str, id: &str) -> String {
    let id = id.trim_end_matches("_BP2").trim_end_matches("_BP");
    match id.split_once("_TimeLoop_") {
        Some((place, letter)) if place == world => letter.to_string(),
        Some((place, letter)) => format!("{} {letter}", spaced(place)),
        None => id.to_string(),
    }
}

/// `ArcasSpire` → `Arcas Spire`: a space before each capital that follows a small letter.
fn spaced(name: &str) -> String {
    let mut out = String::new();
    let mut last = ' ';
    for c in name.chars() {
        if c.is_uppercase() && last.is_lowercase() {
            out.push(' ');
        }
        out.push(c);
        last = c;
    }
    out
}

/// The achievements' kinds, in the order they are listed when grouped: the story, good
/// deeds and mysteries, combat, gear, research and collections.
const ACHIEVEMENT_KINDS: [fn() -> &'static str; 5] =
    [|| tr!("ACH_STORY"), || tr!("ACH_SECRETS"), || tr!("ACH_COMBAT"), || tr!("ACH_GEAR"), || tr!("ACH_COLLECTING")];

/// An achievement's kind, by its Steam API name.
fn achievement_kind(api: &str) -> usize {
    let starts = |p: &[&str]| p.iter().any(|x| api.starts_with(x));
    if starts(&["GoodDeeds", "Mystery"]) {
        1
    } else if starts(&["Haze", "HollowWalkers", "KillAll"]) {
        2
    } else if starts(&["WeaponMaxUpgrade", "DefensiveGear", "LoadoutMaxGrade"]) {
        3
    } else if starts(&["Research", "VOFK", "DroneSkills", "LymbicSkills", "Relics", "BaseballCaps", "ItemPlacements"]) {
        4
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_timeloop_is_named_by_its_letter_or_its_place() {
        assert_eq!(super::timeloop_name("AcasaMarshes", "AcasaMarshes_TimeLoop_A_BP"), "A");
        assert_eq!(super::timeloop_name("SenedraForest", "ArcasSpire_TimeLoop_A"), "Arcas Spire A");
    }
}
