//! The cheat pages: a group's cheats, held toggles, saved positions, and what is on.

use super::*;
use crate::ui::theme::INLINE;
use egui_taffy::taffy::prelude::{auto, length};
use egui_taffy::taffy::{Size, Style};

/// How wide a cheat row's slider is (track and value box), so the names line up
/// beside it and the values sit at the row's right edge (px).
const SLIDER: f32 = 148.0;

/// `w-{px} shrink-0`: a node of a fixed width.
fn fixed(px: f32) -> Style {
    Style { size: Size { width: length(px), height: auto() }, flex_shrink: 0.0, ..Default::default() }
}

/// A cheat's name, growing to fill its row, with an "unverified" chip after it when
/// it has not been seen working. The game's own value, when there is one, is its hover.
fn name(t: &mut Tui, c: &Cheat, now: Option<f32>) {
    let full = crate::i18n::tr(c.label);
    let (short, aside) = split_label(full);
    block(t, |ui| {
        ui.horizontal_wrapped(|ui| {
            let mut r = ui.label(short);
            // A long aside ("999 = most, stops pick-ups") goes to the hover, so the row
            // keeps one line; a short one ("base 450") stays in the name.
            if aside {
                r = r.on_hover_text(full);
            }
            if let Some(now) = now {
                r.on_hover_text(trf!("THE_GAMES_VALUE_NOW", now = now));
            }
            if !c.verified {
                tw::pill(ui, tr!("UNVERIFIED"), tw::Tone::Wait).on_hover_text(tr!("NOT_YET_VERIFIED_IN_THE_GAME"));
            }
        });
    });
}

/// A cheat's label without a long parenthetical aside, and whether one was cut: an
/// aside of up to 12 characters (`(base 450)`) is part of the name.
fn split_label(label: &str) -> (&str, bool) {
    match label.find(" (").or_else(|| label.find('(')) {
        Some(at) if label[at..].trim().chars().count() > 14 => (label[..at].trim_end(), true),
        _ => (label, false),
    }
}

impl Panel {
    /// A group's cheats, one row each: the switch, the name (growing), and the value
    /// at the right edge (a slider, or a slider and its button). Toggles end at the name.
    pub(super) fn held(&mut self, t: &mut Tui, group: Group, snap: Option<&Snapshot>) {
        let mut changed = false;
        for c in CHEATS.iter().filter(|c| c.group == group) {
            if matches!(c.kind, Kind::Set { .. }) {
                continue;
            }
            let mut on = self.on.get(c.id).copied().unwrap_or(false);
            let now = chosen(c).and_then(|a| snap.and_then(|s| s.value(a)));
            t.style(tw::row(INLINE)).add(|t| match c.kind {
                Kind::Toggle(_) => {
                    changed |= w(t, |ui| toggle(ui, &mut on)).changed();
                    name(t, c, now);
                }
                Kind::Slider { min, max, .. } => {
                    changed |= w(t, |ui| toggle(ui, &mut on)).changed();
                    name(t, c, now);
                    let step = if max - min > 50.0 { 10.0 } else { 0.05 };
                    let v = self.value.entry(c.id).or_insert(min);
                    let mut slider = t.style(fixed(SLIDER)).add(|t| tw::slider(t, v, min..=max, step, ""));
                    if let Some(now) = now {
                        slider = slider.on_hover_text(trf!("THE_GAMES_VALUE_NOW", now = now));
                    }
                    // Live while dragging, at most every 150 ms, and always on release.
                    let due = self.slid.get(c.id).is_none_or(|x| x.elapsed() >= Duration::from_millis(150));
                    if on && ((slider.changed() && due) || slider.drag_stopped()) {
                        self.slid.insert(c.id, Instant::now());
                        changed = true;
                    }
                }
                Kind::SetStock { max, default, .. } => {
                    // Written once, on the button, not held: no switch, but its room kept
                    // so the name starts where the others do.
                    w(t, |ui| {
                        let size = ui.text_style_height(&egui::TextStyle::Body) * egui::vec2(2.0, 1.0);
                        ui.allocate_exact_size(size, egui::Sense::hover());
                    });
                    name(t, c, now);
                    let v = self.value.entry(c.id).or_insert(default);
                    t.style(fixed(SLIDER)).add(|t| tw::slider(t, v, 1.0..=max, 1.0, ""));
                    let v = *v;
                    if w(t, |ui| ui.button(tr!("APPLY"))).clicked() {
                        let _ = self.tx.send(Request::Set(c.id, v));
                    }
                }
                Kind::Set { .. } => {}
            });
            if !matches!(c.kind, Kind::SetStock { .. }) {
                self.on.insert(c.id, on);
            }
        }
        if changed {
            self.send_active();
        }
    }

