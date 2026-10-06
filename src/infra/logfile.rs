//! A log file beside the restore record: `hiumod.log` in the mod's data folder.
//!
//! The panel has no console, so what it did — each write asked for, each refusal,
//! the gate opening and closing — is kept here with the time it happened. It is
//! cut back to its last half when it passes 1 MiB, so it never needs looking after.

use std::io::Write;
use std::path::PathBuf;

const LIMIT: u64 = 1 << 20;

pub fn path() -> PathBuf {
    crate::paths::data_dir().join("hiumod.log")
}

#[cfg(windows)]
fn now() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut t = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

#[cfg(not(windows))]
fn now() -> String {
    String::new()
}

/// The local time for a file or folder name: `2026-10-03_07-43-09`.
pub fn stamp() -> String {
    now().replace(' ', "_").replace(':', "-")
}

/// Panics written here before the process ends (a release build aborts on one, and the panel has
/// no console to show it): where, and what it said.
pub fn catch_panics() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info.location().map_or(String::new(), |l| format!("{}:{}", l.file(), l.line()));
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        line(&format!("PANIC in {thread} at {at}: {what}"));
        before(info);
    }));
}

/// Append one line. Never fails loudly: a log that cannot be written must not stop
/// the thing it was logging.
pub fn line(text: &str) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LIMIT) {
        if let Ok(old) = std::fs::read_to_string(&path) {
            let keep = &old[old.len() / 2..];
            let keep = keep.find('\n').map_or(keep, |i| &keep[i + 1..]);
            let _ = std::fs::write(&path, keep);
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {text}", now());
    }
}
