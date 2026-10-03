//! Where the mod keeps its files: beside the game, in `<install root>\Mods\`.
//!
//! Beside `HellIsUs\`, not inside it: Steam's "verify integrity" checks only the
//! files it shipped, so a `Mods\` folder at the install root survives updates and
//! repairs, and goes when the game is uninstalled.
//!
//! Where the game cannot be found, or its folder cannot be written, the files fall
//! back to `%LOCALAPPDATA%\hiumod\`. Files left there are moved across the first
//! time the game folder is usable.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FILES: [&str; 4] = ["settings.txt", "verify.txt", "originals.txt", "hiumod.log"];

pub fn fallback() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    root.join("hiumod")
}

/// The install root, found through Steam's libraries (locate.rs).
#[cfg(windows)]
fn game_root() -> Option<PathBuf> {
    crate::game::locate::find(None).ok()?.root().map(Path::to_path_buf)
}

#[cfg(not(windows))]
fn game_root() -> Option<PathBuf> {
    None
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(".hiumod-write-test");
    let ok = std::fs::create_dir_all(dir).is_ok() && std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Move what `from` holds into `to`, file by file — never over a file `to` already
/// has, which is newer by definition. Returns what moved.
pub fn migrate(from: &Path, to: &Path) -> Vec<&'static str> {
    let mut moved = Vec::new();
    for f in FILES {
        let (src, dst) = (from.join(f), to.join(f));
        if src.is_file() && !dst.exists() && (std::fs::rename(&src, &dst).is_ok() || copy_then_remove(&src, &dst)) {
            moved.push(f);
        }
    }
    let _ = std::fs::remove_dir(from); // only if now empty
    moved
}

/// `rename` cannot cross drives; the game may be on another one.
fn copy_then_remove(src: &Path, dst: &Path) -> bool {
    std::fs::copy(src, dst).is_ok() && std::fs::remove_file(src).is_ok()
}

/// Decided once per run, so every file this run writes lands in one place.
pub fn data_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let fallback = fallback();
        let root = game_root();
        match root.as_ref().map(|r| r.join("Mods")) {
            Some(dir) if writable(&dir) => {
                if fallback.is_dir() {
                    migrate(&fallback, &dir);
                }
                dir
            }
            _ => fallback,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_old_files_but_never_over_new_ones() {
        let base = std::env::temp_dir().join(format!("hiumod-paths-{}", std::process::id()));
        let (from, to) = (base.join("old"), base.join("new"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(from.join("settings.txt"), "keep false\n").unwrap();
        std::fs::write(from.join("verify.txt"), "old\n").unwrap();
        std::fs::write(to.join("verify.txt"), "new\n").unwrap();

        assert_eq!(migrate(&from, &to), ["settings.txt"]);
        assert_eq!(std::fs::read_to_string(to.join("settings.txt")).unwrap(), "keep false\n");
        assert_eq!(std::fs::read_to_string(to.join("verify.txt")).unwrap(), "new\n", "kept the newer");
        assert!(from.join("verify.txt").exists(), "what did not move stays where it was");
        let _ = std::fs::remove_dir_all(&base);
    }
}