    /// Saved positions: save where the hero stands, go back to it. One row a slot: its
    /// name over where it is (dim), the buttons at the right edge.
    pub(super) fn positions(&self, t: &mut Tui, snap: Option<&Snapshot>) {
        card(t, tr!("SAVE_AND_LOAD_POSITIONS"), |t| {
            let world = snap.and_then(|s| s.world.clone());
            for i in 0..crate::engine::SLOTS {
                let slot = snap.and_then(|s| s.slots[i].clone());
                let here = slot.as_ref().is_some_and(|(w, _)| Some(w) == world.as_ref());
                let place = match &slot {
                    Some((_, p)) if here => format!("{:.0}, {:.0}, {:.0}", p[0], p[1], p[2]),
                    Some((w, _)) => trf!("ANOTHER_REGION", w = crate::i18n::place(w)),
                    None => tr!("EMPTY").into(),
                };
                t.style(tw::row(INLINE)).add(|t| {
                    block(t, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(trf!("SLOT", slot = i + 1));
                        ui.add(egui::Label::new(RichText::new(place).color(DIM).small()).wrap());
                    });
                    if w(t, |ui| ui.button(tr!("SAVE"))).clicked() {
                        let _ = self.tx.send(Request::SavePosition(i));
                    }
                    let load = w(t, |ui| {
                        ui.add_enabled(here, egui::Button::new(tr!("LOAD_POSITION")))
                            .on_hover_text(tr!("MOVES_ONLY_WITHIN_THE_REGION_IT"))
                            .on_disabled_hover_text(tr!("MOVES_ONLY_WITHIN_THE_REGION_IT"))
                    });
                    if load.clicked() {
                        let _ = self.tx.send(Request::LoadPosition(i));
                    }
                });
            }
        });
    }

    /// Every cheat that is on, across the groups: the name and its group (dim), and the
    /// value as a chip at the right edge; the game's own value is the chip's hover.
    pub(super) fn summary(&self, t: &mut Tui, snap: Option<&Snapshot>) {
        let on: Vec<&Cheat> = CHEATS.iter().filter(|c| self.on.get(c.id).copied().unwrap_or(false)).collect();
        card(t, &trf!("CHEATS_ON_COUNT", count = on.len()), |t| {
            if on.is_empty() {
                text(t, RichText::new(tr!("NO_CHEATS_ARE_ON")).color(DIM));
            }
            for c in on {
                t.style(tw::row(INLINE)).add(|t| {
                    block(t, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(crate::i18n::tr(c.label));
                            ui.label(RichText::new(c.group.label()).color(DIM).small());
                        });
                    });
                    let (value, game) = match c.kind {
                        Kind::Slider { .. } => {
                            let v = self.value.get(c.id).copied().unwrap_or(0.0);
                            (format!("{v:.2}"), chosen(c).and_then(|a| snap.and_then(|s| s.value(a))))
                        }
                        _ => (tr!("ON").to_string(), None),
                    };
                    w(t, |ui| {
                        let r = tw::pill(ui, value, tw::Tone::Ok);
                        if let Some(now) = game {
                            r.on_hover_text(trf!("THE_GAMES_VALUE_NOW", now = now));
                        }
                    });
                });
            }
        });
    }
}
