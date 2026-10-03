//! Phase 3 (.spec/GUIDE.md §27): the puzzles near the hero with their answers kept
//! hidden until asked for (F6), the vault notebook (F7), the enemies left (F8).

use super::*;
use crate::puzzles::{Answer, Puzzle};
use crate::tables::VaultState;

/// A stable id for a place the guide is sent to, from a GUID (bit 63: a survey id).
fn id_of(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish() | 1 << 63
}

/// A puzzle's answer in words.
fn answer(p: &Puzzle) -> Vec<String> {
    match &p.answer {
        Answer::Dials(dials) => dials
            .iter()
            .enumerate()
            .map(|(i, d)| match d.turns() {
                0 => trf!("다이얼 {n}: 맞음", n = i + 1),
                k => trf!("다이얼 {n}: {k}칸 돌리기 (지금 {now} → {want})", n = i + 1, k = k, now = d.now + 1, want = d.want + 1),
            })
            .collect(),
        Answer::Code(code) => vec![trf!("코드: {code}", code = code)],
        Answer::Items(items) => {
            vec![trf!("필요한 것: {items}", items = items.iter().map(|i| crate::goals::item_label(i)).collect::<Vec<_>>().join(" · "))]
        }
    }
}

/// How big a puzzle is: dials and their places, the code's length, the items.
fn shape(a: &Answer) -> String {
    match a {
        Answer::Dials(d) => {
            let mut places: Vec<String> = d.iter().map(|x| x.places.to_string()).collect();
            places.dedup();
            trf!("다이얼 {n}개 · {places}칸", n = d.len(), places = places.join("/"))
        }
        Answer::Code(c) => trf!("{n}자리 코드", n = c.chars().count()),
        Answer::Items(i) => trf!("물건 {n}개", n = i.len()),
    }
}

/// A listed puzzle's answer: the dials' places in order (from 1), the code, the items.
fn listed_answer(a: &Answer) -> String {
    match a {
        Answer::Dials(d) => trf!("다이얼 위치 (왼쪽부터, 1부터): {list}", list = d.iter().map(|x| (x.want + 1).to_string()).collect::<Vec<_>>().join(" · ")),
        Answer::Code(c) => trf!("코드: {code}", code = c),
        Answer::Items(i) => trf!("필요한 것: {items}", items = i.iter().map(|x| crate::goals::item_label(x)).collect::<Vec<_>>().join(" · ")),
    }
}

/// A vault code as its symbols (assets/symbols), their names on hover.
fn symbol_row(t: &mut Tui, code: &[u8]) {
    w(t, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            for &c in code {
                crate::ui::svg::symbol(ui, c, 30.0, OK);
            }
        })
    });
}

/// The symbols a vault door's dials want (from 1), when the puzzle is a vault's.
fn vault_dials(class: &str, a: &Answer) -> Option<Vec<u8>> {
    match a {
        Answer::Dials(d) if class.starts_with("VOFK_") => Some(d.iter().map(|x| x.want + 1).collect()),
        _ => None,
    }
}

