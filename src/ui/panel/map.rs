//! The map page: the minimap and big map settings, the relief, what the map shows, the pins, the keys.

use super::*;

impl Panel {
    /// The map & guide page. Left column: the minimap, the land, the big map, what is
    /// shown, this area. Right: compass and north, the guide and where to go, every
    /// key, the game's menus.
    pub(super) fn map_tab(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let shared = self.shared.clone();
        let mut guard = shared.map.lock().unwrap();
        let before = guard.clone();
        // Two columns of cards per page.
        t.style(tw::full(tw::cards(tw::CARD_MIN))).add(|t| match self.tool {
            Some(Tool::Guide) => {
                t.style(tw::col(tw::GAP)).add(|t| self.guide_column(t, &mut guard, snap));
                t.style(tw::col(tw::GAP)).add(|t| self.goals_card(t, &mut guard, snap));
            }
            Some(Tool::Collect) => {
                t.style(tw::col(tw::GAP)).add(|t| self.collection_card(t, &mut guard, snap));
                t.style(tw::col(tw::GAP)).add(|t| {
                    self.secrets_card(t, snap);
                    self.stories_card(t, &mut guard, snap);
                });
            }
            Some(Tool::Quests) => {
                t.style(tw::col(tw::GAP)).add(|t| self.quests_card(t, &mut guard, snap));
                t.style(tw::col(tw::GAP)).add(|t| {
                    self.deadlines_card(t, &mut guard, snap);
                    self.handovers_card(t, &mut guard, snap);
                });
            }
            _ => {
                t.style(tw::col(tw::GAP)).add(|t| {
                    self.map_column(t, &mut guard);
                    self.keys_card(t, &mut guard);
                });
                t.style(tw::col(tw::GAP)).add(|t| self.marks_column(t, &mut guard, snap));
            }
        });
        if *guard != before {
            guard.dirty = true;
        }
    }

