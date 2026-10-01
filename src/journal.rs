//! A log file beside the restore record: `hiumod.log` in the mod's data folder.
//!
//! The panel has no console, so what it did — each write asked for, each refusal,
//! the gate opening and closing — is kept here with the time it happened. It is
//! cut back to its last half when it passes 1 MiB, so it never needs looking after.

use std::io::Write;
use std::path::PathBuf;

const LIMIT: u64 = 1 << 20;

pub fn path() -> PathBuf {
    crate::hold::default_path().with_file_name("hiumod.log")
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
