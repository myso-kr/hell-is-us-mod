//! The guide page: the guide's target and why, the tiers it may pick from, its switches
//! (compass included), and every place with something new.

use super::super::theme::{
    ACCENT, ACCENT_DEEP, BLOCK, CONTROL, CONTROL_HOVER, GROUND, INLINE, R_CONTROL, TEXT, TIGHT, TITLE,
};
use super::*;
use egui_taffy::taffy::{prelude::length, Style};

impl Panel {
    pub(super) fn guide_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);

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
        let uncertain = state.route && *self.shared.route_uncertain.lock().unwrap();
        let target = state.target.and_then(|id| goals.iter().find(|g| g.id == id)).cloned();

        card(t, tr!("GUIDE"), |t| {
            // The target first, in large type: what the guide points at, how far, and why.
            well(t, |t| match &target {
                Some(g) => {
                    t.style(tw::row(BLOCK)).add(|t| {
                        w(t, |ui| crate::ui::svg::tier(ui, g.tier, 28.0).on_hover_text(g.tier.label()));
                        t.style(tw::grow(tw::col(TIGHT))).add(|t| {
                            block(t, |ui| {
                                let name = RichText::new(&g.label).size(16.0).strong().color(TITLE);
                                ui.add(egui::Label::new(name).wrap()).on_hover_text(&g.detail);
                            });
                            t.style(tw::wrap(TIGHT)).add(|t| {
                                let (mode, tone, why) = if state.chosen {
                                    (tr!("GUIDE_BY_HAND"), tw::Tone::Accent, tr!("PICKED_BY_HAND_KEPT_UNTIL_IT"))
                                } else {
                                    (tr!("GUIDE_AUTO"), tw::Tone::Quiet, tr!("AUTO_GUIDE_KEEPS_GUIDING_TO_THE"))
                                };
                                w(t, |ui| tw::pill(ui, mode, tone).on_hover_text(why));
                                if uncertain {
                                    w(t, |ui| {
                                        tw::pill(ui, tr!("GUIDE_NO_ROUTE"), tw::Tone::Wait)
                                            .on_hover_text(tr!("NO_WALKING_ROUTE_IT_MAY_BE"))
                                    });
                                }
                            });
                        });
                        // The distance as the card's figure; the height under it, smaller.
                        if let Some(h) = here {
                            let span = crate::raster::span(h, g.at);
                            let (across, height) = span.split_once(' ').unwrap_or((span.as_str(), ""));
                            w(t, |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(RichText::new(across).size(24.0).strong().color(TITLE));
                                    if !height.is_empty() {
                                        ui.label(RichText::new(height).small().monospace().color(DIM));
                                    }
                                });
                            });
                        }
                    });
                    note(t, g.detail.clone());
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
            });

            // The actions, normal size, and what was skipped with its undo.
            if target.is_some() || !state.skipped.is_empty() {
                t.style(tw::wrap(INLINE)).add(|t| {
                    if let Some(g) = &target {
                        if w(t, |ui| ui.button(tr!("NEXT_GOAL")))
                            .on_hover_text(tr!("THIS_GOAL_IS_DONE_OR_OUT"))
                            .clicked()
                        {
                            state.skipped.insert(g.id);
                            state.target = None;
                            state.chosen = false;
                            state.guide_auto = true;
                        }
                        if state.chosen
                            && w(t, |ui| ui.button(tr!("BACK_TO_AUTO")))
                                .on_hover_text(tr!("PICKED_BY_HAND_KEPT_UNTIL_IT"))
                                .clicked()
                        {
                            state.chosen = false;
                            state.target = None;
                            state.guide_auto = true;
                        }
                    }
                    if !state.skipped.is_empty() {
                        tw::chip(t, trf!("GOALS_SKIPPED", count = state.skipped.len()), tw::Tone::Quiet);
                        if w(t, |ui| ui.button(tr!("UNDO"))).clicked() {
                            state.skipped.clear();
                            state.target = None;
                        }
                    }
                });
            }

            segments(t, &goals, &mut state.goal_tiers);

            // The switches, short; what each does is in its hover.
            t.style(tw::wrap(BLOCK)).add(|t| {
                flag(t, &mut state.guide_auto, tr!("GUIDE_AUTO"), tr!("AUTO_GUIDE_KEEPS_GUIDING_TO_THE"));
                flag(t, &mut state.route, tr!("GUIDE_ROUTE"), tr!("WALKING_ROUTE_A_AROUND_TERRAIN_WATER"));
                let compass = trf!("COMPASS_F", key = state.compass_key);
                flag(t, &mut state.compass, &compass, tr!("TOP_CENTRE_OF_THE_SCREEN"));
            });
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
        let grouped = &mut self.places_grouped;
        card(t, &trf!("PLACES", count = list.len()), |t| {
            tw::order(t, grouped, tr!("BY_DISTANCE"), tr!("BY_KIND"));
            // By kind: one heading per tier, each nearest first; else all nearest first.
            let groups: Vec<(Option<crate::goals::Tier>, Vec<&crate::goals::Goal>)> = if *grouped {
                crate::goals::Tier::ALL
                    .iter()
                    .map(|t| (Some(*t), list.iter().copied().filter(|g| g.tier == *t).collect::<Vec<_>>()))
                    .filter(|(_, l)| !l.is_empty())
                    .collect()
            } else {
                vec![(None, list)]
            };
            block(t, |ui| {
                // min_scrolled_height too: the block is laid out at last frame's height, and
                // a scroll area alone never asks for more than it was given.
                tw::scroll(ui, "places", 220.0, 220.0, super::super::theme::CARD, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (tier, list) in groups {
                        if let Some(tier) = tier {
                            tw::group_heading(ui, tier.label(), list.len());
                        }
                        for g in list {
                            let far = here.map_or(String::new(), |h| crate::raster::span(h, g.at));
                            if place_row(ui, g, &far, state.target == Some(g.id)).clicked() {
                                state.target = Some(g.id);
                                state.chosen = true;
                            }
                        }
                    }
                });
            });
        });
    }
}

