//! The map page: the minimap and big map settings, the relief, what the map shows, the pins, the keys.

use super::super::theme::INLINE;
use super::*;

/// The minimap's preview side in the panel (px).
const MINI_PREVIEW: f32 = 180.0;

/// A preview frame the overlay made, as a texture kept in `slot` (made once, then set
/// each new frame): its id and size.
fn texture(
    ctx: &egui::Context,
    slot: &mut Option<(u64, egui::TextureHandle)>,
    name: &str,
    frame: Option<(usize, usize, Vec<u32>, u64)>,
) -> Option<(egui::TextureId, usize, usize)> {
    let (w, h, px, n) = frame?;
    if slot.as_ref().map(|s| s.0) != Some(n) {
        let bytes: Vec<u8> =
            px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]).collect();
        let image = egui::ColorImage::from_rgba_premultiplied([w, h], &bytes);
        match slot.as_mut() {
            Some((at, tex)) => {
                tex.set(image, egui::TextureOptions::LINEAR);
                *at = n;
            }
            None => *slot = Some((n, ctx.load_texture(name, image, egui::TextureOptions::LINEAR))),
        }
    }
    slot.as_ref().map(|(_, t)| (t.id(), w, h))
}

/// The space a preview takes before the overlay has drawn one.
fn waiting(ui: &mut egui::Ui, size: egui::Vec2) {
    let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(r, super::super::theme::R_CONTROL, super::super::theme::CONTROL);
    ui.painter().text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        tr!("PREVIEW_NEEDS_THE_HERO"),
        egui::FontId::proportional(12.0),
        DIM,
    );
}

impl Panel {
    /// The previews' frames from the overlay, as textures: (minimap, big map), each its
    /// texture id and size.
    #[allow(clippy::type_complexity)]
    fn previews(
        &mut self,
        ctx: &egui::Context,
    ) -> (Option<(egui::TextureId, usize, usize)>, Option<(egui::TextureId, usize, usize)>) {
        let mini = self.shared.preview.lock().unwrap().as_ref().map(|(s, px, n)| (*s, *s, px.clone(), *n));
        let mini = texture(ctx, &mut self.preview_tex, "map-preview", mini);
        let big = self.shared.preview_big.lock().unwrap().clone();
        let big = texture(ctx, &mut self.preview_big_tex, "map-preview-big", big);
        (mini, big)
    }

