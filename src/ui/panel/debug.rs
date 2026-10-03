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
            ui.label(RichText::new(trf!("MEMORY_IN_USE_PRIVATE_PEAK_ALSO", working = mb(ws), private = mb(private), peak = mb(peak))).color(DIM).small());
            ui.add_space(4.0);
        }
        ui.label(RichText::new(tr!("CHEATS_ON")).strong());
        ui.label(RichText::new(tr!("ORIGINAL_WRITTEN_AND_WHAT_THE_GAME")).color(DIM).small());
        let active = snap.map(|s| s.active.clone()).unwrap_or_default();
        if active.is_empty() {
            ui.label(RichText::new(tr!("NO_CHEATS_ARE_ON")).color(DIM));
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
                                    ui.label(RichText::new(tr!("ENEMIES_AND_INVENTORY")).color(DIM).small());
                                    ui.label("");
                                    ui.label(RichText::new(tr!("APPLIED_EVERY_TICK")).color(OK));
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
                                    ui.label(RichText::new(trf!("APPLIED", n = n)).color(OK))
                                }
                                (Some(_), Some(n)) => {
                                    ui.label(RichText::new(trf!("GAME_CHANGED_IT", n = n)).color(BAD)).on_hover_text(
                                        tr!("DIFFERS_FROM_WHAT_WAS_WRITTEN_THE"),
                                    )
                                }
                                _ => ui.label(RichText::new(tr!("UNREADABLE")).color(BAD)),
                            };
                            ui.end_row();
                        }
                    }
                });
            });
        }

        ui.separator();
        ui.label(RichText::new(tr!("RECORD_TEST_RESULTS")).strong());
        ui.label(RichText::new(tr!("TRY_IT_IN_THE_GAME_THEN")).color(DIM).small());
        let mut changed = false;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt("marks-scroll").show(ui, |ui| {
                egui::Grid::new("marks").num_columns(3).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for c in CHEATS {
                        ui.label(crate::i18n::tr(c.label));
                        let mark = self.marks.get(c.id).copied();
                        ui.horizontal(|ui| {
                            if ui.selectable_label(mark == Some(true), tr!("WORKS")).clicked() {
                                self.marks.insert(c.id.to_string(), true);
                                changed = true;
                            }
                            if ui.selectable_label(mark == Some(false), tr!("FAILS")).clicked() {
                                self.marks.insert(c.id.to_string(), false);
                                changed = true;
                            }
                        });
                        ui.label(
                            RichText::new(if c.verified { tr!("VERIFIED") } else { tr!("UNVERIFIED") })
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
        if ui.button(tr!("OPEN_THE_LOG_FOLDER")).on_hover_text("hiumod.log, verify.txt, originals.txt").clicked() {
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
