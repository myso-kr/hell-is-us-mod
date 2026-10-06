//! The Filming page (film.rs in cheat/): a group of its own, not a cheat. Four cards, one concern
//! each — the take (start, stop, where it is, its key), the route (where it walks or flies, the
//! points and the routes kept, the pace), the camera (the lens, a flight, distance and field of
//! view) and the director's plan (the shot it gives each beat of the route as it stands).
//! The settings are shared with the worker (`Shared.film_setup`) and kept in `Mods\film.txt`.

use super::*;
use crate::film::{AerialPlan, Director, Lens, Mode, Setup, Source};
use crate::ui::theme::INLINE;

/// The director's plan, kept while the route and the take it was made for stay the same: each
/// stretch (cm along) and the name of how it is shot.
#[derive(Default)]
pub(super) struct PlanCache {
    key: u64,
    rows: Vec<(f32, f32, &'static str)>,
}

impl Panel {
    /// The page: the take and the route side by side with the camera, the plan across under them.
    pub(super) fn film_page(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let mut setup = self.shared.film_setup.lock().unwrap().clone();
        let before = setup.clone();
        let cols = self.columns;
        tw::spans(t, cols, &[1, 1, 1, 3], |t, i| match i {
            0 => self.film_take(t, snap, &mut setup),
            1 => self.film_route(t, &mut setup),
            2 => self.film_camera(t, &mut setup),
            _ => self.film_plan(t, snap, &setup),
        });
        if setup != before {
            setup.save();
            *self.shared.film_setup.lock().unwrap() = setup;
        }
    }

    /// The take: start and stop in the header, where it stands, the key, what stops it.
    fn film_take(&mut self, t: &mut Tui, snap: Option<&Snapshot>, setup: &mut Setup) {
        let state = self.shared.film.lock().unwrap().clone();
        let rolling = state.rolling();
        let may = self.grants(crate::settings::Consent::CHEATS) && snap.is_some_and(|s| s.gate.is_ok());
        let tx = self.tx.clone();
        tw::card_with(
            t,
            tr!("FILM_TAKE"),
            |ui| {
                if ui.add_enabled(rolling, egui::Button::new(tr!("FILM_STOP"))).clicked() {
                    let _ = tx.send(Request::Cut);
                }
                if ui.add_enabled(may && !rolling, egui::Button::new(tr!("FILM_START"))).clicked() {
                    let _ = tx.send(Request::Film(crate::film::COUNTDOWN_S));
                }
            },
            |t| {
                text(t, RichText::new(state.text()).strong().color(if rolling { OK } else { DIM }));
                choices(t, |t| {
                    for m in Mode::ALL {
                        if w(t, |ui| ui.radio(setup.mode == m, m.label())).clicked() && setup.mode != m {
                            setup.mode = m;
                            // a flight mostly looks the way it goes; circling has no subject there
                            if m == Mode::Flight && setup.lens == Lens::Orbit {
                                setup.lens = Lens::Follow;
                            }
                        }
                    }
                });
                note(t, setup.mode.about());
                let taken = self.shared.map.lock().unwrap().keys().to_vec();
                field(t, tr!("FILM_KEY"), |t| {
                    w(t, |ui| {
                        ui.scope(|ui| super::map::keycap_picker(ui, "film_key", &mut setup.key, &taken))
                            .response
                            .on_hover_text(tr!("FILM_KEY_HOVER"))
                    })
                });
            },
        );
    }

