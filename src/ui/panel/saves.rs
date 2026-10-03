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
        card(t, tr!("세이브 백업"), |t| {
            note(t, trf!("게임이 저장할 때마다 세이브 파일을 복사해 둡니다 (최근 {a0}개)", a0 = crate::backup::KEEP));
            for (name, _) in self.backups.iter().take(3) {
                text(t, RichText::new(name.replace('_', " ")).color(DIM).small());
            }
            if self.backups.is_empty() {
                text(t, RichText::new(tr!("아직 백업이 없습니다")).color(DIM).small());
            }
            choices(t, |t| {
                if w(t, |ui| ui.button(tr!("지금 백업"))).clicked() {
                    self.reply = Some(match crate::backup::make("manual") {
                        Ok(to) => (true, trf!("백업함: {a0}", a0 = to.file_name().unwrap_or_default().to_string_lossy()), Instant::now()),
                        Err(e) => (false, trf!("백업 실패: {e}", e = e), Instant::now()),
                    });
                    self.backups_read = None;
                }
                if w(t, |ui| ui.button(tr!("폴더 열기"))).clicked() {
                    let dir = crate::backup::dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            });
            note(t, tr!(r"되돌리려면 게임을 끈 뒤 백업 폴더의 .sav 파일을 세이브 폴더(%LOCALAPPDATA%\HellIsUs\Saved\SaveGames)에 덮어쓰세요"));
        });
    }

    /// The game's own save files and when each was last written.
    pub(super) fn slots_card(&mut self, t: &mut Tui) {
        card(t, tr!("세이브 파일"), |t| {
            let Some(dir) = crate::backup::saves() else {
                note(t, tr!("게임의 세이브 폴더를 찾지 못했습니다"));
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
                    0..60 => tr!("방금").to_string(),
                    60..3600 => trf!("{a0}분 전", a0 = ago / 60),
                    3600..86400 => trf!("{a0}시간 전", a0 = ago / 3600),
                    _ => trf!("{a0}일 전", a0 = ago / 86400),
                };
                field(t, name.trim_end_matches(".sav"), |t| text(t, RichText::new(format!("{when} · {} KB", len / 1024)).color(DIM)));
            }
            note(t, dir.display().to_string());
        });
    }
}
