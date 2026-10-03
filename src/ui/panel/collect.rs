//! The collect page: collectibles taken here and everywhere, secrets done, NPCs with more to tell.

use super::*;

impl Panel {
    /// Collectibles placed in the worlds: taken here and everywhere, and the nearest
    /// left here, per sort (survey.rs `collection`).
    pub(super) fn collection_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.collection.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        card(t, "수집 진행도", |t| {
            if list.is_empty() {
                note(t, "조사 DB 가 없습니다 — 콘솔에서 `doctor survey` 를 한 번 실행하세요");
                return;
            }
            note(t, "월드에 놓인 것만 셉니다 (NPC 보상·상점은 제외) · 이 지역 / 전체");
            for c in list.iter() {
                let open = self.unfolded_collect == Some(c.label);
                t.style(tw::row(8.0)).add(|t| {
                    let label = format!("{}  {}/{} · 전체 {}/{}", c.label, c.here.0, c.here.1, c.all.0, c.all.1);
                    let done = c.here.0 == c.here.1;
                    if tw::pick(t, open, RichText::new(label).color(if done { DIM } else { Color32::from_gray(225) })) {
                        self.unfolded_collect = if open { None } else { Some(c.label) };
                    }
                });
                if open {
                    let mut left = c.left_here.clone();
                    if let Some(h) = here {
                        left.sort_by(|a, b| (a.at[0] - h[0]).hypot(a.at[1] - h[1]).total_cmp(&(b.at[0] - h[0]).hypot(b.at[1] - h[1])));
                    }
                    for x in left.iter().take(8) {
                        let far = here.map_or(String::new(), |h| crate::raster::distance((x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0));
                        if tw::pick(t, state.target == Some(x.id), format!("    {} ({far})", x.label)) {
                            guide_to(state, &goals, x);
                        }
                    }
                }
            }
        });
    }

    /// Good deeds, mysteries and timeloops: done of all.
    pub(super) fn secrets_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        use crate::quests::{Kind, Status};
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let totals = snap.map_or([0; 3], |s| s.secret_totals);
        card(t, "선행 · 미스터리 · 타임루프", |t| {
            for (i, (kind, _)) in Kind::SECRETS.iter().enumerate() {
                let of = |s: Status| journal.iter().filter(|q| q.kind == *kind && q.status == s).count();
                field(t, kind.label(), |t| {
                    text(t, format!("완료 {} / {} · 진행 중 {} · 실패 {}", of(Status::Completed), totals[i], of(Status::Started), of(Status::Failed)))
                });
            }
        });
    }

    /// NPCs whose talk still holds something new: here nearest first, elsewhere by count.
    pub(super) fn stories_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.stories.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        card(t, &format!("들을 이야기가 남은 NPC ({})", list.len()), |t| {
            note(t, "대화에서 아직 모르는 사실·단서를 줄 수 있는 사람 (갈래가 잠긴 대화도 포함)");
            let mut mine: Vec<&crate::survey::Need> = list.iter().filter(|x| Some(&x.world) == here_world.as_ref()).collect();
            if let Some(h) = here {
                mine.sort_by(|a, b| (a.at[0] - h[0]).hypot(a.at[1] - h[1]).total_cmp(&(b.at[0] - h[0]).hypot(b.at[1] - h[1])));
            }
            for x in mine.iter().take(10) {
                let far = here.map_or(String::new(), |h| crate::raster::distance((x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0));
                if tw::pick(t, state.target == Some(x.id), format!("{} ({far})", x.label.trim_start_matches("대화: "))) {
                    guide_to(state, &goals, x);
                }
            }
            let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for x in list.iter().filter(|x| Some(&x.world) != here_world.as_ref()) {
                *elsewhere.entry(x.world.as_str()).or_default() += 1;
            }
            if !elsewhere.is_empty() {
                note(t, format!("다른 지역: {}", elsewhere.iter().map(|(w, n)| format!("{w} {n}")).collect::<Vec<_>>().join(" · ")));
            }
        });
    }
}
