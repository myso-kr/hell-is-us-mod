//! The collect page: collectibles taken here and everywhere, secrets done, NPCs with more to tell.

use super::super::theme::INLINE;
use super::*;

impl Panel {
    /// Collectibles placed in the worlds: taken here and everywhere per sort; opened, the
    /// nearest left here and how many are left in each other region — the completion
    /// board (.spec/JOURNEY.md §3.9) without a page of its own.
    pub(super) fn collection_card(
        &mut self,
        t: &mut Tui,
        state: &mut crate::minimap::MapState,
        snap: Option<&Snapshot>,
    ) {
        let list = snap.map(|s| s.collection.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        card(t, tr!("COLLECTION"), |t| {
            if list.is_empty() {
                note(t, tr!("NO_SURVEY_DB_RUN_DOCTOR_SURVEY"));
                return;
            }
            note(t, tr!("COUNTS_WHAT_LIES_IN_THE_WORLD"));
            let places = self.grants(crate::settings::Consent::PLACES);
            for c in list.iter() {
                // Where each one left lies is "where hidden things are": without it, counts.
                let open = places && self.unfolded_collect == Some(c.label);
                t.style(tw::row(INLINE)).add(|t| {
                    w(t, |ui| crate::ui::svg::sort(ui, crate::survey::collect_sort(c.label), 18.0));
                    let done = c.here.0 == c.here.1;
                    let label = RichText::new(crate::i18n::tr(c.label));
                    if tw::pick(t, open, label.color(if done { DIM } else { super::super::theme::TEXT })) && places {
                        self.unfolded_collect = if open { None } else { Some(c.label) };
                    }
                    // This region on the meter; everywhere, dim, after it.
                    w(t, |ui| tw::meter(ui, Some(56.0), c.here.0, c.here.1));
                    let counts = RichText::new(format!("{}/{}", c.here.0, c.here.1)).monospace().size(11.5);
                    w(t, |ui| ui.label(counts.color(super::super::theme::TEXT)));
                    let all = RichText::new(format!("{}/{}", c.all.0, c.all.1)).monospace().size(11.5);
                    w(t, |ui| ui.label(all.color(DIM)));
                });
                if open {
                    let mut left = c.left_here.clone();
                    if let Some(h) = here {
                        left.sort_by(|a, b| {
                            (a.at[0] - h[0]).hypot(a.at[1] - h[1]).total_cmp(&(b.at[0] - h[0]).hypot(b.at[1] - h[1]))
                        });
                    }
                    for x in left.iter().take(8) {
                        let far = here.map_or(String::new(), |h| crate::raster::span(h, x.at));
                        let sort = crate::survey::collect_sort(c.label);
                        if tw::pick_with(
                            t,
                            state.is_followed(x.id),
                            |ui| {
                                ui.add_space(14.0);
                                crate::ui::svg::sort(ui, sort, 16.0);
                            },
                            format!("{} ({far})", x.label),
                        ) {
                            guide_to(state, &goals, x);
                        }
                    }
                    // Where the rest are, most first.
                    let rest = c
                        .left_by_world
                        .iter()
                        .filter(|(w, _)| Some(w) != here_world.as_ref())
                        .map(|(w, n)| (crate::i18n::place(w), *n))
                        .collect();
                    tw::regions(t, tr!("OTHER_REGIONS_CARD"), rest);
                }
            }
        });
    }

    /// The Collect page's hero: the four counts that make a full game, as rings —
    /// collectibles taken, enemy groups beaten, achievements, and secrets done.
    pub(super) fn collect_hero(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.collection.clone()).unwrap_or_default();
        let (got, all) = list.iter().fold((0, 0), |(g, a), c| (g + c.all.0, a + c.all.1));
        let hollows = snap.map(|s| s.hollows.clone()).unwrap_or_default();
        let (left, groups) = hollows.iter().fold((0, 0), |(l, a), h| (l + h.left, a + h.all));
        let achievements = self.achievements_list();
        let unlocked = achievements.iter().filter(|a| a.unlocked).count();
        // Good deeds, mysteries and timeloops are the Quests page's: not counted twice.
        tw::hero(t, |t| {
            t.style(tw::grow(tw::grid(3, super::super::theme::PAD))).add(|t| {
                for (label, d, a) in [
                    (tr!("COLLECTION"), got, all),
                    (tr!("STAT_ENEMY_GROUPS"), groups - left, groups),
                    (tr!("ACHIEVEMENTS_RING"), unlocked, achievements.len()),
                ] {
                    w(t, |ui| ui.vertical_centered(|ui| tw::ring(ui, 96.0, d, a, label)));
                }
            });
        });
    }

    /// Good deeds, mysteries and timeloops: done of all.
    pub(super) fn secrets_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        use crate::quests::{Kind, Status};
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let totals = snap.map_or([0; 3], |s| s.secret_totals);
        card(t, tr!("GOOD_DEEDS_MYSTERIES_TIMELOOPS"), |t| {
            // One ring each: done of all; under way and failed on hover.
            t.style(tw::grid(3, INLINE)).add(|t| {
                for (i, (kind, _)) in Kind::SECRETS.iter().enumerate() {
                    let of = |s: Status| journal.iter().filter(|q| q.kind == *kind && q.status == s).count();
                    let hover = trf!(
                        "DONE_IN_PROGRESS_FAILED",
                        done = of(Status::Completed),
                        all = totals[i],
                        started = of(Status::Started),
                        failed = of(Status::Failed)
                    );
                    w(t, |ui| {
                        ui.vertical_centered(|ui| {
                            tw::ring(ui, 72.0, of(Status::Completed), totals[i], &kind.label()).on_hover_text(hover);
                        })
                    });
                }
            });
        });
    }

    /// NPCs whose talk still holds something new: here nearest first, elsewhere by count.
    pub(super) fn stories_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.stories.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        card(t, &trf!("NPCS_WITH_MORE_TO_TELL", count = list.len()), |t| {
            note(t, tr!("PEOPLE_WHOSE_TALK_CAN_STILL_GIVE"));
            let mut mine: Vec<&crate::survey::Need> =
                list.iter().filter(|x| Some(&x.world) == here_world.as_ref()).collect();
            if let Some(h) = here {
                mine.sort_by(|a, b| {
                    (a.at[0] - h[0]).hypot(a.at[1] - h[1]).total_cmp(&(b.at[0] - h[0]).hypot(b.at[1] - h[1]))
                });
            }
            for x in mine.iter().take(10) {
                let far = here.map_or(String::new(), |h| crate::raster::span(h, x.at));
                if tw::pick_with(
                    t,
                    state.is_followed(x.id),
                    |ui| {
                        crate::ui::svg::sort(ui, crate::actors::Sub::Npc, 16.0);
                    },
                    format!("{} ({far})", x.label.trim_start_matches(trf!("TALK_NPC", p = "").as_str())),
                ) {
                    guide_to(state, &goals, x);
                }
            }
            let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for x in list.iter().filter(|x| Some(&x.world) != here_world.as_ref()) {
                *elsewhere.entry(x.world.as_str()).or_default() += 1;
            }
            let counts = elsewhere.iter().map(|(w, n)| (crate::i18n::place(w), *n)).collect();
            tw::regions(t, tr!("OTHER_REGIONS_CARD"), counts);
        });
    }
}