    /// What each line and area on the maps is, as drawn with the settings now.
    pub(super) fn legend_card(&mut self, t: &mut Tui, state: &crate::minimap::MapState) {
        card(t, tr!("LEGEND"), |t| {
            let outline = state.mini_outline && state.big_outline;
            // What each line and colour is, as drawn now.
            let entries = crate::raster::legend(state, outline);
            t.style(tw::grid(2, INLINE)).add(|t| {
                for (swatch, c, name) in entries {
                    w(t, |ui| {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 10.0), egui::Sense::hover());
                            let colour = Color32::from_rgba_unmultiplied(c.0, c.1, c.2, c.3.max(160));
                            let p = ui.painter();
                            let y = r.center().y;
                            match swatch {
                                crate::raster::Swatch::Line => {
                                    p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], (2.0, colour));
                                }
                                crate::raster::Swatch::Dashed => {
                                    for k in 0..3 {
                                        let x = r.left() + k as f32 * 7.0;
                                        p.line_segment([egui::pos2(x, y), egui::pos2(x + 4.0, y)], (2.0, colour));
                                    }
                                }
                                crate::raster::Swatch::Fill => {
                                    p.rect_filled(r.shrink(1.0), 2.0, colour);
                                }
                            }
                            ui.label(RichText::new(name).small().color(DIM));
                        });
                    });
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
            Some(Tool::Settings) => self.settings_tab(t),
            // Where to go: the guide, the places, and the pins the player drops to go back to.
            Some(Tool::Guide) => tw::masonry(t, "guide", cols, 4, |t, i| match i {
                0 => self.auto_card(t, guard, snap),
                1 => self.tracks_card(t, guard, snap),
                2 => self.goals_card(t, guard, snap),
                _ => self.marks_column(t, guard, snap),
            }),
            // What is known: the quests' clues, a clue looked up, and who still has more to tell.
            // Who has more to tell is where they are: with "where hidden things are".
            // The quest's clues two columns wide, the search beside; who has more to tell
            // across the page under them.
            Some(Tool::Clues) => {
                let places = self.grants(crate::settings::Consent::PLACES);
                let spans: &[u16] = if places { &[2, 1, 3] } else { &[2, 1] };
                tw::spans(t, cols, spans, |t, i| match i {
                    0 => self.quest_clues_card(t, guard, snap),
                    1 => self.find_clue_card(t, snap),
                    _ => self.stories_card(t, guard, snap),
                })
            }
            Some(Tool::Puzzles) => {
                super::deep::puzzles_hero(t, snap);
                // This region's puzzles two columns wide, the locks beside; the choice
                // puzzles and the vaults across the page.
                tw::spans(t, cols, &[2, 1, 3, 3], |t, i| match i {
                    0 => self.puzzles_card(t, guard, snap),
                    1 => self.locks_card(t, guard, snap),
                    2 => self.slot_puzzles_card(t, guard, snap),
                    _ => self.vaults_card(t, guard, snap),
                })
            }
            Some(Tool::Collect) => {
                // Without "where hidden things are", the collection shows counts only.
                self.collect_hero(t, snap);
                // Three cards side by side; the achievements, a long list, across the page.
                tw::spans(t, cols, &[1, 1, 1, 3], |t, i| match i {
                    0 => self.collection_card(t, guard, snap),
                    1 => self.hollows_card(t, guard, snap),
                    2 => self.budget_card(t, snap),
                    _ => self.achievements_card(t),
                })
            }
            // The missable deeds' deadlines tell what the story does next: with answers.
            Some(Tool::Quests) => {
                // The journal two columns wide with the hand-overs beside it; the missable
                // deeds two wide with the side-story rings beside them.
                let answers = self.grants(crate::settings::Consent::ANSWERS);
                let order: &[(u16, u8)] =
                    if answers { &[(2, 0), (1, 2), (2, 1), (1, 3)] } else { &[(2, 0), (1, 2), (3, 3)] };
                let spans: Vec<u16> = order.iter().map(|(n, _)| *n).collect();
                tw::spans(t, cols, &spans, |t, i| match order[i].1 {
                    0 => self.quests_card(t, guard, snap),
                    1 => self.deadlines_card(t, guard, snap),
                    2 => self.handovers_card(t, guard, snap),
                    _ => self.secrets_card(t, snap),
                })
            }
            // How the maps look: nothing else.
            _ => tw::masonry(t, "map", cols, 3, |t, i| match i {
                0 => self.legend_card(t, guard),
                1 => self.map_column(t, guard),
                _ => self.keys_card(t, guard),
            }),
        }
        if *guard != before {
            guard.dirty = true;
        }
    }

    pub(super) fn map_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState) {
        let (mini, big) = self.previews(&t.egui_ctx().clone());
        card(t, tr!("MINIMAP"), |t| {
            // As it draws now (the overlay renders it while this page shows).
            w(t, |ui| {
                ui.vertical_centered(|ui| match mini {
                    Some((id, _, _)) => {
                        ui.add(egui::Image::new((id, egui::vec2(MINI_PREVIEW, MINI_PREVIEW))));
                    }
                    None => waiting(ui, egui::vec2(MINI_PREVIEW, MINI_PREVIEW)),
                })
            });
            field(t, tr!("DISPLAY"), |t| {
                for d in crate::minimap::Display::ALL {
                    w(t, |ui| ui.selectable_value(&mut state.display, d, d.label()));
                }
            });
            field(t, tr!("UP_IS"), |t| {
                w(t, |ui| ui.selectable_value(&mut state.heading_up, false, tr!("NORTH_N")));
                w(t, |ui| ui.selectable_value(&mut state.heading_up, true, tr!("CAMERA")));
            });
            field(t, tr!("RADIUS"), |t| {
                tw::slider(t, &mut state.radius_m, 20.0..=crate::minimap::RADIUS_MAX, 10.0, " m")
            });
            field(t, tr!("STYLE"), |t| {
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, true, tr!("OUTLINE")));
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, false, tr!("FILLED")));
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
            // As it covers the game window now: as wide as the card, the window's shape.
            w(t, |ui| {
                let width = ui.available_width();
                match big {
                    Some((id, w, h)) => {
                        let size = egui::vec2(width, width * h as f32 / w as f32);
                        let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        // The game is behind it in play: a dark ground stands in for it.
                        ui.painter().rect_filled(r, super::super::theme::R_CONTROL, super::super::theme::GROUND);
                        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        ui.painter().image(id, r, uv, Color32::WHITE);
                    }
                    None => waiting(ui, egui::vec2(width, width * 0.42)),
                }
            });
            field(t, tr!("RADIUS"), |t| {
                tw::slider(t, &mut state.big_radius_m, 50.0..=crate::minimap::RADIUS_MAX, 25.0, " m")
            });
            field(t, tr!("STYLE"), |t| {
                w(t, |ui| ui.selectable_value(&mut state.big_outline, true, tr!("OUTLINE")));
                w(t, |ui| ui.selectable_value(&mut state.big_outline, false, tr!("FILLED")));
            });
            field(t, tr!("OPACITY"), |t| tw::slider(t, &mut state.big_alpha, 20..=100, 1.0, " %"));
            // The big map's ground as dots: it covers the middle of the screen, so the
            // game shows through the gaps.
            field(t, tr!("DRAW_AS_DOTS"), |t| {
                w(t, |ui| toggle(ui, &mut state.dots)).on_hover_text(tr!("DRAW_AS_DOTS_HOVER"));
            });
        });

        // Each layer's opacity, for both maps, and presets that set the three at once.
        card(t, tr!("LAYER_OPACITY"), |t| {
            let presets: [(&str, [u8; 3]); 4] = [
                (tr!("PRESET_SOLID"), [100, 100, 100]),
                (tr!("PRESET_BALANCED"), [60, 90, 100]),
                (tr!("PRESET_SUBTLE"), [30, 65, 90]),
                (tr!("PRESET_ICONS_ONLY"), [0, 35, 100]),
            ];
            tw::choices(t, |t| {
                for (name, values) in presets {
                    if w(t, |ui| ui.selectable_label(state.opacity == values, name)).clicked() {
                        state.opacity = values;
                    }
                }
                if !presets.iter().any(|(_, v)| *v == state.opacity) {
                    tw::chip(t, tr!("PRESET_CUSTOM"), tw::Tone::Accent);
                }
            });
            for (label, i) in [(tr!("OPACITY_GROUND"), 0), (tr!("OPACITY_LINES"), 1), (tr!("OPACITY_ICONS"), 2)] {
                field(t, label, |t| tw::slider(t, &mut state.opacity[i], 0..=100, 5.0, " %"));
            }
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
                        if w(t, |ui| ui.selectable_label(state.is_followed(id), tr!("GUIDE"))).clicked() {
                            state.follow(crate::guide::track::Track {
                                id,
                                world: crate::survey::Survey::world_of(wd).to_string(),
                                at: m.at,
                                label: m.title(),
                                place: false,
                                ..Default::default()
                            });
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