    pub(super) fn map_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState) {
        card(t, "미니맵", |t| {
            field(t, "표시 방식", |t| {
                for d in crate::minimap::Display::ALL {
                    w(t, |ui| ui.selectable_value(&mut state.display, d, d.label()));
                }
            });
            field(t, "위쪽", |t| {
                w(t, |ui| ui.selectable_value(&mut state.heading_up, false, "북쪽(N)"));
                w(t, |ui| ui.selectable_value(&mut state.heading_up, true, "카메라 방향"));
            });
            field(t, "반경", |t| tw::slider(t, &mut state.radius_m, 20.0..=300.0, 10.0, " m"));
            field(t, "스타일", |t| {
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, true, "윤곽선"));
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, false, "채움"));
            });
            field(t, "아이콘 크기", |t| tw::slider(t, &mut state.icon_px, crate::minimap::ICON_PX, 1.0, " px"));
            switch(t, &mut state.hide_in_menus, "인벤토리·메뉴가 열리면 모든 오버레이 숨기기");
        });

        card(t, "지형", |t| {
            field(t, "지형 표시", |t| {
                for m in crate::minimap::ReliefMode::ALL {
                    w(t, |ui| ui.selectable_value(&mut state.relief, m, m.label()));
                }
            });
            field(t, "벽·바닥 윤곽", |t| w(t, |ui| toggle(ui, &mut state.terrain)));
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

        card(t, "큰 지도", |t| {
            field(t, "반경", |t| tw::slider(t, &mut state.big_radius_m, 50.0..=1000.0, 25.0, " m"));
            field(t, "스타일", |t| {
                w(t, |ui| ui.selectable_value(&mut state.big_outline, true, "윤곽선"));
                w(t, |ui| ui.selectable_value(&mut state.big_outline, false, "채움"));
            });
            field(t, "불투명도", |t| tw::slider(t, &mut state.big_alpha, 20..=100, 1.0, " %"));
        });

    }

    /// What the map shows, and the pins: the map page's second column.
    pub(super) fn marks_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let world = snap.and_then(|s| s.world.clone());
        let near = snap.map(|s| s.things.clone()).unwrap_or_default();
        card(t, "표시할 것", |t| {
            for (i, k) in ThingKind::ALL.into_iter().enumerate() {
                let [r, g, b] = k.rgb();
                let colour = Color32::from_rgb(r, g, b);
                let total = near.iter().filter(|x| x.kind() == k).count();
                let subs = Sub::ALL.iter().filter(|x| x.kind() == k).count();
                let mut on = state.layers & k.bit() != 0;
                t.style(tw::row(8.0)).add(|t| {
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
                    t.style(tw::grow(tw::row(6.0))).add(|t| {
                        w(t, |ui| {
                            ui.label(RichText::new(k.label()).strong().color(if on { Color32::WHITE } else { DIM }))
                        });
                        w(t, |ui| ui.label(RichText::new(format!("({total})")).color(DIM)));
                    });
                    if subs > 1 {
                        let hidden = Sub::ALL.iter().filter(|x| x.kind() == k && state.hidden.contains(x)).count();
                        let label = match (self.unfolded[i], hidden) {
                            (true, _) => "세부 ▲".to_string(),
                            (false, 0) => "세부 ▼".to_string(),
                            (false, h) => format!("세부 ▼ ({h}개 숨김)"),
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
                            if w(t, |ui| ui.add_enabled(on, chip(&format!("{} ({n})", sub.label()), shown, colour)))
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

        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        card(t, "지도 핀 · 지나온 길", |t| match &world {
            Some(wd) => {
                // The kind the marker key gives the next pin.
                field(t, format!("새 핀 (F{})", state.marker_key), |t| {
                    w(t, |ui| {
                        if pin_picker(ui, "new-pin", &mut state.pin_kind) {
                            state.dirty = true;
                        }
                    });
                });
                let count = state.markers.get(wd).map_or(0, Vec::len);
                if count == 0 {
                    note(t, format!("이 지역에 핀이 없습니다 — 잠긴 문·퍼즐 앞에서 F{} 를 누르면 꽂힙니다", state.marker_key));
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
                    let far = here.map_or(String::new(), |h| crate::raster::distance((m.at[0] - h[0]).hypot(m.at[1] - h[1]) / 100.0));
                    let mut note_text = m.note.clone();
                    let mut kind = m.kind;
                    t.style(tw::row(6.0)).add(|t| {
                        w(t, |ui| pin_picker(ui, &format!("pin-{i}"), &mut kind));
                        block(t, |ui| ui.add(egui::TextEdit::singleline(&mut note_text).hint_text("메모").desired_width(f32::INFINITY)));
                        w(t, |ui| ui.label(RichText::new(far).color(DIM).small()));
                        if w(t, |ui| ui.selectable_label(state.target == Some(id), "안내")).clicked() {
                            state.target = Some(id);
                            state.chosen = true;
                            state.route = true;
                        }
                        if w(t, |ui| ui.small_button("×")).on_hover_text("이 핀 지우기").clicked() {
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
                field(t, "이 지역", |t| text(t, RichText::new(format!("지나온 길 {trail}점 · 핀 {count}개")).color(DIM)));
                choices(t, |t| {
                    if w(t, |ui| ui.button("경로 지우기")).clicked() {
                        state.clear_trail(wd);
                    }
                    if w(t, |ui| ui.button("핀 모두 지우기")).clicked() {
                        state.clear_markers(wd);
                    }
                });
            }
            _ => text(t, RichText::new("주인공을 조작할 수 있을 때 표시됩니다").color(DIM)),
        });
    }

    /// The overlay's keys.
    pub(super) fn keys_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState) {
        card(t, "단축키", |t| {
            for (label, id) in [
                ("지도 표시 방식 전환", "toggle_key"),
                ("마커 찍기/지우기", "marker_key"),
                ("나침반 표시/숨김", "compass_key"),
                ("다음 목표로 안내", "cycle_key"),
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
                field(t, label, |t| w(t, |ui| key_picker(ui, id, key, &taken)));
            }
        });
    }
}
