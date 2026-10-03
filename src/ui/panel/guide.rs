//! The guide page: the compass, the guide's target and why, every place with something new.

use super::super::theme::INLINE;
use super::*;

impl Panel {
    pub(super) fn guide_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let dist = |g: &crate::goals::Goal| {
            here.map_or(f32::MAX, |h| ((g.at[0] - h[0]).powi(2) + (g.at[1] - h[1]).powi(2)).sqrt() / 100.0)
        };

        card(t, tr!("COMPASS_AND_HEADING"), |t| {
            field(t, trf!("COMPASS_F", key = state.compass_key), |t| {
                w(t, |ui| toggle(ui, &mut state.compass));
                w(t, |ui| ui.label(RichText::new(tr!("TOP_CENTRE_OF_THE_SCREEN")).color(DIM).small()));
            });
        });

        // Why nothing is being guided to, when nothing is.
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let followed = crate::quests::followed(&journal, state.quest.as_deref()).cloned();
        let elsewhere: usize = snap
            .and_then(|s| followed.as_ref().and_then(|q| s.needs.iter().find(|(k, _)| *k == q.key)))
            .map(|(_, list)| {
                let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
                list.iter().filter(|x| !x.done && Some(&x.world) != here.as_ref()).count()
            })
            .unwrap_or(0);
        card(t, tr!("GUIDE"), |t| {
            switch(t, &mut state.guide_auto, tr!("AUTO_GUIDE_KEEPS_GUIDING_TO_THE"));
            switch(
                t,
                &mut state.route,
                tr!("WALKING_ROUTE_A_AROUND_TERRAIN_WATER"),
            );
            choices(t, |t| {
                for tier in crate::goals::Tier::ALL {
                    let [r, g, b] = tier.rgb();
                    let n = goals.iter().filter(|x| x.tier == tier).count();
                    let on = state.goal_tiers & (1 << tier as u8) != 0;
                    if w(t, |ui| ui.add(chip(&format!("{} ({n})", tier.label()), on, Color32::from_rgb(r, g, b))))
                        .clicked()
                    {
                        state.goal_tiers ^= 1 << tier as u8;
                    }
                }
            });
            match state.target.and_then(|id| goals.iter().find(|g| g.id == id)) {
                Some(g) => {
                    t.style(tw::row(INLINE)).add(|t| {
                        w(t, |ui| crate::ui::svg::tier(ui, g.tier, 18.0));
                        text(t, RichText::new(&g.label).strong());
                        w(t, |ui| ui.label(RichText::new(crate::raster::distance(dist(g))).color(DIM)));
                        if w(t, |ui| ui.button(tr!("NEXT_GOAL")))
                            .on_hover_text(tr!("THIS_GOAL_IS_DONE_OR_OUT"))
                            .clicked()
                        {
                            state.skipped.insert(g.id);
                            state.target = None;
                            state.chosen = false;
                            state.guide_auto = true;
                        }
                    });
                    if state.chosen {
                        t.style(tw::row(INLINE)).add(|t| {
                            text(t, RichText::new(tr!("PICKED_BY_HAND_KEPT_UNTIL_IT")).color(DIM).small());
                            if w(t, |ui| ui.small_button(tr!("BACK_TO_AUTO"))).clicked() {
                                state.chosen = false;
                                state.target = None;
                                state.guide_auto = true;
                            }
                        });
                    }
                    note(t, g.detail.clone());
                    if state.route && *self.shared.route_uncertain.lock().unwrap() {
                        text(
                            t,
                            RichText::new(
                                tr!("NO_WALKING_ROUTE_IT_MAY_BE"),
                            )
                            .color(WAIT)
                            .small(),
                        );
                    }
                }
                None => {
                    let why = if !state.guide_auto {
                        tr!("AUTO_GUIDE_IS_OFF_TURN_ON").to_string()
                    } else if goals.is_empty() {
                        tr!("NOTHING_NEW_TO_GET_FOUND_NEARBY").to_string()
                    } else if elsewhere > 0 {
                        trf!("NO_QUEST_GOAL_IN_REGION", elsewhere = elsewhere)
                    } else if !state.skipped.is_empty() {
                        tr!("EVERY_REMAINING_GOAL_WAS_SKIPPED").to_string()
                    } else {
                        tr!("NO_QUEST_GOAL_NEARBY").to_string()
                    };
                    text(t, RichText::new(why).color(DIM));
                }
            }
            if !state.skipped.is_empty() {
                t.style(tw::row(INLINE)).add(|t| {
                    text(t, RichText::new(trf!("GOALS_SKIPPED", count = state.skipped.len())).color(DIM).small());
                    if w(t, |ui| ui.small_button(tr!("UNDO"))).clicked() {
                        state.skipped.clear();
                        state.target = None;
                    }
                });
            }
        });

    }

    /// Every place with something new, nearest first: press to guide there.
    pub(super) fn goals_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let dist = |g: &crate::goals::Goal| {
            here.map_or(f32::MAX, |h| ((g.at[0] - h[0]).powi(2) + (g.at[1] - h[1]).powi(2)).sqrt() / 100.0)
        };
        let mut list: Vec<&crate::goals::Goal> =
            goals.iter().filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0).collect();
        list.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
        card(t, &trf!("PLACES", count = list.len()), |t| {
            block(t, |ui| {
                // min_scrolled_height too: the block is laid out at last frame's height, and
                // a scroll area alone never asks for more than it was given.
                egui::ScrollArea::vertical().max_height(220.0).min_scrolled_height(220.0).show(ui, |ui| {
                    for g in list {
                        let chosen = state.target == Some(g.id);
                        ui.horizontal(|ui| {
                            crate::ui::svg::tier(ui, g.tier, 16.0);
                            let label = format!("{}  ({})", g.label, crate::raster::distance(dist(g)));
                            let button = egui::Button::selectable(chosen, label).truncate();
                            if ui.add(button).on_hover_text(&g.detail).clicked() {
                                state.target = Some(g.id);
                                state.chosen = true;
                            }
                        });
                    }
                });
            });
        });

    }
}
