//! The debug page: each cheat's writes against what the game holds, memory, verification notes.

use super::*;

impl Panel {
    /// For each cheat that is on: every attribute it writes — what it was before,
    /// what is being written, what the game holds now. A row in red is a cheat the
    /// game is overriding. Then a place to record what each cheat did in play.
    pub(super) fn debug_tab(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let fmt = |v: Option<f32>| v.map_or("—".to_string(), |v| format!("{v:.2}"));
        if let (Some((ws, private)), Some(peak)) = (crate::memstat::now(), crate::memstat::peak()) {
            use crate::memstat::mb;
            ui.label(RichText::new(trf!("메모리: 사용 {a0} · 전용 {a1} · 최고 {a2} (1분마다 로그에도 남음)", a0 = mb(ws), a1 = mb(private), a2 = mb(peak))).color(DIM).small());
            ui.add_space(4.0);
        }
        ui.label(RichText::new(tr!("켜진 치트")).strong());
        ui.label(RichText::new(tr!("원래 값 → 넣는 값, 그리고 지금 게임에 들어 있는 값")).color(DIM).small());
        let active = snap.map(|s| s.active.clone()).unwrap_or_default();
        if active.is_empty() {
            ui.label(RichText::new(tr!("켜진 치트가 없습니다")).color(DIM));
        } else if let Some(snap) = snap {
            egui::ScrollArea::horizontal().id_salt("debug-scroll").show(ui, |ui| {
                egui::Grid::new("debug").num_columns(4).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for a in &active {
                        let Some(c) = cheats::find(a.cheat) else { continue };
                        for e in c.effects() {
                            let (target, want) = match *e {
                                Effect::Fixed(t, v) => (t, Some(v)),
                                Effect::Chosen(t) => (t, Some(a.value)),
                                Effect::Fill(t, max) => (t, snap.value(max)),
                                // Past the hero: many targets, not one value to compare.
                                Effect::EnemyTime
                                | Effect::EnemyFrail
                                | Effect::Stock(_)
                                | Effect::WeaponXp
                                | Effect::Ghost
                                | Effect::Untouchable => {
                                    ui.label(crate::i18n::tr(c.label));
                                    ui.label(RichText::new(tr!("적·인벤토리 대상")).color(DIM).small());
                                    ui.label("");
                                    ui.label(RichText::new(tr!("매 틱 적용")).color(OK));
                                    ui.end_row();
                                    continue;
                                }
                            };
                            let now = snap.value(target);
                            let was = snap.originals.iter().find(|(x, _)| *x == target).map(|(_, (_, c))| *c);
                            ui.label(crate::i18n::tr(c.label));
                            ui.label(RichText::new(target.name).color(DIM).small());
                            ui.label(format!("{} → {}", fmt(was), fmt(want)));
                            match (want, now) {
                                (Some(w), Some(n)) if (w - n).abs() <= 0.01 + w.abs() * 0.01 => {
                                    ui.label(RichText::new(trf!("적용 중 {n:.2}", n = n)).color(OK))
                                }
                                (Some(_), Some(n)) => {
                                    ui.label(RichText::new(trf!("게임이 바꿈 {n:.2}", n = n)).color(BAD)).on_hover_text(
                                        tr!("넣은 값과 다릅니다. 게임이 매 순간 다시 계산하는 값일 수 있습니다."),
                                    )
                                }
                                _ => ui.label(RichText::new(tr!("읽을 수 없음")).color(BAD)),
                            };
                            ui.end_row();
                        }
                    }
                });
            });
        }

        ui.separator();
        ui.label(RichText::new(tr!("테스트 결과 기록")).strong());
        ui.label(RichText::new(tr!("게임에서 해 보고 눌러 주세요. verify.txt에 저장됩니다.")).color(DIM).small());
        let mut changed = false;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt("marks-scroll").show(ui, |ui| {
                egui::Grid::new("marks").num_columns(3).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for c in CHEATS {
                        ui.label(crate::i18n::tr(c.label));
                        let mark = self.marks.get(c.id).copied();
                        ui.horizontal(|ui| {
                            if ui.selectable_label(mark == Some(true), tr!("됨 ✓")).clicked() {
                                self.marks.insert(c.id.to_string(), true);
                                changed = true;
                            }
                            if ui.selectable_label(mark == Some(false), tr!("안 됨 ✗")).clicked() {
                                self.marks.insert(c.id.to_string(), false);
                                changed = true;
                            }
                        });
                        ui.label(
                            RichText::new(if c.verified { tr!("확인됨") } else { tr!("미검증") })
                                .color(if c.verified { OK } else { WAIT })
                                .small(),
                        );
                        ui.end_row();
                    }
                });
            });
        });
        if changed {
            if let Err(e) = verify::save(&self.marks) {
                self.reply = Some((false, e, Instant::now()));
            }
        }

        ui.separator();
        if ui.button(tr!("로그·기록 폴더 열기")).on_hover_text("hiumod.log, verify.txt, originals.txt").clicked() {
            if let Some(dir) = verify::path().parent() {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::process::Command::new("explorer.exe")
                    .arg(dir)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }
    }
}
