//! The save page: the backups and the game's own save files.

use super::*;
use crate::ui::theme::{ACCENT, EDGE, INLINE, TEXT};

/// How many backups the timeline shows; the rest are in the folder.
const SHOWN: usize = 6;

/// How long ago, said as people say it: just now, 5 min ago, 3h ago, 2 d ago.
fn ago(at: std::time::SystemTime) -> String {
    let s = at.elapsed().map_or(0, |d| d.as_secs());
    match s {
        0..60 => tr!("JUST_NOW").to_string(),
        60..3600 => trf!("MIN_AGO", minutes = s / 60),
        3600..86400 => trf!("H_AGO", hours = s / 3600),
        _ => trf!("DAYS_AGO", count = s / 86400),
    }
}

/// A backup folder's name (`2026-10-03_12-30-00_manual`, logfile::stamp then the
/// reason) as its time (`10-03 12:30`) and why it was made. A name in another shape
/// is shown as it is.
fn when_and_why(name: &str) -> (String, String) {
    let shaped = name.len() >= 19 && name.as_bytes()[10] == b'_' && name.is_char_boundary(19);
    if !shaped {
        return (name.replace('_', " "), String::new());
    }
    let time = format!("{} {}", &name[5..10], name[11..16].replace('-', ":"));
    let why = match name.get(20..).unwrap_or("") {
        "" => tr!("BACKUP_AUTO").to_string(),
        "manual" => tr!("BACKUP_MANUAL").to_string(),
        other => other.replace('_', " "),
    };
    (time, why)
}

/// The backups as a timeline, newest on top: a dot on a thin rail (the newest in the
/// accent), the time, why it was made (dim), and how long ago at the right edge.
fn timeline(ui: &mut egui::Ui, rows: &[(String, String, String)]) {
    const ROW: f32 = 20.0;
    const RAIL: f32 = 14.0;
    for (i, (time, why, age)) in rows.iter().enumerate() {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
        let p = ui.painter();
        let dot = egui::pos2(rect.left() + RAIL / 2.0, rect.center().y);
        if i + 1 < rows.len() {
            p.vline(dot.x, dot.y..=rect.bottom() + ui.spacing().item_spacing.y + ROW / 2.0, (1.0, EDGE));
        }
        p.circle_filled(dot, 3.5, if i == 0 { ACCENT } else { EDGE });
        let x = rect.left() + RAIL + INLINE;
        let shown = p.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            time,
            egui::FontId::monospace(12.0),
            if i == 0 { TEXT } else { DIM },
        );
        p.text(
            egui::pos2(shown.right() + INLINE, rect.center().y),
            egui::Align2::LEFT_CENTER,
            why,
            egui::FontId::proportional(11.5),
            DIM,
        );
        p.text(
            egui::pos2(rect.right(), rect.center().y),
            egui::Align2::RIGHT_CENTER,
            age,
            egui::FontId::proportional(11.5),
            DIM,
        );
    }
}

impl Panel {
    /// The save backups (backup.rs): a timeline of the newest, a backup now, the folder.
    pub(super) fn backups_card(&mut self, t: &mut Tui) {
        // Listed once a second at most: it reads the folder.
        if self.backups_read.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
            self.backups = crate::backup::list();
            self.backups_read = Some(Instant::now());
        }
        card(t, tr!("SAVE_BACKUPS"), |t| {
            note(t, trf!("COPIES_THE_SAVE_FILES_EACH_TIME", count = crate::backup::KEEP));
            if self.backups.is_empty() {
                text(t, RichText::new(tr!("NO_BACKUPS_YET")).color(DIM).small());
            } else {
                let rows: Vec<(String, String, String)> = self
                    .backups
                    .iter()
                    .take(SHOWN)
                    .map(|(name, path)| {
                        let (time, why) = when_and_why(name);
                        let age = path.metadata().and_then(|m| m.modified()).map(ago).unwrap_or_default();
                        (time, why, age)
                    })
                    .collect();
                block(t, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    timeline(ui, &rows);
                });
            }
            choices(t, |t| {
                if w(t, |ui| ui.button(tr!("BACK_UP_NOW"))).clicked() {
                    self.reply = Some(match crate::backup::make("manual") {
                        Ok(to) => (
                            true,
                            trf!("BACKED_UP", file = to.file_name().unwrap_or_default().to_string_lossy()),
                            Instant::now(),
                        ),
                        Err(e) => (false, trf!("BACKUP_FAILED", e = e), Instant::now()),
                    });
                    self.backups_read = None;
                }
                // How to roll back is what the folder is opened for: its hover.
                let open = w(t, |ui| ui.button(tr!("OPEN_FOLDER")).on_hover_text(tr!("TO_ROLL_BACK_QUIT_THE_GAME")));
                if open.clicked() {
                    let dir = crate::backup::dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            });
        });
    }

    /// The game's own save files, newest first: the name, when it was last written
    /// (dim), and its size at the right edge in monospace so the sizes line up.
    pub(super) fn slots_card(&mut self, t: &mut Tui) {
        card(t, tr!("SAVE_FILES"), |t| {
            let Some(dir) = crate::backup::saves() else {
                note(t, tr!("THE_GAMES_SAVE_FOLDER_WAS_NOT"));
                return;
            };
            let mut files: Vec<(String, std::time::SystemTime, u64)> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("sav")))
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    Some((e.file_name().to_string_lossy().to_string(), m.modified().ok()?, m.len()))
                })
                .collect();
            files.sort_by_key(|f| std::cmp::Reverse(f.1));
            for (name, at, len) in files {
                t.style(tw::row(INLINE)).add(|t| {
                    block(t, |ui| {
                        ui.add(egui::Label::new(name.trim_end_matches(".sav")).truncate());
                    });
                    w(t, |ui| ui.label(RichText::new(ago(at)).color(DIM).small()));
                    w(t, |ui| ui.label(RichText::new(format!("{:>6} KB", len / 1024)).monospace().color(TEXT)));
                });
            }
            note(t, dir.display().to_string());
        });
    }
}
