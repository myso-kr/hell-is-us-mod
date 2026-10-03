//! The guide page: the compass, the guide's target and why, every place with something new.

use super::*;

impl Panel {
    pub(super) fn guide_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let dist = |g: &crate::goals::Goal| {
            here.map_or(f32::MAX, |h| ((g.at[0] - h[0]).powi(2) + (g.at[1] - h[1]).powi(2)).sqrt() / 100.0)
        };

        card(t, tr!("나침반 · 방향"), |t| {
            field(t, trf!("나침반 (F{a0})", a0 = state.compass_key), |t| {
                w(t, |ui| toggle(ui, &mut state.compass));
                w(t, |ui| ui.label(RichText::new(tr!("화면 위 가운데")).color(DIM).small()));
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
        card(t, tr!("안내"), |t| {
            switch(t, &mut state.guide_auto, tr!("자동 안내 — 따라가는 퀘스트의 다음 목표로 계속 안내"));
            switch(
                t,
                &mut state.route,
                tr!("실제 이동 경로 (A*) — 지형·물·벽을 돌아가는 길, 나침반이 다음 꺾이는 곳을 가리킴"),
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
                    let [r, gg, b] = g.tier.rgb();
                    t.style(tw::row(8.0)).add(|t| {
                        w(t, |ui| ui.label(RichText::new("◆").color(Color32::from_rgb(r, gg, b))));
                        text(t, RichText::new(&g.label).strong());
                        w(t, |ui| ui.label(RichText::new(crate::raster::distance(dist(g))).color(DIM)));
                        if w(t, |ui| ui.button(tr!("다음 목표 ▶")))
                            .on_hover_text(tr!("이 목표는 끝났거나 지금 갈 수 없음 — 넘기고 다음 목표로 (이번 실행 동안)"))
                            .clicked()
                        {
                            state.skipped.insert(g.id);
                            state.target = None;
                            state.chosen = false;
                            state.guide_auto = true;
                        }
                    });
                    if state.chosen {
                        t.style(tw::row(8.0)).add(|t| {
                            text(t, RichText::new(tr!("직접 고른 목표 — 끝날 때까지 유지")).color(DIM).small());
                            if w(t, |ui| ui.small_button(tr!("자동으로"))).clicked() {
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
                                tr!("⚠ 걸어서 닿는 길이 없습니다 — 닫힌 문·퍼즐·열쇠 너머일 수 있음. 닿는 곳까지 안내하고 나머지는 노란 점선"),
                            )
                            .color(WAIT)
                            .small(),
                        );
                    }
                }
                None => {
                    let why = if !state.guide_auto {
                        tr!("자동 안내가 꺼져 있습니다 — 위 스위치를 켜거나 '갈 곳' 에서 고르세요").to_string()
                    } else if goals.is_empty() {
                        tr!("이 근처에서 새로 얻을 것이 있는 곳을 찾지 못했습니다").to_string()
                    } else if elsewhere > 0 {
                        trf!("이 지역엔 따라가는 퀘스트의 목표가 없습니다 — 다른 지역에 {elsewhere}곳 (장갑차로 이동)", elsewhere = elsewhere)
                    } else if !state.skipped.is_empty() {
                        tr!("남은 목표를 모두 건너뛰었습니다").to_string()
                    } else {
                        tr!("따라가는 퀘스트의 목표가 이 근처에 없습니다 — '갈 곳' 에서 직접 고를 수 있습니다").to_string()
                    };
                    text(t, RichText::new(why).color(DIM));
                }
            }
            if !state.skipped.is_empty() {
                t.style(tw::row(8.0)).add(|t| {
                    text(t, RichText::new(trf!("건너뛴 목표 {a0}개", a0 = state.skipped.len())).color(DIM).small());
                    if w(t, |ui| ui.small_button(tr!("되돌리기"))).clicked() {
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
        card(t, &trf!("갈 곳 ({a0})", a0 = list.len()), |t| {
            block(t, |ui| {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for g in list {
                        let [r, gg, b] = g.tier.rgb();
                        let chosen = state.target == Some(g.id);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("◆").color(Color32::from_rgb(r, gg, b)));
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