    /// The route: the guide's, or the points (taken in the game or on the 3D map) and the routes
    /// kept by name; the pace, back and forth.
    fn film_route(&mut self, t: &mut Tui, setup: &mut Setup) {
        card(t, tr!("FILM_ROUTE"), |t| {
            let n = setup.points.len();
            choices(t, |t| {
                for (src, label) in [
                    (Source::Guide, tr!("FILM_SOURCE_ROUTE").to_string()),
                    (Source::Points, trf!("FILM_SOURCE_POINTS", n = n)),
                    (Source::Recording, trf!("FILM_SOURCE_RECORDING", n = setup.recordings.len())),
                ] {
                    if w(t, |ui| ui.radio(setup.source == src, label)).clicked() {
                        setup.source = src;
                    }
                }
            });
            match setup.source {
                Source::Guide => {}
                Source::Points => {
                    if n == 0 {
                        note(t, trf!("FILM_POINTS_HOW", key = format!("Ctrl+F{}", setup.key)));
                    } else if w(t, |ui| ui.small_button(tr!("FILM_CLEAR_POINTS"))).clicked() {
                        setup.points.clear();
                    }
                    self.film_routes_kept(t, setup);
                }
                Source::Recording => self.film_recordings(t, setup),
            }
            field(t, tr!("FILM_PACE"), |t| tw::slider(t, &mut setup.pace, 0.2..=1.0, 0.05, ""));
            switch(t, &mut setup.repeat, tr!("FILM_REPEAT"));
        });
    }

    /// The recordings: each chosen to play, or removed; how to make one.
    fn film_recordings(&mut self, t: &mut Tui, setup: &mut Setup) {
        let recording = self.shared.film_recorder.lock().unwrap().as_ref().map(|r| r.len());
        match recording {
            Some(n) => text(t, RichText::new(trf!("FILM_RECORDING_NOW", n = n)).color(WAIT)),
            None => note(t, trf!("FILM_RECORD_HOW", key = format!("Ctrl+Shift+F{}", setup.key))),
        }
        let mut remove = None;
        for (k, r) in setup.recordings.iter().enumerate() {
            let (cm, secs) = r.length();
            t.style(tw::row(INLINE)).add(|t| {
                if w(t, |ui| ui.radio(setup.recording == k, r.name.as_str())).clicked() {
                    setup.recording = k;
                }
                tw::distance(t, format!("{:.0} m · {:.0} s", cm / 100.0, secs));
                if w(t, |ui| ui.small_button("×")).clicked() {
                    remove = Some(k);
                }
            });
        }
        if let Some(k) = remove {
            setup.recordings.remove(k);
            setup.recording = setup.recording.min(setup.recordings.len().saturating_sub(1));
        }
    }

