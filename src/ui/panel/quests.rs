//! The quest page: the journal and what the followed quest needs, missable deadlines, hand-overs.

use super::super::theme::INLINE;
use super::*;

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
            if tw::pick(t, state.quest.is_none(), auto) {
                pick = Some(None);
            }
            for q in journal.iter().filter(|q| q.active()) {
                let tag = q.kind.label();
                let mut label = format!("[{tag}] {}", q.name);
                if let Some((got, all)) = q.progress.filter(|(_, all)| *all > 0) {
                    label += &trf!("CLUES", got = got, all = all);
                }
                let on = state.quest.as_deref() == Some(q.key.as_str());
                let kind = q.kind;
                if tw::pick_with(
                    t,
                    on,
                    |ui| {
                        crate::ui::svg::quest(ui, kind, 16.0);
                    },
                    label,
                ) {
                    pick = Some(Some(q.key.clone()));
                }
            }
            let done = journal.iter().filter(|q| q.status == Status::Completed).count();
            let failed = journal.iter().filter(|q| q.status == Status::Failed).count();
            note(t, trf!("DONE_FAILED", done = done, failed = failed));
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
                text(t, RichText::new(trf!("NEEDED_LEFT", left = left, all = needs.len())).strong());
                // This world's, nearest first: press to guide there.
                let mut mine: Vec<&crate::survey::Need> =
                    needs.iter().filter(|x| !x.done && Some(&x.world) == here_world.as_ref()).collect();
                let d = |x: &crate::survey::Need| here.map_or(0.0, |h| (x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0);
                mine.sort_by(|a, b| d(a).total_cmp(&d(b)));
                let far = |x: &crate::survey::Need| here.map_or(String::new(), |h| crate::raster::span(h, x.at));
                for x in mine.iter().take(8) {
                    let label = match x.what.as_str() {
                        "" => format!("{} ({})", x.label, far(x)),
                        w if w == x.label => format!("{w} ({})", far(x)),
                        w => format!("{w}: {} ({})", x.label, far(x)),
                    };
                    // A person to talk to, or a thing to take.
                    let npc = x.label.starts_with(trf!("TALK_NPC", p = "").as_str());
                    let sort = if npc { crate::actors::Sub::Npc } else { crate::actors::Sub::Quest };
                    if tw::pick_with(
                        t,
                        state.target == Some(x.id),
                        |ui| {
                            crate::ui::svg::sort(ui, sort, 16.0);
                        },
                        label,
                    ) {
                        guide_to(state, &goals, x);
                    }
                }
                // Other worlds: how many, where.
                let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
                for x in needs.iter().filter(|x| !x.done && Some(&x.world) != here_world.as_ref()) {
                    *elsewhere.entry(x.world.as_str()).or_default() += 1;
                }
                if !elsewhere.is_empty() {
                    let list: Vec<String> =
                        elsewhere.iter().map(|(w, n)| format!("{} {n}", crate::i18n::place(w))).collect();
                    note(t, trf!("OTHER_REGIONS_TAKE_THE_APC", regions = list.join(" · ")));
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
                    t.style(tw::row(INLINE)).add(|t| {
                        tw::chip(t, mark, colour);
                        let label = format!("{}{}", d.title, if d.started { "" } else { tr!("NOT_STARTED") });
                        if tw::pick(t, state.quest.as_deref() == Some(d.key.as_str()), label) && d.started {
                            state.quest = Some(d.key.clone());
                            state.target = None;
                            state.chosen = false;
                            state.guide_auto = true;
                            state.route = true;
                        }
                    });
                    note(t, format!("{}: {}", d.due.label(), d.what));
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
                let place = if same {
                    here.map_or(String::new(), |h| crate::raster::span(h, x.at))
                } else {
                    trf!("TAKE_THE_APC", place = crate::i18n::place(&x.world))
                };
                let label =
                    format!("{} → {} ({place})", x.what, x.label.trim_start_matches(trf!("TALK_NPC", p = "").as_str()));
                if same {
                    if tw::pick_with(
                        t,
                        state.target == Some(x.id),
                        |ui| {
                            crate::ui::svg::sort(ui, crate::actors::Sub::Npc, 16.0);
                        },
                        label,
                    ) {
                        guide_to(state, &goals, x);
                    }
                } else {
                    text(t, RichText::new(label).color(DIM));
                }
            }
        });
    }
}
