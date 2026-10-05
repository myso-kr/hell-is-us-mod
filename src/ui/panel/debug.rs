//! The debug page: each cheat's writes against what the game holds, memory, verification notes.

use super::*;
use tw::Tone;

impl Panel {
    /// The page's figures: the memory the panel takes (now and at its peak), the cheats
    /// on, how many are verified in play; and the log folder.
    pub(super) fn debug_status(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        use crate::memstat::mb;
        let on = snap.map_or(0, |s| s.active.len());
        let verified = CHEATS.iter().filter(|c| c.verified).count();
        let (working, peak) = match (crate::memstat::now(), crate::memstat::peak()) {
            (Some((ws, _)), Some(p)) => (mb(ws), mb(p)),
            _ => ("-".into(), "-".into()),
        };
        card(t, tr!("DEBUG_STATUS"), |t| {
            t.style(tw::grid(2, crate::ui::theme::INLINE)).add(|t| {
                for (label, value) in [
                    (tr!("MEMORY_NOW"), working),
                    (tr!("MEMORY_PEAK"), peak),
                    (tr!("CHEATS_ON"), on.to_string()),
                    (tr!("VERIFIED"), format!("{verified}/{}", CHEATS.len())),
                ] {
                    tw::stat(t, |_| {}, &value, label, None);
                }
            });
            note(t, tr!("MEMORY_ALSO_LOGGED"));
            if w(t, |ui| ui.button(tr!("OPEN_THE_LOG_FOLDER")).on_hover_text("hiumod.log, verify.txt, originals.txt"))
                .clicked()
            {
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
        });
    }

    /// For each cheat that is on: every attribute it writes, what it was before, what
    /// is being written, and a chip for what the game holds now. A red chip is a cheat
    /// the game is overriding.
    pub(super) fn debug_writes(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        // An unknown value: a plain mark, not a dash that reads as a minus.
        let fmt = |v: Option<f32>| v.map_or("?".to_string(), |v| format!("{v:.2}"));
        let title = tr!("CHEATS_ON");
        tw::card_with(
            t,
            title,
            |ui| {
                ui.label(RichText::new("?").color(DIM)).on_hover_text(tr!("ORIGINAL_WRITTEN_AND_WHAT_THE_GAME"));
            },
            |t| {
                block(t, |ui| {
                    let active = snap.map(|s| s.active.clone()).unwrap_or_default();
                    if active.is_empty() {
                        ui.label(RichText::new(tr!("NO_CHEATS_ARE_ON")).color(DIM));
                        return;
                    }
                    let Some(snap) = snap else { return };
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
                                        | Effect::EnemyHealth
                                        | Effect::Stock(_)
                                        | Effect::WeaponXp
                                        | Effect::Ghost
                                        | Effect::Untouchable => {
                                            ui.label(crate::i18n::tr(c.label));
                                            ui.label(RichText::new(tr!("ENEMIES_AND_INVENTORY")).color(DIM).small());
                                            ui.label("");
                                            tw::pill(ui, tr!("APPLIED_EVERY_TICK"), Tone::Ok);
                                            ui.end_row();
                                            continue;
                                        }
                                    };
                                    let now = snap.value(target);
                                    let was = snap.originals.iter().find(|(x, _)| *x == target).map(|(_, (_, c))| *c);
                                    ui.label(crate::i18n::tr(c.label));
                                    ui.label(RichText::new(target.name).color(DIM).small());
                                    ui.label(RichText::new(format!("{} → {}", fmt(was), fmt(want))).monospace());
                                    match (want, now) {
                                        (Some(w), Some(n)) if (w - n).abs() <= 0.01 + w.abs() * 0.01 => {
                                            tw::pill(ui, trf!("APPLIED", n = n), Tone::Ok)
                                        }
                                        (Some(_), Some(n)) => tw::pill(ui, trf!("GAME_CHANGED_IT", n = n), Tone::Bad)
                                            .on_hover_text(tr!("DIFFERS_FROM_WHAT_WAS_WRITTEN_THE")),
                                        _ => tw::pill(ui, tr!("UNREADABLE"), Tone::Bad),
                                    };
                                    ui.end_row();
                                }
                            }
                        });
                    });
                })
            },
        );
    }

    /// What the guide works with (the overlay's trace, also in `Mods\doctor\guide.jsonl`):
    /// the auto guide's target, where it comes from and its route, the nearest goals with
    /// why each is picked or not, and the last events.
    pub(super) fn debug_guide(&mut self, t: &mut Tui) {
        let text = self.shared.trace.lock().unwrap().clone();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let s = |v: &serde_json::Value| match v {
            serde_json::Value::String(x) => x.clone(),
            serde_json::Value::Null => "-".into(),
            x => x.to_string(),
        };
        card(t, tr!("DEBUG_GUIDE"), |t| {
            if v.is_null() {
                note(t, tr!("DEBUG_GUIDE_EMPTY"));
                return;
            }
            let a = &v["auto"];
            let head = if a.is_null() {
                trf!("DEBUG_GUIDE_NO_TARGET", world = s(&v["world"]))
            } else {
                format!(
                    "{} · {} · {}m · {}m · {}",
                    s(&a["label"]),
                    s(&a["source"]),
                    s(&a["distance_m"]),
                    s(&a["height_m"]),
                    if a["blocked"] == true { tr!("DEBUG_BLOCKED") } else { "" }
                )
            };
            block(t, |ui| ui.label(RichText::new(head).strong()));
            if !a["route"].is_null() {
                note(t, format!("route: {}", a["route"]));
            }
            block(t, |ui| {
                ui.columns(2, |cols| {
                    egui::Grid::new("guide-near").num_columns(5).striped(true).spacing([8.0, 2.0]).show(
                        &mut cols[0],
                        |ui| {
                            for g in v["nearest"].as_array().into_iter().flatten() {
                                ui.add(egui::Label::new(s(&g["label"])).truncate());
                                ui.label(RichText::new(s(&g["source"])).small().color(DIM));
                                ui.label(
                                    RichText::new(format!("{}m {}m", s(&g["distance_m"]), s(&g["height_m"])))
                                        .monospace()
                                        .small(),
                                );
                                let flags: Vec<&str> =
                                    [("wanted", "W"), ("blocked", "B"), ("skipped", "S"), ("followed", "F")]
                                        .iter()
                                        .filter(|(k, _)| g[*k] == true)
                                        .map(|(_, f)| *f)
                                        .collect();
                                ui.label(RichText::new(flags.join("")).monospace().small().color(DIM));
                                ui.end_row();
                            }
                        },
                    );
                    let ui = &mut cols[1];
                    for e in v["events"].as_array().into_iter().flatten().take(14) {
                        ui.add(egui::Label::new(RichText::new(s(e)).small().monospace()).wrap());
                    }
                });
            });
            note(t, tr!("DEBUG_GUIDE_HINT"));
        });
    }

    /// A place to record what each cheat did in play, two columns of cheats.
    pub(super) fn debug_marks(&mut self, t: &mut Tui) {
        let mut changed = false;
        card(t, tr!("RECORD_TEST_RESULTS"), |t| {
            note(t, tr!("TRY_IT_IN_THE_GAME_THEN"));
            block(t, |ui| {
                let half = CHEATS.len().div_ceil(2);
                ui.columns(2, |cols| {
                    for (k, part) in CHEATS.chunks(half).enumerate() {
                        let ui = &mut cols[k];
                        egui::Grid::new(("marks", k)).num_columns(3).striped(true).spacing([10.0, 4.0]).show(
                            ui,
                            |ui| {
                                for c in part {
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
                                    if c.verified {
                                        tw::pill(ui, tr!("VERIFIED"), Tone::Ok);
                                    } else {
                                        tw::pill(ui, tr!("UNVERIFIED"), Tone::Wait);
                                    }
                                    ui.end_row();
                                }
                            },
                        );
                    }
                });
            });
        });
        if changed {
            if let Err(e) = verify::save(&self.marks) {
                self.reply = Some((false, e, Instant::now()));
            }
        }
    }
}
