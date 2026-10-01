//! Start the game the way the Steam library does: through Steam, by app id.
//!
//! Running the executable directly works too (the game has no DRM wrapper), but it
//! skips Steam's overlay and cloud sync; asking Steam keeps the player's normal
//! setup.

use super::locate::APP_ID;

pub fn launch() -> Result<(), String> {
    // explorer.exe hands the URL to Steam and exits at once whatever happens, so its
    // status says nothing; whether the game came up is for the caller to watch.
    // Its stdio is null, not inherited: the panel has given its console up by now,
    // and handles to a console that is gone make the spawn fail.
    use std::process::Stdio;
    std::process::Command::new("explorer.exe")
        .arg(format!("steam://rungameid/{APP_ID}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("cannot start the game: {e}"))
}
