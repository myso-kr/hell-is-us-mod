//! The quest page: the journal and what the followed quest needs, missable deadlines, hand-overs.

use super::super::theme::{INLINE, TIGHT, TITLE};
use super::*;

/// A deadline's detail runs to this many characters on the card; the rest is on hover.
const DETAIL: usize = 64;

/// `done/all`, small and dim, for the end of a line beside its meter.
fn count(done: usize, all: usize) -> RichText {
    RichText::new(format!("{done}/{all}")).monospace().size(11.5).color(DIM)
}

/// A distance at the end of a line, small and dim (nothing when it is unknown).
fn distance(t: &mut Tui, span: String) {
    if !span.is_empty() {
        w(t, |ui| ui.label(RichText::new(span).monospace().size(11.5).color(DIM)));
    }
}

/// `s` cut to `n` characters, an ellipsis where it was cut.
fn clip(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", s[..i].trim_end()),
        None => s.to_string(),
    }
}

impl Panel {
    /// The quest journal: which quest the guide and the tracker follow.
    pub(super) fn quests_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        use crate::quests::Status;
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        card(t, tr!("QUESTS"), |t| {
            switch(t, &mut state.tracker, tr!("QUEST_TRACKER_RIGHT_MIDDLE_OF_THE"));
            if journal.is_empty() {
                note(t, tr!("READING_THE_QUESTS_THEY_APPEAR_A"));
                return;
            }
            let followed = crate::quests::followed(&journal, state.quest.as_deref()).map(|q| q.key.clone());
            let mut pick: Option<Option<String>> = None;
            let auto = match journal.iter().find(|q| Some(&q.key) == followed.as_ref()) {
                Some(q) if state.quest.is_none() => trf!("MAIN_STORY_AUTO_NOW", quest = q.name),
                _ => tr!("MAIN_STORY_AUTO").to_string(),
            };
            if tw::line(t, Some(state.quest.is_none()), |ui| ui.add_space(16.0), auto, |_| {}) {
                pick = Some(None);
            }
            for q in journal.iter().filter(|q| q.active()) {
                // The followed quest is marked, whether chosen or followed automatically.
                let on = Some(&q.key) == followed.as_ref();
                let (kind, tag) = (q.kind, q.kind.label());
                let progress = q.progress.filter(|(_, all)| *all > 0);
                let icon = |ui: &mut egui::Ui| {
                    crate::ui::svg::quest(ui, kind, 16.0).on_hover_text(tag);
                };
                // A main quest's clues: a small meter and the count at the line's end.
                let end = |t: &mut Tui| {
                    if let Some((got, all)) = progress {
                        let hover = trf!("CLUES", got = got, all = all).trim_start_matches([' ', '·']).to_string();
                        w(t, |ui| tw::meter(ui, Some(40.0), got, all));
                        w(t, |ui| ui.label(count(got, all)).on_hover_text(hover));
                    }
                };
                if tw::line(t, Some(on), icon, q.name.as_str(), end) {
                    pick = Some(Some(q.key.clone()));
                }
            }
            let done = journal.iter().filter(|q| q.status == Status::Completed).count();
            let failed = journal.iter().filter(|q| q.status == Status::Failed).count();
            t.style(tw::row(TIGHT)).add(|t| {
                tw::chip(t, trf!("QUESTS_DONE", n = done), if done > 0 { tw::Tone::Ok } else { tw::Tone::Quiet });
                tw::chip(
                    t,
                    trf!("QUESTS_FAILED", n = failed),
                    if failed > 0 { tw::Tone::Bad } else { tw::Tone::Quiet },
                );
            });
            // What the followed quest needs, from the survey of every world.
            let needs = snap
                .and_then(|s| s.needs.iter().find(|(k, _)| Some(k) == followed.as_ref()))
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            let here_world =
                snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
            let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
            let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
            if needs.is_empty() {
                note(t, tr!("NEEDED_NO_SURVEY_DB_RUN_HIUMOD"));
            } else {
                let left = needs.iter().filter(|x| !x.done).count();
                let all = needs.len();
                // The header: what is found of all it needs, as a meter.
                t.style(tw::row(INLINE)).add(|t| {
                    block(t, |ui| {
                        ui.label(RichText::new(tr!("QUEST_NEEDS")).strong().color(TITLE)).on_hover_text(trf!(
                            "NEEDED_LEFT",
                            left = left,
                            all = all
                        ));
                    });
                    w(t, |ui| tw::meter(ui, Some(72.0), all - left, all));
                    w(t, |ui| ui.label(count(all - left, all)));
                });
                // This world's, nearest first: press to guide there.
                let mut mine: Vec<&crate::survey::Need> =
                    needs.iter().filter(|x| !x.done && Some(&x.world) == here_world.as_ref()).collect();
                let d = |x: &crate::survey::Need| here.map_or(0.0, |h| (x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0);
                mine.sort_by(|a, b| d(a).total_cmp(&d(b)));
                let far = |x: &crate::survey::Need| here.map_or(String::new(), |h| crate::raster::span(h, x.at));
                for x in mine.iter().take(8) {
                    let label = match x.what.as_str() {
                        "" => x.label.clone(),
                        w if w == x.label => w.to_string(),
                        w => format!("{w}: {}", x.label),
                    };
                    // A person to talk to, or a thing to take.
                    let npc = x.label.starts_with(trf!("TALK_NPC", p = "").as_str());
                    let sort = if npc { crate::actors::Sub::Npc } else { crate::actors::Sub::Quest };
                    let span = far(x);
                    let icon = |ui: &mut egui::Ui| {
                        crate::ui::svg::sort(ui, sort, 16.0);
                    };
                    if tw::line(t, Some(state.target == Some(x.id)), icon, label, |t| distance(t, span)) {
                        guide_to(state, &goals, x);
                    }
                }
                // Other worlds: how many each, as chips; the way there on hover.
                let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
                for x in needs.iter().filter(|x| !x.done && Some(&x.world) != here_world.as_ref()) {
                    *elsewhere.entry(x.world.as_str()).or_default() += 1;
                }
                if !elsewhere.is_empty() {
                    t.style(tw::wrap(TIGHT)).add(|t| {
                        for (world, n) in elsewhere.iter() {
                            let place = crate::i18n::place(world);
                            let hover = trf!("TAKE_THE_APC", place = place);
                            w(t, |ui| tw::pill(ui, format!("{place} {n}"), tw::Tone::Quiet).on_hover_text(hover));
                        }
                    });
                }
            }
            if let Some(p) = pick {
                state.quest = p;
                // Guide anew, to the newly followed quest.
                state.target = None;
                state.chosen = false;
                state.guide_auto = true;
                state.route = true;
                state.dirty = true;
            }
        });
    }