impl Panel {
    /// Steam's achievements in the game's language: how many, the ones left with
    /// their progress, a hidden one's text only once unlocked or asked for.
    pub(super) fn achievements_card(&mut self, t: &mut Tui) {
        if self.achievements.as_ref().is_none_or(|(at, _)| at.elapsed() >= std::time::Duration::from_secs(10)) {
            self.achievements = Some((std::time::Instant::now(), crate::game::achievements::load(&crate::i18n::culture())));
        }
        let list = self.achievements.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
        let done = list.iter().filter(|a| a.unlocked).count();
        card(t, &trf!("업적 {done}/{all}", done = done, all = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("Steam 업적 캐시를 찾지 못했습니다 (Steam/appcache/stats)"));
                return;
            }
            w(t, |ui| ui.checkbox(&mut self.show_unlocked, tr!("달성한 것도 보기")));
            for a in list.iter().filter(|a| self.show_unlocked || !a.unlocked) {
                let id = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    a.api.hash(&mut h);
                    h.finish()
                };
                let secret = a.hidden && !a.unlocked && !self.revealed.contains(&id);
                let progress = a.progress.filter(|(_, of)| *of > 1).map(|(v, of)| format!("  {}/{of}", v.min(of))).unwrap_or_default();
                t.style(tw::row(8.0)).add(|t| {
                    let head = if secret { tr!("숨겨진 업적").to_string() } else { format!("{}{}{progress}", if a.unlocked { "✓ " } else { "" }, a.name) };
                    text(t, RichText::new(head).color(if a.unlocked { DIM } else { Color32::from_gray(225) }));
                    if secret && w(t, |ui| ui.small_button(tr!("보기"))).clicked() {
                        self.revealed.insert(id);
                    }
                });
                if !secret {
                    text(t, RichText::new(format!("    {}", a.desc)).color(DIM).small());
                }
            }
        });
    }

    /// Every puzzle of the worlds (the survey): this region's left first — dials and
    /// codes, and on request the keys and item placements — with the answer behind a
    /// button and a guide to it; the other regions as counts.
    pub(super) fn catalogue_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.catalogue.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        let shown = |p: &crate::survey::Placed, placements: bool| placements || p.kind != crate::puzzles::Kind::Placement;
        let mine: Vec<&(crate::survey::Placed, bool)> = list
            .iter()
            .filter(|(p, _)| Some(&p.world) == here_world.as_ref() && shown(p, self.show_placements))
            .collect();
        let left = mine.iter().filter(|(_, solved)| !solved).count();
        card(t, &trf!("퍼즐 목록 · 이 지역 남은 {left}", left = left), |t| {
            if list.is_empty() {
                note(t, tr!("퍼즐 목록이 없습니다 — 콘솔에서 `doctor survey` 를 한 번 실행하세요"));
                return;
            }
            w(t, |ui| ui.checkbox(&mut self.show_placements, tr!("열쇠·물건 놓기 퍼즐도 보기")));
            let mut rows = mine.clone();
            let far = |p: &crate::survey::Placed| here.map_or(0.0, |h| (p.at[0] - h[0]).hypot(p.at[1] - h[1]));
            rows.sort_by(|a, b| a.1.cmp(&b.1).then(far(&a.0).total_cmp(&far(&b.0))));
            for (p, solved) in rows.iter().take(30) {
                let id = p.id();
                let open = self.revealed.contains(&id);
                let dist = here.map_or(String::new(), |_| crate::raster::distance(far(p) / 100.0));
                let head = format!(
                    "{} · {} · {} ({dist}){}",
                    crate::i18n::tr(p.kind.label()),
                    shape(&p.answer),
                    crate::goals::pretty(&p.class),
                    if *solved { " ✓" } else { "" }
                );
                t.style(tw::row(8.0)).add(|t| {
                    text(t, RichText::new(head).color(if *solved { DIM } else { Color32::from_gray(225) }).small());
                    if w(t, |ui| ui.small_button(if open { tr!("숨기기") } else { tr!("답 보기") })).clicked() {
                        if open {
                            self.revealed.remove(&id);
                        } else {
                            self.revealed.insert(id);
                        }
                    }
                    if !*solved && w(t, |ui| ui.small_button(tr!("안내"))).clicked() {
                        let x = crate::survey::Need {
                            world: p.world.clone(),
                            id,
                            label: format!("{} · {}", crate::i18n::tr(p.kind.label()), shape(&p.answer)),
                            what: String::new(),
                            at: p.at,
                            done: false,
                        };
                        guide_to(state, &goals, &x);
                    }
                });
                if open {
                    match vault_dials(&p.class, &p.answer) {
                        Some(code) => symbol_row(t, &code),
                        None => text(t, RichText::new(format!("    {}", listed_answer(&p.answer))).color(OK)),
                    }
                }
            }
            // The other regions: how many are left there.
            let mut elsewhere: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for (p, _) in list.iter().filter(|(p, s)| !s && Some(&p.world) != here_world.as_ref() && shown(p, self.show_placements)) {
                *elsewhere.entry(p.world.as_str()).or_default() += 1;
            }
            if !elsewhere.is_empty() {
                let parts: Vec<String> = elsewhere.iter().map(|(w, n)| format!("{} {n}", crate::i18n::place(w))).collect();
                note(t, trf!("다른 지역: {a0}", a0 = parts.join(" · ")));
            }
        });
    }

    /// The puzzles within 40 m: kind, name, how far; the answer behind a button.
    pub(super) fn puzzles_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.puzzles.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        card(t, &trf!("근처 퍼즐 ({n})", n = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("40 m 안에 다이얼·키패드·물건 놓기 퍼즐이 없습니다"));
                return;
            }
            note(t, tr!("게임이 가진 정답을 읽습니다 — 직접 풀고 싶으면 누르지 마세요"));
            for p in list.iter() {
                let far = here.map_or(String::new(), |h| crate::raster::distance((p.at[0] - h[0]).hypot(p.at[1] - h[1]) / 100.0));
                let name = crate::goals::pretty(&p.class);
                let head = format!("{} · {name} ({far}){}", crate::i18n::tr(p.kind.label()), if p.solved { " ✓" } else { "" });
                let open = self.revealed.contains(&p.id);
                t.style(tw::row(8.0)).add(|t| {
                    text(t, RichText::new(head).color(if p.solved { DIM } else { Color32::from_gray(225) }));
                    if w(t, |ui| ui.small_button(if open { tr!("숨기기") } else { tr!("답 보기") })).clicked() {
                        if open {
                            self.revealed.remove(&p.id);
                        } else {
                            self.revealed.insert(p.id);
                        }
                    }
                });
                if open {
                    if let Some(code) = vault_dials(&p.class, &p.answer) {
                        symbol_row(t, &code);
                    }
                    for line in answer(p) {
                        text(t, RichText::new(format!("    {line}")).color(OK));
                    }
                }
            }
        });
    }

    /// The Vaults of Forbidden Knowledge: opened, known (clue, code behind a button,
    /// guide to the door) or waiting on research.
    pub(super) fn vaults_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.vaults.clone()).unwrap_or_default();
        let lore = snap.map_or(0, |s| s.lore_known);
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let opened = list.iter().filter(|v| v.state == VaultState::Opened).count();
        card(t, &trf!("금단의 지식 금고 {done}/{all}", done = opened, all = list.len()), |t| {
            if list.is_empty() {
                note(t, tr!("금고 표가 없습니다 — 콘솔에서 `doctor survey` 를 한 번 실행하세요"));
                return;
            }
            note(t, trf!("장갑차의 {g:Facts_Tania/Tania_Real_Name}에게 연구 아이템을 맡기면 금고 정보가 풀립니다 · 지금 연구 {n}개", n = lore));
            note(t, tr!("연구 아이템은 '수집 진행도' 의 연구 자료 줄을 누르면 가까운 것부터 안내합니다"));
            for v in list.iter() {
                let name = crate::i18n::game_text(&v.vault.name);
                let region = crate::i18n::game_text(&v.vault.region);
                let status = match v.state {
                    VaultState::Opened => tr!("열림 ✓").to_string(),
                    VaultState::Known => tr!("정보 있음").to_string(),
                    VaultState::Locked => trf!("연구 {n}/{need}", n = lore, need = v.vault.entries),
                };
                let id = id_of(&v.vault.guid);
                let open = self.revealed.contains(&id);
                t.style(tw::row(8.0)).add(|t| {
                    let colour = if v.state == VaultState::Opened { DIM } else { Color32::from_gray(225) };
                    text(t, RichText::new(format!("{name} · {region} — {status}")).color(colour));
                    if v.state != VaultState::Opened && w(t, |ui| ui.small_button(if open { tr!("숨기기") } else { tr!("코드 보기") })).clicked() {
                        if open {
                            self.revealed.remove(&id);
                        } else {
                            self.revealed.insert(id);
                        }
                    }
                    if let Some((world, at)) = v.door.clone().filter(|_| v.state != VaultState::Opened) {
                        if w(t, |ui| ui.small_button(tr!("안내"))).clicked() {
                            let x = crate::survey::Need { world, id, label: name.clone(), what: String::new(), at, done: false };
                            guide_to(state, &goals, &x);
                        }
                    }
                });
                if open {
                    if v.state == VaultState::Known {
                        text(t, RichText::new(format!("    {}", crate::i18n::game_text(&v.vault.clue))).color(DIM).small());
                    }
                    symbol_row(t, &v.vault.code);
                    note(t, tr!("    문 앞 다이얼에서는 '근처 퍼즐' 카드가 몇 칸 돌릴지 알려 줍니다"));
                }
            }
        });
    }

    /// The Hollows left for "every Hollow" (Legend of the Phol): per region, its
    /// timeloops, and a button to the nearest one left here.
    pub(super) fn hollows_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let list = snap.map(|s| s.hollows.clone()).unwrap_or_default();
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here_world = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32]);
        let (left, all) = list.iter().fold((0, 0), |(l, a), h| (l + h.left, a + h.all));
        card(t, &trf!("남은 적 무리 {left}/{all}", left = left, all = all), |t| {
            if list.is_empty() {
                note(t, tr!("스포너 표가 없습니다 — 콘솔에서 `doctor survey` 를 한 번 실행하세요"));
                return;
            }
            note(t, tr!("적 무리(스포너) 단위 · 세이브에 기록된 무리는 처치한 것으로 셉니다"));
            for h in list.iter() {
                let mine = here_world.as_deref() == Some(h.world.as_str());
                let line = trf!("{place}: {left}/{all} 무리 · 적 {enemies}", place = crate::i18n::place(&h.world), left = h.left, all = h.all, enemies = h.enemies_left);
                t.style(tw::row(8.0)).add(|t| {
                    let colour = if h.left == 0 { DIM } else if mine { Color32::from_gray(235) } else { Color32::from_gray(200) };
                    text(t, RichText::new(line).color(colour));
                    if mine && h.left > 0 {
                        if let (Some(p), true) = (here, w(t, |ui| ui.small_button(tr!("가장 가까운 곳"))).clicked()) {
                            let near = h.places.iter().min_by(|a, b| (a[0] - p[0]).hypot(a[1] - p[1]).total_cmp(&(b[0] - p[0]).hypot(b[1] - p[1])));
                            if let Some(at) = near {
                                let x = crate::survey::Need {
                                    world: h.world.clone(),
                                    id: id_of(&format!("hollow{at:?}")),
                                    label: tr!("남은 적 무리").to_string(),
                                    what: String::new(),
                                    at: *at,
                                    done: false,
                                };
                                guide_to(state, &goals, &x);
                            }
                        }
                    }
                });
                for (lp, l, a) in h.timeloops.iter().filter(|(_, l, _)| *l > 0) {
                    note(t, trf!("    타임루프 {name}: {left}/{all}", name = lp.trim_end_matches("_BP").trim_end_matches("_BP2"), left = l, all = a));
                }
            }
        });
    }
}
