//! The cheat pages: a group's cheats, held toggles, saved positions, and what is on.

use super::*;
use crate::ui::theme::INLINE;

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
    /// A group's cheats, a tile each, two to a row: the switch and the name on top, the
    /// value under them (a slider, or a slider and its button), and the game's own value
    /// now at the foot, when it reads.
    pub(super) fn held(&mut self, t: &mut Tui, group: Group, snap: Option<&Snapshot>) {
        let mut changed = false;
        // A row's tiles as tall as each other: a slider's next to a switch's.
        let tiles = egui_taffy::taffy::Style {
            align_items: Some(egui_taffy::taffy::AlignItems::Stretch),
            ..tw::grid(2, INLINE)
        };
        t.style(tiles).add(|t| {
            for c in CHEATS.iter().filter(|c| c.group == group) {
                if matches!(c.kind, Kind::Set { .. }) {
                    continue;
                }
                let mut on = self.on.get(c.id).copied().unwrap_or(false);
                let now = chosen(c).and_then(|a| snap.and_then(|s| s.value(a)));
                super::guide::well(t, |t| {
                    t.style(tw::row(INLINE)).add(|t| {
                        if !matches!(c.kind, Kind::SetStock { .. }) {
                            changed |= w(t, |ui| toggle(ui, &mut on)).changed();
                        }
                        name(t, c, now);
                    });
                    match c.kind {
                        Kind::Slider { min, max, .. } => {
                            let step = if max - min > 50.0 { 10.0 } else { 0.05 };
                            let v = self.value.entry(c.id).or_insert(min);
                            let slider = tw::slider(t, v, min..=max, step, "");
                            // Live while dragging, at most every 150 ms, and always on release.
                            let due = self.slid.get(c.id).is_none_or(|x| x.elapsed() >= Duration::from_millis(150));
                            if on && ((slider.changed() && due) || slider.drag_stopped()) {
                                self.slid.insert(c.id, Instant::now());
                                changed = true;
                            }
                        }
                        // Written once, on the button, not held: no switch.
                        Kind::SetStock { max, default, .. } => {
                            t.style(tw::row(INLINE)).add(|t| {
                                let v = self.value.entry(c.id).or_insert(default);
                                tw::slider(t, v, 1.0..=max, 1.0, "");
                                let v = *v;
                                if w(t, |ui| ui.button(tr!("APPLY"))).clicked() {
                                    let _ = self.tx.send(Request::Set(c.id, v));
                                }
                            });
                        }
                        _ => {}
                    }
                    if let Some(now) = now {
                        note(t, trf!("THE_GAMES_VALUE_NOW", now = now));
                    }
                });
                if !matches!(c.kind, Kind::SetStock { .. }) {
                    self.on.insert(c.id, on);
                }
            }
        });
        if changed {
            self.send_active();
        }
    }

    /// Filming mode (film.rs): the hero walked along the guide's route or the 3D map's points
    /// by the stick's input, the camera as a drone's; started from here after a countdown, or
    /// by its key in the game; any key or mouse move stops it. The settings are the worker's
    /// too (`Shared.film_setup`), kept in `Mods\film.txt`.
    pub(super) fn film_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let state = self.shared.film.lock().unwrap().clone();
        let rolling = state.rolling();
        let may = self.grants(crate::settings::Consent::CHEATS) && snap.is_some_and(|s| s.gate.is_ok());
        let taken = self.shared.map.lock().unwrap().keys().to_vec();
        // the map's keys and these: chosen here or on the map settings' keys tab
        let mut setup = self.shared.film_setup.lock().unwrap().clone();
        let before = setup.clone();
        card(t, tr!("FILMING"), |t| {
            note(t, tr!("FILM_NOTE"));
            let n = setup.points.len();
            choices(t, |t| {
                if w(t, |ui| ui.radio(!setup.use_points, tr!("FILM_SOURCE_ROUTE"))).clicked() {
                    setup.use_points = false;
                }
                if w(t, |ui| ui.radio(setup.use_points, trf!("FILM_SOURCE_POINTS", n = n))).clicked() {
                    setup.use_points = true;
                }
                if n > 0 && w(t, |ui| ui.small_button(tr!("FILM_CLEAR_POINTS"))).clicked() {
                    setup.points.clear();
                }
            });
            if setup.use_points {
                if n == 0 {
                    note(t, trf!("FILM_POINTS_HOW", key = format!("Ctrl+F{}", setup.key)));
                }
                // routes kept by name: the points saved, loaded back, removed
                let mut load = None;
                let mut remove = None;
                for (k, (name, pts)) in setup.routes.iter().enumerate() {
                    t.style(tw::row(INLINE)).add(|t| {
                        block(t, |ui| {
                            ui.label(format!("{name}  ·  {}", trf!("FILM_N_POINTS", n = pts.len())));
                        });
                        if w(t, |ui| ui.small_button(tr!("LOAD_POSITION"))).clicked() {
                            load = Some(k);
                        }
                        if w(t, |ui| ui.small_button("×")).clicked() {
                            remove = Some(k);
                        }
                    });
                }
                if let Some(k) = load {
                    setup.points = setup.routes[k].1.clone();
                }
                if let Some(k) = remove {
                    setup.routes.remove(k);
                }
                if n > 0 {
                    t.style(tw::row(INLINE)).add(|t| {
                        w(t, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.film_name).hint_text(tr!("FILM_ROUTE_NAME")))
                        });
                        let name = self.film_name.trim().to_string();
                        if w(t, |ui| ui.add_enabled(!name.is_empty(), egui::Button::new(tr!("SAVE")))).clicked() {
                            setup.routes.retain(|(k, _)| *k != name);
                            setup.routes.push((name, setup.points.clone()));
                            self.film_name.clear();
                        }
                    });
                }
            }
            switch(t, &mut setup.flight, tr!("FILM_FLIGHT"));
            if setup.flight {
                note(t, tr!("FILM_FLIGHT_NOTE"));
            }
            field(t, tr!("FILM_PACE"), |t| tw::slider(t, &mut setup.pace, 0.2..=1.0, 0.05, ""));
            field(t, tr!("FILM_LENS"), |t| {
                choices(t, |t| {
                    for l in crate::film::Lens::ALL {
                        // circling turns about the pivot, which on a flight is on the path
                        let off = setup.flight && l == crate::film::Lens::Orbit;
                        let r = w(t, |ui| {
                            ui.add_enabled(!off, egui::RadioButton::new(setup.lens == l, l.label()))
                                .on_disabled_hover_text(tr!("FILM_ORBIT_NOT_IN_FLIGHT"))
                        });
                        if r.clicked() {
                            setup.lens = l;
                        }
                    }
                });
            });
            let mut far = setup.distance.is_some();
            switch(t, &mut far, tr!("FILM_DISTANCE"));
            match (far, setup.distance) {
                (true, None) => setup.distance = Some(800.0),
                (false, Some(_)) => setup.distance = None,
                _ => {}
            }
            if let Some(d) = setup.distance.as_mut() {
                field(t, tr!("FILM_DISTANCE_CM"), |t| tw::slider(t, d, crate::film::DISTANCE, 50.0, " cm"));
            }
            let mut zoom = setup.fov.is_some();
            switch(t, &mut zoom, tr!("FILM_FOV"));
            match (zoom, setup.fov) {
                (true, None) => setup.fov = Some(55.0),
                (false, Some(_)) => setup.fov = None,
                _ => {}
            }
            if let Some(f) = setup.fov.as_mut() {
                field(t, tr!("FILM_FOV_DEG"), |t| tw::slider(t, f, crate::film::FOV, 1.0, "°"));
            }
            switch(t, &mut setup.repeat, tr!("FILM_REPEAT"));
            field(t, tr!("FILM_KEY"), |t| w(t, |ui| super::map::keycap_picker(ui, "film_key", &mut setup.key, &taken)));
            choices(t, |t| {
                let start = w(t, |ui| ui.add_enabled(may && !rolling, egui::Button::new(tr!("FILM_START"))));
                if start.clicked() {
                    let _ = self.tx.send(Request::Film(crate::film::COUNTDOWN_S));
                }
                if w(t, |ui| ui.add_enabled(rolling, egui::Button::new(tr!("FILM_STOP")))).clicked() {
                    let _ = self.tx.send(Request::Cut);
                }
            });
            text(t, RichText::new(state.text()).color(if rolling { OK } else { DIM }));
        });
        if setup != before {
            setup.save();
            *self.shared.film_setup.lock().unwrap() = setup;
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

    /// What is followed (the auto guide's pick and every track) with a button each that
    /// moves the hero there: a step short of it, in this region only.
    /// Before each teleport the engine keeps where the hero stood; the header's button goes
    /// back there, shown from a teleport until it is used.
    pub(super) fn teleports(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let mut back = false;
        let went = self.went;
        let mut gone = false;
        tw::card_with(
            t,
            tr!("TELEPORT_TO_FOLLOWED"),
            |ui| {
                if went && ui.small_button(tr!("TELEPORT_BACK")).clicked() {
                    back = true;
                }
            },
            |t| {
                let Some(s) = snap else { return };
                let world = s.world.clone().unwrap_or_default();
                let here = s.pose.map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
                let state = self.shared.map.lock().unwrap();
                let mut goals: Vec<crate::goals::Goal> = s.goals.iter().cloned().collect();
                goals.extend(state.place_goals(&s.goals, &world));
                let rows: Vec<(String, [u8; 3], [f32; 3])> = state
                    .followed()
                    .into_iter()
                    .filter_map(|f| {
                        let g = goals.iter().find(|g| g.id == f.id)?;
                        let label = match f.track.and_then(|id| state.tracks.iter().find(|t| t.id == id)) {
                            Some(tr) => tr.shown(),
                            None => format!("{} · {}", tr!("GUIDE_AUTO_CARD"), g.label),
                        };
                        Some((label, f.colour.unwrap_or([235, 235, 235]), g.at))
                    })
                    .collect();
                drop(state);
                note(t, tr!("TELEPORT_TO_FOLLOWED_HINT"));
                if rows.is_empty() {
                    text(t, RichText::new(tr!("NOTHING_FOLLOWED_HERE")).color(DIM));
                }
                for (label, [r, g, b], at) in rows {
                    t.style(tw::row(INLINE)).add(|t| {
                        w(t, |ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 5.0, Color32::from_rgb(r, g, b));
                        });
                        block(t, |ui| ui.add(egui::Label::new(&label).truncate()));
                        if let Some(h) = here {
                            w(t, |ui| {
                                ui.label(RichText::new(crate::raster::span(h, at)).monospace().small().color(DIM))
                            });
                        }
                        if w(t, |ui| ui.button(tr!("TELEPORT"))).clicked() {
                            let _ = self.tx.send(Request::Teleport(world.clone(), at, label.clone()));
                            gone = true;
                        }
                    });
                }
            },
        );
        if gone {
            self.went = true;
        }
        if back {
            let _ = self.tx.send(Request::GoBack);
            self.went = false;
        }
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