/// A quiet inset inside a card (a fill, no edge) that holds the page's main figure.
fn well<T>(t: &mut Tui, body: impl FnOnce(&mut Tui) -> T) -> T {
    let style = Style { padding: length(BLOCK), ..tw::col(INLINE) };
    t.style(style)
        .add_with_background_ui(
            |ui, c| {
                ui.painter().rect_filled(c.full_container(), R_CONTROL, CONTROL);
            },
            |t, _| body(t),
        )
        .main
}

/// The tier filter as one segmented control: each tier with its count, lit in the
/// tier's own colour while the guide and the list may pick from it.
fn segments(t: &mut Tui, goals: &[crate::goals::Goal], tiers: &mut u8) {
    w(t, |ui| {
        egui::Frame::new().fill(GROUND).corner_radius(R_CONTROL).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().button_padding = egui::vec2(10.0, 2.0);
                let font = egui::TextStyle::Small.resolve(ui.style());
                for tier in crate::goals::Tier::ALL {
                    let [r, g, b] = tier.rgb();
                    let colour = Color32::from_rgb(r, g, b);
                    let n = goals.iter().filter(|x| x.tier == tier).count();
                    let on = *tiers & (1 << tier as u8) != 0;
                    let mut label = egui::text::LayoutJob::default();
                    label.append(
                        tier.label(),
                        0.0,
                        egui::TextFormat::simple(font.clone(), if on { colour } else { DIM }),
                    );
                    label.append(
                        &n.to_string(),
                        6.0,
                        egui::TextFormat::simple(egui::FontId::monospace(font.size), DIM),
                    );
                    let button = egui::Button::new(label)
                        .fill(if on { colour.gamma_multiply(0.16) } else { Color32::TRANSPARENT })
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(R_CONTROL - 1);
                    if ui.add(button).clicked() {
                        *tiers ^= 1 << tier as u8;
                    }
                }
            });
        });
    });
}

/// A switch with a short label (pressable too); the long explanation is its hover.
fn flag(t: &mut Tui, on: &mut bool, label: &str, hover: &str) {
    t.style(tw::row(TIGHT + 2.0)).add(|t| {
        w(t, |ui| toggle(ui, on).on_hover_text(hover));
        let pressed = w(t, |ui| ui.add(egui::Label::new(label).sense(egui::Sense::click())).on_hover_text(hover));
        if pressed.clicked() {
            *on = !*on;
        }
    });
}

/// One place: its tier's icon, its name (cut to fit), the distance right-aligned in a
/// dim monospace. The current target is marked with the accent's deep tint and a bar.
fn place_row(ui: &mut egui::Ui, g: &crate::goals::Goal, far: &str, chosen: bool) -> egui::Response {
    const ROW: f32 = 26.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
    // The background goes under the icon and text, but its look depends on the hover,
    // known only once the row is made pressable (after them, so it is on top).
    let bg = ui.painter().add(egui::Shape::Noop);
    let inner = rect.shrink2(egui::vec2(8.0, 0.0));
    let mut icon =
        ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let icon = crate::ui::svg::tier(&mut icon, g.tier, 14.0).rect;
    let p = ui.painter();
    let far = p.layout_no_wrap(far.to_string(), egui::FontId::monospace(11.5), DIM);
    let far_w = far.size().x;
    p.galley(egui::pos2(inner.right() - far_w, rect.center().y - far.size().y / 2.0), far, DIM);
    let left = icon.right() + INLINE;
    let colour = if chosen { TITLE } else { TEXT };
    let mut job = egui::text::LayoutJob::single_section(
        g.label.clone(),
        egui::TextFormat::simple(egui::TextStyle::Body.resolve(ui.style()), colour),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((inner.right() - far_w - INLINE - left).max(0.0));
    let name = p.layout_job(job);
    p.galley(egui::pos2(left, rect.center().y - name.size().y / 2.0), name, colour);
    let r = ui.interact(rect, ui.id().with(("place", g.id)), egui::Sense::click());
    let fill = if chosen {
        ACCENT_DEEP
    } else if r.hovered() {
        CONTROL_HOVER
    } else {
        Color32::TRANSPARENT
    };
    let mut shapes = vec![egui::Shape::rect_filled(rect, R_CONTROL, fill)];
    if chosen {
        let bar = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 5.0), egui::vec2(2.0, ROW - 10.0));
        shapes.push(egui::Shape::rect_filled(bar, 1.0, ACCENT));
    }
    ui.painter().set(bg, egui::Shape::Vec(shapes));
    r.on_hover_text(&g.detail)
}