    /// Missable good deeds and their deadlines; in act 2, the keystones left and what is
    /// due before the next (missables.rs).
    pub(super) fn deadlines_card(
        &mut self,
        t: &mut Tui,
        state: &mut crate::minimap::MapState,
        snap: Option<&Snapshot>,
    ) {
        use crate::missables::When;
        let list = snap.map(|s| s.deadlines.clone()).unwrap_or_default();
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        card(t, tr!("MISSABLE_GOOD_DEEDS"), |t| {
            if list.is_empty() {
                note(t, tr!("EVERY_GOOD_DEED_WITH_A_DEADLINE"));
            }
            for d in list.iter().filter(|d| d.when != When::Passed) {
                let (mark, colour) = match d.when {
                    When::Now => (tr!("SOON"), tw::Tone::Bad),
                    _ => (tr!("LATER"), tw::Tone::Quiet),
                };
                tw::item(t, |t| {
                    // The title line: when, the deed (press to follow it once it is started),
                    // and a chip while it is not.
                    let on = d.started.then_some(state.quest.as_deref() == Some(d.key.as_str()));
                    let icon = |ui: &mut egui::Ui| {
                        tw::pill(ui, mark, colour);
                    };
                    let end = |t: &mut Tui| {
                        if !d.started {
                            let not = tr!("NOT_STARTED").trim().trim_matches(['(', ')']).to_string();
                            // Most deeds are not started: grey, so the chips that need
                            // attention (soon) are the only coloured ones.
                            tw::chip(t, not, tw::Tone::Quiet);
                        }
                    };
                    if tw::line(t, on, icon, d.title.as_str(), end) {
                        state.quest = Some(d.key.clone());
                        state.target = None;
                        state.chosen = false;
                        state.guide_auto = true;
                        state.route = true;
                    }
                    // One dim detail line: the deadline, what to do cut short, all of it on hover.
                    let detail = format!("{}: {}", d.due.label(), d.what);
                    block(t, |ui| {
                        let short = clip(&detail, DETAIL);
                        let r = ui.add(egui::Label::new(RichText::new(short).small().color(DIM)).wrap());
                        if detail.chars().count() > DETAIL {
                            r.on_hover_text(detail.as_str());
                        }
                    });
                });
            }
            let passed = list.iter().filter(|d| d.when == When::Passed).count();
            if passed > 0 {
                note(t, trf!("DEADLINES_ALREADY_PAST_THOSE_GOOD_DEEDS", passed = passed));
            }
        });
        if let Some((left, before)) = crate::missables::keystone_advice(&journal, &list) {
            card(t, tr!("KEYSTONE_ORDER"), |t| {
                note(t, trf!("KEYSTONE_ORDER_ADVICE"));
                for (i, l) in left.iter().enumerate() {
                    text(t, format!("{}. {l}", i + 1));
                }
                if !before.is_empty() {
                    text(t, RichText::new(trf!("BEFORE_THE_NEXT_KEYSTONE", deeds = before.join(" · "))).color(BAD));
                }
            });
        }
    }

    /// Items the hero holds that someone wants: who, where — press to guide there.
    pub(super) fn handovers_card(
        &mut self,
        t: &mut Tui,
        state: &mut crate::minimap::MapState,
        snap: Option<&Snapshot>,
    ) {
        let list = snap.map(|s| s.handovers.clone()).unwrap_or_default();
        if list.is_empty() {
            card(t, tr!("THINGS_TO_HAND_OVER"), |t| note(t, tr!("NO_ONE_WANTS_AN_ITEM_YOU")));
            return;
        }
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        card(t, &trf!("THINGS_TO_HAND_OVER_COUNT", count = list.len()), |t| {
            note(t, tr!("PEOPLE_WHO_WANT_AN_ITEM_YOU"));
            for x in list.iter() {
                let same = Some(&x.world) == here_world.as_ref();
                let label = format!("{} → {}", x.what, x.label.trim_start_matches(trf!("TALK_NPC", p = "").as_str()));
                let icon = |ui: &mut egui::Ui| {
                    crate::ui::svg::sort(ui, crate::actors::Sub::Npc, 16.0);
                };
                // In this world: press to guide there, the distance at the end. Elsewhere: not
                // pressable, the way there as a chip.
                let on = same.then_some(state.target == Some(x.id));
                let end = |t: &mut Tui| {
                    if same {
                        distance(t, here.map_or(String::new(), |h| crate::raster::span(h, x.at)));
                    } else {
                        tw::chip(t, trf!("TAKE_THE_APC", place = crate::i18n::place(&x.world)), tw::Tone::Quiet);
                    }
                };
                if tw::line(t, on, icon, label, end) {
                    guide_to(state, &goals, x);
                }
            }
        });
    }
}
