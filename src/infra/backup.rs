//! Save backups: whenever the game writes a save, a copy of every save file goes to
//! `Mods\backups\<when>\` — so a turn-in that broke a save (the "Man of the People"
//! report), a keystone placed too early or a good deed failed can be undone by
//! copying the files back. The newest `KEEP` copies are kept.
//!
//! The saves are `%LOCALAPPDATA%\HellIsUs\Saved\SaveGames\*.sav` (the profile and one
//! file per slot); a write is seen as a newer modified time on any of them.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How many backups are kept.
pub const KEEP: usize = 20;

/// The game's save folder.
pub fn saves() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let dir = PathBuf::from(local).join("HellIsUs").join("Saved").join("SaveGames");
    dir.is_dir().then_some(dir)
}

/// Where the backups go.
pub fn dir() -> PathBuf {
    crate::paths::data_dir().join("backups")
}

fn save_files(saves: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(saves) else { return Vec::new() };
    let mut out: Vec<PathBuf> =
        read.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sav"))).collect();
    out.sort();
    out
}

/// The newest write among the save files.
pub fn newest(saves: &Path) -> Option<SystemTime> {
    save_files(saves).iter().filter_map(|p| p.metadata().ok()?.modified().ok()).max()
}

/// Copy every save file into a new backup folder named by the local time (and a
/// reason, when there is one); then drop the oldest past `KEEP`. The folder made.
pub fn make(reason: &str) -> Result<PathBuf, String> {
    let saves = saves().ok_or("the game's save folder was not found")?;
    let files = save_files(&saves);
    if files.is_empty() {
        return Err("no save files to back up".into());
    }
    let mut name = crate::logfile::stamp();
    if !reason.is_empty() {
        name.push('_');
        name.push_str(&reason.replace(|c: char| !c.is_alphanumeric() && c != '-', "_"));
    }
    let to = dir().join(name);
    std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
    for f in &files {
        let target = to.join(f.file_name().unwrap_or_default());
        std::fs::copy(f, &target).map_err(|e| format!("{}: {e}", f.display()))?;
    }
    prune(KEEP);
    Ok(to)
}

/// The backups, newest first: (folder name, path).
pub fn list() -> Vec<(String, PathBuf)> {
    let Ok(read) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut out: Vec<(String, PathBuf)> = read
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .collect();
    // Names start with the time, so they sort by it.
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

fn prune(keep: usize) {
    for (_, path) in list().into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// Watches the save folder and backs it up after each write: polled, every few
/// seconds, from its own thread; a write is backed up once it has settled (no change
/// for a moment), so a save half-written is never copied.
pub struct Watch {
    seen: Option<SystemTime>,
    changed: Option<std::time::Instant>,
}

impl Default for Watch {
    fn default() -> Watch {
        Watch { seen: saves().and_then(|s| newest(&s)), changed: None }
    }
}

impl Watch {
    /// Look once; back up when a write has settled. What was backed up, if anything.
    pub fn poll(&mut self) -> Option<Result<PathBuf, String>> {
        let now = saves().and_then(|s| newest(&s));
        if now != self.seen {
            self.seen = now;
            self.changed = Some(std::time::Instant::now());
            return None;
        }
        let settled = self.changed.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(3));
        if settled {
            self.changed = None;
            return Some(make(""));
        }
        None
    }
}
