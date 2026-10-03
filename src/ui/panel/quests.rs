//! The quest page: the journal and what the followed quest needs, missable deadlines, hand-overs.

use super::*;

impl Panel {
    /// The quest journal: which quest the guide and the tracker follow.
    pub(super) fn quests_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        use crate::quests::Status;
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        card(t, "퀘스트", |t| {
            switch(t, &mut state.tracker, "퀘스트 추적기 — 화면 오른쪽 가운데");
            if journal.is_empty() {
                note(t, "퀘스트를 읽는 중입니다 — 게임을 불러오고 몇 초 뒤에 나옵니다");
                return;
            }
            let followed = crate::quests::followed(&journal, state.quest.as_deref()).map(|q| q.key.clone());
            let mut pick: Option<Option<String>> = None;
            let auto = match journal.iter().find(|q| Some(&q.key) == followed.as_ref()) {
                Some(q) if state.quest.is_none() => format!("메인 스토리 자동 — {}", q.name),
                _ => "메인 스토리 자동".to_string(),
            };
            if tw::pick(t, state.quest.is_none(), auto) {
                pick = Some(None);
            }
            for q in journal.iter().filter(|q| q.active()) {
                let tag = q.kind.label();
                let mut label = format!("[{tag}] {}", q.name);
                if let Some((got, all)) = q.progress.filter(|(_, all)| *all > 0) {
                    label += &format!(" · 단서 {got}/{all}");
                }
                let on = state.quest.as_deref() == Some(q.key.as_str());
                if tw::pick(t, on, label) {
                    pick = Some(Some(q.key.clone()));
                }
            }
            let done = journal.iter().filter(|q| q.status == Status::Completed).count();
            let failed = journal.iter().filter(|q| q.status == Status::Failed).count();
            note(t, format!("완료 {done}개 · 실패 {failed}개"));
            note(t, "영어로 나오는 선행 이름은 게임이 아직 보여 주지 않은 것 — 데이터패드의 탐험 → 선행에서 보면 한국어로 바뀝니다");
            // What the followed quest needs, from the survey of every world.
            let needs = snap
                .and_then(|s| s.needs.iter().find(|(k, _)| Some(k) == followed.as_ref()))
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
            let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
            let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
            if needs.is_empty() {
                note(t, "필요한 것: 조사 DB 없음 — `hiumod doctor survey` 를 한 번 실행하면 모든 지역의 아이템·NPC 위치가 채워집니다");
            } else {
                let left = needs.iter().filter(|x| !x.done).count();
                text(t, RichText::new(format!("필요한 것 — 남은 {left} / {}", needs.len())).strong());
                // This world's, nearest first: press to guide there.
                let mut mine: Vec<&crate::survey::Need> =
                    needs.iter().filter(|x| !x.done && Some(&x.world) == here_world.as_ref()).collect();
                let d = |x: &crate::survey::Need| here.map_or(0.0, |h| (x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0);
                mine.sort_by(|a, b| d(a).total_cmp(&d(b)));
                for x in mine.iter().take(8) {
                    let label = format!("{} — {} ({})", x.what, x.label, crate::raster::distance(d(x)));
                    if tw::pick(t, state.target == Some(x.id), label) {
                        guide_to(state, &goals, x);
                    }
                }
                // Other worlds: how many, where.
                let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
                for x in needs.iter().filter(|x| !x.done && Some(&x.world) != here_world.as_ref()) {
                    *elsewhere.entry(x.world.as_str()).or_default() += 1;
                }
                if !elsewhere.is_empty() {
                    let list: Vec<String> = elsewhere.iter().map(|(w, n)| format!("{w} {n}")).collect();
                    note(t, format!("다른 지역 (장갑차로 이동): {}", list.join(" · ")));
                }
            }
            if let Some(p) = pick {
                state.quest = p;
                // Guide anew, to the newly followed quest.
                state.target = None;
                state.chosen = false;
                state.guide_auto = true;
                state.route = true;
                state.dirty = true;
            }
        });
    }

    /// Missable good deeds and their deadlines; in act 2, the keystones left and what is
    /// due before the next (missables.rs).
    pub(super) fn deadlines_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        use crate::missables::When;
        let list = snap.map(|s| s.deadlines.clone()).unwrap_or_default();
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        card(t, "놓치기 쉬운 선행", |t| {
            if list.is_empty() {
                note(t, "마감이 있는 선행은 모두 끝났거나 지났습니다");
            }
            for d in list.iter().filter(|d| d.when != When::Passed) {
                let (mark, colour) = match d.when {
                    When::Now => ("임박", BAD),
                    _ => ("나중", DIM),
                };
                t.style(tw::row(8.0)).add(|t| {
                    w(t, |ui| ui.label(RichText::new(mark).color(colour).small().strong()));
                    let label = format!("{}{}", d.title, if d.started { "" } else { " (시작 전)" });
                    if tw::pick(t, state.quest.as_deref() == Some(d.key.as_str()), label) && d.started {
                        state.quest = Some(d.key.clone());
                        state.target = None;
                        state.chosen = false;
                        state.guide_auto = true;
                        state.route = true;
                    }
                });
                note(t, format!("{} — {}", d.due.label(), d.what));
            }
            let passed = list.iter().filter(|d| d.when == When::Passed).count();
            if passed > 0 {
                note(t, format!("이미 지난 마감 {passed}개 — 그 선행은 실패했을 가능성이 큽니다"));
            }
        });
        if let Some((left, before)) = crate::missables::keystone_advice(&journal, &list) {
            card(t, "키스톤 순서", |t| {
                note(t, "권장 순서 (공포 먼저: 일부 아이템이 Talju 에만 있음) — 키스톤마다 시간이 흘러 선행이 끝날 수 있습니다");
                for (i, l) in left.iter().enumerate() {
                    text(t, format!("{}. {l}", i + 1));
                }
                if !before.is_empty() {
                    text(t, RichText::new(format!("다음 키스톤 전에: {}", before.join(" · "))).color(BAD));
                }
            });
        }
    }

    /// Items the hero holds that someone wants: who, where — press to guide there.
    pub(super) fn handovers_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.handovers.clone()).unwrap_or_default();
        if list.is_empty() {
            card(t, "건네줄 수 있는 것", |t| note(t, "지금 가진 아이템을 원하는 사람이 없습니다"));
            return;
        }
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        card(t, &format!("건네줄 수 있는 것 ({})", list.len()), |t| {
            note(t, "가진 아이템을 원하는 사람 — 대화의 \"거래\" 로 건넵니다");
            for x in list.iter() {
                let same = Some(&x.world) == here_world.as_ref();
                let place = if same {
                    here.map_or(String::new(), |h| crate::raster::distance((x.at[0] - h[0]).hypot(x.at[1] - h[1]) / 100.0))
                } else {
                    format!("{} — 장갑차로 이동", x.world)
                };
                let label = format!("{} → {} ({place})", x.what, x.label.trim_start_matches("대화: "));
                if same {
                    if tw::pick(t, state.target == Some(x.id), label) {
                        guide_to(state, &goals, x);
                    }
                } else {
                    text(t, RichText::new(label).color(DIM));
                }
            }
        });
    }
}