    /// The routes kept by name: each loaded back or removed; the points now saved under a name.
    fn film_routes_kept(&mut self, t: &mut Tui, setup: &mut Setup) {
        let (mut load, mut remove) = (None, None);
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
        if setup.points.is_empty() {
            return;
        }
        t.style(tw::row(INLINE)).add(|t| {
            block(t, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.film_name)
                        .hint_text(tr!("FILM_ROUTE_NAME"))
                        .desired_width(f32::INFINITY),
                );
            });
            let name = self.film_name.trim().to_string();
            if w(t, |ui| ui.add_enabled(!name.is_empty(), egui::Button::new(tr!("SAVE")))).clicked() {
                setup.routes.retain(|(k, _)| *k != name);
                setup.routes.push((name, setup.points.clone()));
                self.film_name.clear();
            }
        });
    }

    /// The camera: the lens, distance and field of view (each a switch and its
    /// slider on one row).
    fn film_camera(&mut self, t: &mut Tui, setup: &mut Setup) {
        card(t, tr!("FILM_LENS"), |t| {
            choices(t, |t| {
                for l in Lens::ALL {
                    // circling turns about the pivot, which on a flight is on the path, not the hero
                    let off = setup.mode == Mode::Flight && l == Lens::Orbit;
                    let r = w(t, |ui| {
                        ui.add_enabled(!off, egui::RadioButton::new(setup.lens == l, l.label()))
                            .on_disabled_hover_text(tr!("FILM_ORBIT_NOT_IN_FLIGHT"))
                    });
                    if r.clicked() {
                        setup.lens = l;
                    }
                }
            });
            if setup.lens == Lens::Director {
                note(t, tr!("FILM_DIRECTOR_NOTE"));
            }
            optional(t, tr!("FILM_DISTANCE_CM"), &mut setup.distance, 800.0, crate::film::DISTANCE, 50.0, " cm");
            optional(t, tr!("FILM_FOV_DEG"), &mut setup.fov, 55.0, crate::film::FOV, 1.0, "°");
        });
    }

    /// The director's plan for the route as it stands: each beat's shot and where it plays.
    fn film_plan(&mut self, t: &mut Tui, snap: Option<&Snapshot>, setup: &Setup) {
        card(t, tr!("FILM_PLAN"), |t| {
            if setup.lens != Lens::Director {
                note(t, tr!("FILM_PLAN_OFF"));
                return;
            }
            if setup.mode == Mode::Live {
                note(t, tr!("FILM_PLAN_LIVE"));
            }
            let route = self.shared.route3d.lock().unwrap().0.clone();
            let flight = setup.mode == Mode::Flight;
            let path = snap.and_then(|s| {
                let (p, _) = s.pose?;
                let hero = [p[0] as f32, p[1] as f32, p[2] as f32];
                if flight {
                    // the camera starts about where the hero stands
                    let mut path = vec![[hero[0], hero[1], hero[2] + 160.0]];
                    path.extend(crate::film::flight_path(setup, &route)?);
                    Some(path)
                } else {
                    crate::film::path(setup, hero, &route, &s.nav)
                }
            });
            let Some((path, snap)) = path.zip(snap) else {
                note(t, tr!("FILM_NO_ROUTE"));
                return;
            };
            // the plan again only when the route has changed (not with every step of the hero)
            let key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                for p in path.iter().skip(1) {
                    p.map(|v| (v / 50.0) as i32).hash(&mut h);
                }
                flight.hash(&mut h);
                h.finish()
            };
            if self.film_plan.key != key {
                let blocking = snap.obstacles.blocking();
                let rows = if flight {
                    let path = crate::film::clear_flight(&path, &blocking);
                    AerialPlan::new(&path, &blocking)
                        .stretches()
                        .into_iter()
                        .map(|(a, b, x)| (a, b, x.label()))
                        .collect()
                } else {
                    Director::new(&path, &blocking).beats().iter().map(|b| (b.from, b.to, b.shot.label())).collect()
                };
                self.film_plan = PlanCache { key, rows };
            }
            tw::scroll_list(t, "film_plan", 260.0, |t| {
                for (i, &(from, to, label)) in self.film_plan.rows.iter().enumerate() {
                    let span = format!("{:.0}–{:.0} m", from / 100.0, to / 100.0);
                    t.style(tw::row(INLINE)).add(|t| {
                        block(t, |ui| {
                            ui.label(RichText::new(format!("{}. {label}", i + 1)));
                        });
                        w(t, |ui| ui.label(RichText::new(span).monospace().small().color(DIM)));
                    });
                }
            });
        });
    }
}

/// A setting that may be left as the game has it: a switch and, when on, its slider on the same
/// row (`on` the value it starts from).
#[allow(clippy::too_many_arguments)]
fn optional(
    t: &mut Tui,
    label: &str,
    value: &mut Option<f32>,
    on: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    suffix: &str,
) {
    // the switch, the name, and the slider taking the rest of the row (a field's label column
    // left it a stub)
    t.style(tw::row(INLINE)).add(|t| {
        let mut set = value.is_some();
        if w(t, |ui| toggle(ui, &mut set)).changed() {
            *value = set.then_some(on);
        }
        w(t, |ui| ui.label(label));
        match value.as_mut() {
            Some(v) => {
                tw::slider(t, v, range, step, suffix);
            }
            None => {
                w(t, |ui| ui.label(RichText::new(tr!("FILM_AS_THE_GAME")).color(DIM)));
            }
        }
    });
}
