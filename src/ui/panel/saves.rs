//! The save page: the backups and the game's own save files.

use super::*;

impl Panel {
    /// The save backups (backup.rs): the newest, a backup now, the folder.
    pub(super) fn backups_card(&mut self, t: &mut Tui) {
        // Listed once a second at most: it reads the folder.
        if self.backups_read.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
            self.backups = crate::backup::list();
            self.backups_read = Some(Instant::now());
        }
        card(t, tr!("SAVE_BACKUPS"), |t| {
            note(t, trf!("COPIES_THE_SAVE_FILES_EACH_TIME", count = crate::backup::KEEP));
            for (name, _) in self.backups.iter().take(3) {
                text(t, RichText::new(name.replace('_', " ")).color(DIM).small());
            }
            if self.backups.is_empty() {
                text(t, RichText::new(tr!("NO_BACKUPS_YET")).color(DIM).small());
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
                if w(t, |ui| ui.button(tr!("OPEN_FOLDER"))).clicked() {
                    let dir = crate::backup::dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            });
            note(t, tr!("TO_ROLL_BACK_QUIT_THE_GAME"));
        });
    }

    /// The game's own save files and when each was last written.
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
                let ago = at.elapsed().map_or(0, |d| d.as_secs());
                let when = match ago {
                    0..60 => tr!("JUST_NOW").to_string(),
                    60..3600 => trf!("MIN_AGO", minutes = ago / 60),
                    3600..86400 => trf!("H_AGO", hours = ago / 3600),
                    _ => trf!("DAYS_AGO", days = ago / 86400),
                };
                field(t, name.trim_end_matches(".sav"), |t| {
                    text(t, RichText::new(format!("{when} · {} KB", len / 1024)).color(DIM))
                });
            }
            note(t, dir.display().to_string());
        });
    }
}
