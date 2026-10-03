//! The cheat pages: a group's cheats, held toggles, saved positions, and what is on.

use super::*;

impl Panel {
    /// A group's cheats as form rows: the name on the left, wrapping if it must; the
    /// switch, slider (track and value box measured apart), button and badge on the
    /// right, wrapping onto a second line rather than overflowing.
    pub(super) fn held(&mut self, t: &mut Tui, group: Group, snap: Option<&Snapshot>) {
        let mut changed = false;
        for c in CHEATS.iter().filter(|c| c.group == group) {
            let mut on = self.on.get(c.id).copied().unwrap_or(false);
            match c.kind {
                Kind::Toggle(_) => field(t, crate::i18n::tr(c.label), |t| {
                    changed |= w(t, |ui| toggle(ui, &mut on)).changed();
                    w(t, |ui| Self::badge(ui, c));
                }),
                Kind::Slider { min, max, .. } => field(t, crate::i18n::tr(c.label), |t| {
                    changed |= w(t, |ui| toggle(ui, &mut on)).changed();
                    let step = if max - min > 50.0 { 10.0 } else { 0.05 };
                    let v = self.value.entry(c.id).or_insert(min);
                    let mut slider = tw::slider(t, v, min..=max, step, "");
                    if let Some(now) = chosen(c).and_then(|a| snap.and_then(|s| s.value(a))) {
                        slider = slider.on_hover_text(trf!("THE_GAMES_VALUE_NOW", now = now));
                    }
                    // Live while dragging, at most every 150 ms — and always on release.
                    let due = self.slid.get(c.id).is_none_or(|x| x.elapsed() >= Duration::from_millis(150));
                    if on && ((slider.changed() && due) || slider.drag_stopped()) {
                        self.slid.insert(c.id, Instant::now());
                        changed = true;
                    }
                    w(t, |ui| Self::badge(ui, c));
                }),
                Kind::SetStock { max, default, .. } => {
                    // Written once, on the button — not held.
                    let tx = self.tx.clone();
                    field(t, crate::i18n::tr(c.label), |t| {
                        let v = self.value.entry(c.id).or_insert(default);
                        tw::slider(t, v, 1.0..=max, 1.0, "");
                        let v = *v;
                        if w(t, |ui| ui.button(tr!("APPLY"))).clicked() {
                            let _ = tx.send(Request::Set(c.id, v));
                        }
                        w(t, |ui| Self::badge(ui, c));
                    });
                    continue;
                }
                Kind::Set { .. } => continue,
            }
            self.on.insert(c.id, on);
        }
        if changed {
            self.send_active();
        }
    }

    pub(super) fn badge(ui: &mut egui::Ui, c: &Cheat) {
        if !c.verified {
            ui.label(RichText::new(tr!("UNVERIFIED")).color(WAIT).small())
                .on_hover_text(tr!("NOT_YET_VERIFIED_IN_THE_GAME"));
        }
    }

    /// Saved positions: save where the hero stands, go back to it.
    pub(super) fn positions(&self, t: &mut Tui, snap: Option<&Snapshot>) {
        card(t, tr!("SAVE_AND_LOAD_POSITIONS"), |t| {
            let world = snap.and_then(|s| s.world.clone());
            for i in 0..crate::engine::SLOTS {
                let slot = snap.and_then(|s| s.slots[i].clone());
                let here = slot.as_ref().is_some_and(|(w, _)| Some(w) == world.as_ref());
                field(t, trf!("SLOT", slot = i + 1), |t| {
                    if w(t, |ui| ui.button(tr!("SAVE"))).clicked() {
                        let _ = self.tx.send(Request::SavePosition(i));
                    }
                    if w(t, |ui| ui.add_enabled(here, egui::Button::new(tr!("LOAD_POSITION")))).clicked() {
                        let _ = self.tx.send(Request::LoadPosition(i));
                    }
                    let place = match &slot {
                        Some((_, p)) if here => format!("{:.0}, {:.0}, {:.0}", p[0], p[1], p[2]),
                        Some((w, _)) => trf!("ANOTHER_REGION", w = crate::i18n::place(w)),
                        None => tr!("EMPTY").into(),
                    };
                    note(t, place);
                });
            }
            note(t, tr!("MOVES_ONLY_WITHIN_THE_REGION_IT"));
        });
    }

    /// Every cheat that is on, across the groups, with its value.
    pub(super) fn summary(&self, t: &mut Tui, snap: Option<&Snapshot>) {
        let on: Vec<&Cheat> = CHEATS.iter().filter(|c| self.on.get(c.id).copied().unwrap_or(false)).collect();
        card(t, &trf!("CHEATS_ON_COUNT", count = on.len()), |t| {
            if on.is_empty() {
                text(t, RichText::new(tr!("NO_CHEATS_ARE_ON")).color(DIM));
            }
            for c in on {
                field(t, crate::i18n::tr(c.label), |t| {
                    let value = match c.kind {
                        Kind::Slider { .. } => {
                            let v = self.value.get(c.id).copied().unwrap_or(0.0);
                            match chosen(c).and_then(|a| snap.and_then(|s| s.value(a))) {
                                Some(n) => trf!("VALUE_AND_GAME", v = v, n = n),
                                None => format!("{v:.2}"),
                            }
                        }
                        _ => tr!("ON").to_string(),
                    };
                    w(t, |ui| ui.label(RichText::new(value).color(OK)));
                    w(t, |ui| ui.label(RichText::new(c.group.label()).color(DIM).small()));
                });
            }
        });
    }
}
