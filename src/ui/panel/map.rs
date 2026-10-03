//! The map page: the minimap and big map settings, the relief, what the map shows, the pins, the keys.

use super::super::theme::INLINE;
use super::*;

/// The preview's side in the panel (px): the minimap's own size.
const PREVIEW: f32 = 240.0;

impl Panel {
    /// The map as the settings below draw it now (the overlay renders it four times a
    /// second while this page shows): the minimap's, or the big map's when that is the
    /// display; what it shows and how, in chips under it.
    pub(super) fn preview_card(&mut self, t: &mut Tui, state: &crate::minimap::MapState) {
        let fresh = self.shared.preview.lock().unwrap().as_ref().map(|h| h.2);
        if fresh.is_some() && fresh != self.preview_tex.as_ref().map(|h| h.0) {
            if let Some((side, px, n)) = self.shared.preview.lock().unwrap().clone() {
                let bytes: Vec<u8> =
                    px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]).collect();
                let image = egui::ColorImage::from_rgba_premultiplied([side, side], &bytes);
                match self.preview_tex.as_mut() {
                    Some((at, tex)) => {
                        tex.set(image, egui::TextureOptions::LINEAR);
                        *at = n;
                    }
                    None => {
                        let tex = t.egui_ctx().load_texture("map-preview", image, egui::TextureOptions::LINEAR);
                        self.preview_tex = Some((n, tex));
                    }
                }
            }
        }
        let tex = self.preview_tex.as_ref().map(|h| h.1.id());
        let big = state.display == crate::minimap::Display::Big;
        card(t, tr!("PREVIEW"), |t| {
            w(t, |ui| {
                ui.vertical_centered(|ui| match tex {
                    Some(id) => {
                        ui.add(egui::Image::new((id, egui::vec2(PREVIEW, PREVIEW))));
                    }
                    None => {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(PREVIEW, PREVIEW), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), PREVIEW / 2.0 - 12.0, super::super::theme::CONTROL);
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            tr!("PREVIEW_NEEDS_THE_HERO"),
                            egui::FontId::proportional(12.0),
                            DIM,
                        );
                    }
                })
            });
            // What is previewed, as chips: which map, how far, which way up, the style.
            let (radius, outline) =
                if big { (state.big_radius_m, state.big_outline) } else { (state.radius_m, state.mini_outline) };
            tw::choices(t, |t| {
                tw::chip(t, if big { tr!("BIG_MAP") } else { tr!("MINIMAP") }, tw::Tone::Accent);
                tw::chip(t, format!("{radius:.0} m"), tw::Tone::Quiet);
                let up = if state.heading_up && !big { tr!("CAMERA") } else { tr!("NORTH_N") };
                tw::chip(t, up, tw::Tone::Quiet);
                tw::chip(t, if outline { tr!("OUTLINE") } else { tr!("FILLED") }, tw::Tone::Quiet);
                if big {
                    tw::chip(t, format!("{}%", state.big_alpha), tw::Tone::Quiet);
                }
            });
        });
    }

    /// The map & guide page. Left column: the minimap, the land, the big map, what is
    /// shown, this area. Right: compass and north, the guide and where to go, every
    /// key, the game's menus.
    pub(super) fn map_tab(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let shared = self.shared.clone();
        let mut guard = shared.map.lock().unwrap();
        let before = guard.clone();
        // The page's cards in masonry columns (tw::masonry), as many as fit.
        let cols = self.columns;
        let guard = &mut *guard;
        match self.tool {
            Some(Tool::Now) => self.now_tab(t, guard, snap),
            Some(Tool::Help) => self.help_tab(t, guard),
            Some(Tool::Guide) => tw::masonry(t, "guide", cols, 2, |t, i| match i {
                0 => self.guide_column(t, guard, snap),
                _ => self.goals_card(t, guard, snap),
            }),
            Some(Tool::Clues) => tw::masonry(t, "clues", cols, 2, |t, i| match i {
                0 => self.quest_clues_card(t, guard, snap),
                _ => self.find_clue_card(t, snap),
            }),
            Some(Tool::Puzzles) => {
                super::deep::puzzles_hero(t, snap);
                tw::masonry(t, "puzzles", cols, 4, |t, i| match i {
                    0 => self.puzzles_card(t, guard, snap),
                    1 => self.locks_card(t, guard, snap),
                    2 => self.vaults_card(t, guard, snap),
                    _ => self.catalogue_card(t, guard, snap),
                })
            }
            Some(Tool::Collect) => {
                self.collect_hero(t, snap);
                tw::masonry(t, "collect", cols, 4, |t, i| match i {
                    0 => self.collection_card(t, guard, snap),
                    1 => self.hollows_card(t, guard, snap),
                    2 => self.stories_card(t, guard, snap),
                    _ => self.achievements_card(t, snap),
                })
            }
            Some(Tool::Quests) => tw::masonry(t, "quests", cols, 4, |t, i| match i {
                0 => self.quests_card(t, guard, snap),
                1 => self.deadlines_card(t, guard, snap),
                2 => self.handovers_card(t, guard, snap),
                _ => self.secrets_card(t, snap),
            }),
            _ => tw::masonry(t, "map", cols, 4, |t, i| match i {
                0 => self.preview_card(t, guard),
                1 => self.map_column(t, guard),
                2 => self.marks_column(t, guard, snap),
                _ => self.keys_card(t, guard),
            }),
        }
        if *guard != before {
            guard.dirty = true;
        }
    }

    pub(super) fn map_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState) {
        card(t, tr!("MINIMAP"), |t| {
            field(t, tr!("DISPLAY"), |t| {
                for d in crate::minimap::Display::ALL {
                    w(t, |ui| ui.selectable_value(&mut state.display, d, d.label()));
                }
            });
            field(t, tr!("UP_IS"), |t| {
                w(t, |ui| ui.selectable_value(&mut state.heading_up, false, tr!("NORTH_N")));
                w(t, |ui| ui.selectable_value(&mut state.heading_up, true, tr!("CAMERA")));
            });
            field(t, tr!("RADIUS"), |t| tw::slider(t, &mut state.radius_m, 20.0..=300.0, 10.0, " m"));
            field(t, tr!("STYLE"), |t| {
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, true, tr!("OUTLINE")));
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, false, tr!("FILLED")));
            });
            // The ground as dots (both maps): the game shows through the gaps.
            field(t, tr!("DRAW_AS_DOTS"), |t| {
                w(t, |ui| toggle(ui, &mut state.dots)).on_hover_text(tr!("DRAW_AS_DOTS_HOVER"));
            });
            field(t, tr!("ICON_SIZE"), |t| tw::slider(t, &mut state.icon_px, crate::minimap::ICON_PX, 1.0, " px"));
            // A short label; the full sentence is the hover text.
            field(t, tr!("HIDE_IN_MENUS"), |t| {
                w(t, |ui| toggle(ui, &mut state.hide_in_menus))
                    .on_hover_text(tr!("HIDE_EVERY_OVERLAY_WHILE_THE_INVENTORY"));
            });
        });

        card(t, tr!("TERRAIN"), |t| {
            field(t, tr!("TERRAIN_VIEW"), |t| {
                for m in crate::minimap::ReliefMode::ALL {
                    w(t, |ui| ui.selectable_value(&mut state.relief, m, m.label()));
                }
            });
            field(t, tr!("WALL_AND_FLOOR_OUTLINES"), |t| w(t, |ui| toggle(ui, &mut state.terrain)));
            if state.terrain {
                choices(t, |t| {
                    for b in crate::raster::Band::ALL {
                        w(t, |ui| {
                            ui.horizontal(|ui| {
                                let (fill, edge) = b.colours();
                                let swatch = Color32::from_rgb(
                                    fill.0.max(edge.0 / 2),
                                    fill.1.max(edge.1 / 2),
                                    fill.2.max(edge.2 / 2),
                                );
                                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                ui.painter().rect_filled(r, 2.0, swatch);
                                ui.painter().rect_stroke(
                                    r,
                                    2.0,
                                    egui::Stroke::new(1.0, Color32::from_rgb(edge.0, edge.1, edge.2)),
                                    egui::StrokeKind::Inside,
                                );
                                ui.label(RichText::new(b.label()).small());
                            });
                        });
                    }
                });
            }
        });

        card(t, tr!("BIG_MAP"), |t| {
            field(t, tr!("RADIUS"), |t| tw::slider(t, &mut state.big_radius_m, 50.0..=1000.0, 25.0, " m"));
            field(t, tr!("STYLE"), |t| {
                w(t, |ui| ui.selectable_value(&mut state.big_outline, true, tr!("OUTLINE")));
                w(t, |ui| ui.selectable_value(&mut state.big_outline, false, tr!("FILLED")));
            });
            field(t, tr!("OPACITY"), |t| tw::slider(t, &mut state.big_alpha, 20..=100, 1.0, " %"));
        });
    }

    /// What the map shows, and the pins: the map page's second column.
    pub(super) fn marks_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let world = snap.and_then(|s| s.world.clone());
        let near = snap.map(|s| s.things.clone()).unwrap_or_default();
        card(t, tr!("SHOW_ON_MAP"), |t| {
            for (i, k) in ThingKind::ALL.into_iter().enumerate() {
                let [r, g, b] = k.rgb();
                let colour = Color32::from_rgb(r, g, b);
                let total = near.iter().filter(|x| x.kind() == k).count();
                let subs = Sub::ALL.iter().filter(|x| x.kind() == k).count();
                let mut on = state.layers & k.bit() != 0;
                t.style(tw::row(INLINE)).add(|t| {
                    if w(t, |ui| toggle(ui, &mut on)).changed() {
                        state.layers ^= k.bit();
                    }
                    w(t, |ui| {
                        let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            dot.center(),
                            6.0,
                            if on { colour } else { colour.gamma_multiply(0.3) },
                        );
                    });
                    w(t, |ui| crate::ui::svg::kind(ui, k, 18.0));
                    // The name takes the rest of the row, so every count lines up at its end.
                    let name = RichText::new(k.label()).color(if on { super::super::theme::TEXT } else { DIM });
                    text(t, name);
                    w(t, |ui| ui.label(RichText::new(total.to_string()).monospace().size(11.5).color(DIM)));
                    if subs > 1 {
                        let hidden = Sub::ALL.iter().filter(|x| x.kind() == k && state.hidden.contains(x)).count();
                        let label = match (self.unfolded[i], hidden) {
                            (true, _) => tr!("DETAILS_OPEN").to_string(),
                            (false, 0) => tr!("DETAILS_CLOSED").to_string(),
                            (false, h) => trf!("DETAILS_HIDDEN", h = h),
                        };
                        if w(t, |ui| ui.add(egui::Button::new(RichText::new(label).small()).frame(false))).clicked() {
                            self.unfolded[i] = !self.unfolded[i];
                        }
                    }
                });
                if self.unfolded[i] && subs > 1 {
                    choices(t, |t| {
                        for sub in Sub::ALL.into_iter().filter(|x| x.kind() == k) {
                            let n = near.iter().filter(|x| x.sub == sub).count();
                            let shown = !state.hidden.contains(&sub);
                            if w(t, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    crate::ui::svg::sort(ui, sub, 16.0);
                                    ui.add_enabled(on, chip(&format!("{} ({n})", sub.label()), shown, colour))
                                })
                                .inner
                            })
                            .clicked()
                            {
                                if shown {
                                    state.hidden.insert(sub);
                                } else {
                                    state.hidden.remove(&sub);
                                }
                            }
                        }
                    });
                }
            }
        });

        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        card(t, tr!("MAP_PINS_AND_TRAIL"), |t| match &world {
            Some(wd) => {
                // The kind the marker key gives the next pin.
                field(t, trf!("NEW_PIN_F", key = state.marker_key), |t| {
                    w(t, |ui| {
                        if pin_picker(ui, "new-pin", &mut state.pin_kind) {
                            state.dirty = true;
                        }
                    });
                });
                let count = state.markers.get(wd).map_or(0, Vec::len);
                if count == 0 {
                    note(t, trf!("NO_PINS_IN_THIS_REGION_PRESS", key = state.marker_key));
                }
                // Nearest first; each: its kind (press to change), a note, guide, remove.
                let mut order: Vec<usize> = (0..count).collect();
                if let (Some(h), Some(list)) = (here, state.markers.get(wd)) {
                    let d = |i: &usize| (list[*i].at[0] - h[0]).hypot(list[*i].at[1] - h[1]);
                    order.sort_by(|a, b| d(a).total_cmp(&d(b)));
                }
                let mut remove = None;
                for i in order {
                    let Some(m) = state.markers.get(wd).and_then(|l| l.get(i)).cloned() else { continue };
                    let id = m.id(wd);
                    let far = here.map_or(String::new(), |h| crate::raster::span(h, m.at));
                    let mut note_text = m.note.clone();
                    let mut kind = m.kind;
                    t.style(tw::row(INLINE)).add(|t| {
                        w(t, |ui| pin_picker(ui, &format!("pin-{i}"), &mut kind));
                        block(t, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut note_text)
                                    .hint_text(tr!("NOTE"))
                                    .desired_width(f32::INFINITY),
                            )
                        });
                        w(t, |ui| ui.label(RichText::new(far).monospace().size(11.5).color(DIM)));
                        if w(t, |ui| ui.selectable_label(state.target == Some(id), tr!("GUIDE"))).clicked() {
                            state.target = Some(id);
                            state.chosen = true;
                            state.route = true;
                        }
                        if w(t, |ui| ui.small_button("×")).on_hover_text(tr!("REMOVE_THIS_PIN")).clicked() {
                            remove = Some(i);
                        }
                    });
                    if kind != m.kind || note_text != m.note {
                        if let Some(p) = state.markers.get_mut(wd).and_then(|l| l.get_mut(i)) {
                            p.kind = kind;
                            p.note = note_text;
                        }
                        state.dirty = true;
                    }
                }
                if let Some(i) = remove {
                    if let Some(l) = state.markers.get_mut(wd) {
                        l.remove(i);
                    }
                    state.dirty = true;
                }
                let trail = state.trails.get(wd).map_or(0, |x| x.iter().flatten().count());
                field(t, tr!("THIS_REGION"), |t| {
                    text(t, RichText::new(trf!("TRAIL_POINTS_PINS", trail = trail, count = count)).color(DIM))
                });
                choices(t, |t| {
                    if w(t, |ui| ui.button(tr!("CLEAR_TRAIL"))).clicked() {
                        state.clear_trail(wd);
                    }
                    if w(t, |ui| ui.button(tr!("REMOVE_ALL_PINS"))).clicked() {
                        state.clear_markers(wd);
                    }
                });
            }
            _ => note(t, tr!("SHOWN_WHILE_THE_HERO_CAN_BE")),
        });
    }

    /// The overlay's keys.
    pub(super) fn keys_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState) {
        card(t, tr!("KEYS"), |t| {
            for (label, id) in [
                (tr!("SWITCH_MAP_DISPLAY"), "toggle_key"),
                (tr!("PLACE_OR_REMOVE_A_MARKER"), "marker_key"),
                (tr!("SHOW_OR_HIDE_THE_COMPASS"), "compass_key"),
                (tr!("GUIDE_TO_THE_NEXT_GOAL"), "cycle_key"),
            ] {
                let mine = match id {
                    "toggle_key" => state.toggle_key,
                    "marker_key" => state.marker_key,
                    "compass_key" => state.compass_key,
                    _ => state.cycle_key,
                };
                let taken = others(state, mine);
                let key = match id {
                    "toggle_key" => &mut state.toggle_key,
                    "marker_key" => &mut state.marker_key,
                    "compass_key" => &mut state.compass_key,
                    _ => &mut state.cycle_key,
                };
                field(t, label, |t| w(t, |ui| keycap_picker(ui, id, key, &taken)));
            }
        });
    }
}

/// `key_picker` drawn as a keycap: the same choices and behaviour, the key in
/// monospace on the control fill.
fn keycap_picker(ui: &mut egui::Ui, id: &str, key: &mut u8, taken: &[u8]) {
    ui.scope(|ui| {
        ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
        ui.visuals_mut().widgets.inactive.weak_bg_fill = super::super::theme::CONTROL;
        key_picker(ui, id, key, taken);
    });
}
