//! The collect page: collectibles taken here and everywhere, secrets done, NPCs with more to tell.

use super::*;

impl Panel {
    /// Collectibles placed in the worlds: taken here and everywhere, and the nearest
    /// left here, per sort (survey.rs `collection`).
    pub(super) fn collection_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.collection.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        card(t, tr!("COLLECTION"), |t| {
            if list.is_empty() {
                note(t, tr!("NO_SURVEY_DB_RUN_DOCTOR_SURVEY"));
                return;
            }
            note(t, tr!("COUNTS_WHAT_LIES_IN_THE_WORLD"));
            for c in list.iter() {
                let open = self.unfolded_collect == Some(c.label);
                t.style(tw::row(8.0)).add(|t| {
                    w(t, |ui| crate::ui::svg::sort(ui, crate::survey::collect_sort(c.label), 18.0));
                    let label = trf!("COLLECT_HERE_AND_ALL", sort = crate::i18n::tr(c.label), got_here = c.here.0, all_here = c.here.1, got = c.all.0, all = c.all.1);
                    let done = c.here.0 == c.here.1;
                    if tw::pick(t, open, RichText::new(label).color(if done { DIM } else { super::super::theme::TEXT })) {
                        self.unfolded_collect = if open { None } else { Some(c.label) };
                    }
                });
                if open {
                    let mut left = c.left_here.clone();
                    if let Some(h) = here {
                        left.sort_by(|a, b| (a.at[0] - h[0]).hypot(a.at[1] - h[1]).total_cmp(&(b.at[0] - h[0]).hypot(b.at[1] - h[1])));
                    }
                    for x in left.iter().take(8) {
                        let far = here.map_or(String::new(), |h| crate::raster::distance((x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0));
                        let sort = crate::survey::collect_sort(c.label);
                        if tw::pick_with(t, state.target == Some(x.id), |ui| { ui.add_space(14.0); crate::ui::svg::sort(ui, sort, 16.0); }, format!("{} ({far})", x.label)) {
                            guide_to(state, &goals, x);
                        }
                    }
                }
            }
        });
    }

    /// Good deeds, mysteries and timeloops: done of all.
    pub(super) fn secrets_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        use crate::quests::{Kind, Status};
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let totals = snap.map_or([0; 3], |s| s.secret_totals);
        card(t, tr!("GOOD_DEEDS_MYSTERIES_TIMELOOPS"), |t| {
            for (i, (kind, _)) in Kind::SECRETS.iter().enumerate() {
                let of = |s: Status| journal.iter().filter(|q| q.kind == *kind && q.status == s).count();
                t.style(tw::row(6.0)).add(|t| {
                    w(t, |ui| crate::ui::svg::quest(ui, *kind, 18.0));
                    w(t, |ui| ui.label(RichText::new(kind.label()).strong()));
                    text(t, trf!("DONE_IN_PROGRESS_FAILED", done = of(Status::Completed), all = totals[i], started = of(Status::Started), failed = of(Status::Failed)));
                });
            }
        });
    }

    /// NPCs whose talk still holds something new: here nearest first, elsewhere by count.
    pub(super) fn stories_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.stories.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        card(t, &trf!("NPCS_WITH_MORE_TO_TELL", count = list.len()), |t| {
            note(t, tr!("PEOPLE_WHOSE_TALK_CAN_STILL_GIVE"));
            let mut mine: Vec<&crate::survey::Need> = list.iter().filter(|x| Some(&x.world) == here_world.as_ref()).collect();
            if let Some(h) = here {
                mine.sort_by(|a, b| (a.at[0] - h[0]).hypot(a.at[1] - h[1]).total_cmp(&(b.at[0] - h[0]).hypot(b.at[1] - h[1])));
            }
            for x in mine.iter().take(10) {
                let far = here.map_or(String::new(), |h| crate::raster::distance((x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0));
                if tw::pick_with(t, state.target == Some(x.id), |ui| { crate::ui::svg::sort(ui, crate::actors::Sub::Npc, 16.0); }, format!("{} ({far})", x.label.trim_start_matches(trf!("TALK_NPC", p = "").as_str()))) {
                    guide_to(state, &goals, x);
                }
            }
            let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for x in list.iter().filter(|x| Some(&x.world) != here_world.as_ref()) {
                *elsewhere.entry(x.world.as_str()).or_default() += 1;
            }
            if !elsewhere.is_empty() {
                note(t, trf!("OTHER_REGIONS", regions = elsewhere.iter().map(|(w, n)| format!("{} {n}", crate::i18n::place(w))).collect::<Vec<_>>().join(" · ")));
            }
        });
    }
}
