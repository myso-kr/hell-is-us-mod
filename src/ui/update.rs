//! Updating by hand from the project's GitHub releases: the panel's footer checks for a
//! newer release when asked, downloads its zip when asked, and applies it when asked.
//!
//! Windows' own `curl.exe` fetches and its `tar.exe` unpacks (both ship with Windows 10
//! 1803 and later), so the panel carries no HTTP or TLS code of its own. The zip is
//! checked against the release's `.sha256` before anything is unpacked.
//!
//! Applying: the running executable cannot be overwritten, but it can be renamed, so it
//! is moved aside (`*.update-old`, removed at the next start) and the new files put in
//! its place. The new panel is started with `HIUMOD_AFTER` set to this one's pid and
//! waits for it to exit (`wait_for_previous`) before it claims the one-panel mutex;
//! this one closes as × closes it, putting the game's values back first.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const REPO: &str = "myso-kr/hell-is-us-mod";
pub const PAGE: &str = "https://github.com/myso-kr/hell-is-us-mod";
/// Set on the panel an update starts: the pid it waits out.
const AFTER: &str = "HIUMOD_AFTER";
/// What a file moved aside by an update is called, after its own name.
const OLD: &str = ".update-old";

/// A release newer than this build.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    zip: String,
    sum: String,
    pub page: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Stage {
    #[default]
    Idle,
    Checking,
    Current,
    Found(Release),
    Fetching(String),
    /// Downloaded, checked and unpacked: the version and where its files are.
    Ready(String, PathBuf),
    Failed(String),
}

#[derive(Clone, Default)]
pub struct Updater {
    stage: Arc<Mutex<Stage>>,
}

impl Updater {
    pub fn stage(&self) -> Stage {
        self.stage.lock().unwrap().clone()
    }

    pub fn fail(&self, why: String) {
        *self.stage.lock().unwrap() = Stage::Failed(why);
    }

    /// In the background: is there a newer release?
    pub fn check(&self, ctx: &eframe::egui::Context) {
        self.run(ctx, Stage::Checking, |_| {
            let r = latest()?;
            Ok(if newer(&r.version, env!("CARGO_PKG_VERSION")) { Stage::Found(r) } else { Stage::Current })
        });
    }

    /// In the background: download, check and unpack `release`.
    pub fn fetch(&self, ctx: &eframe::egui::Context, release: Release) {
        self.run(ctx, Stage::Fetching(release.version.clone()), move |_| {
            let dir = fetch(&release)?;
            Ok(Stage::Ready(release.version, dir))
        });
    }

    fn run(
        &self,
        ctx: &eframe::egui::Context,
        busy: Stage,
        job: impl FnOnce(()) -> Result<Stage, String> + Send + 'static,
    ) {
        *self.stage.lock().unwrap() = busy;
        let (stage, ctx) = (self.stage.clone(), ctx.clone());
        std::thread::spawn(move || {
            let done = job(()).unwrap_or_else(|e| {
                crate::logfile::line(&format!("update: {e}"));
                Stage::Failed(e)
            });
            *stage.lock().unwrap() = done;
            ctx.request_repaint();
        });
    }
}

/// `curl.exe`, quietly (no console window), failing on HTTP errors.
fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("curl.exe")
        .stdin(std::process::Stdio::null())
        .args(["-fsSL", "--max-time", "120", "-A", concat!("hiumod/", env!("CARGO_PKG_VERSION"))])
        .args(args)
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| format!("curl.exe: {e}"))?;
    if !out.status.success() {
        return Err(format!("curl.exe: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

/// The latest release, from GitHub's API.
fn latest() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = curl(&["-H", "Accept: application/vnd.github+json", &url])?;
    release_of(&body)
}

fn release_of(json: &[u8]) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_slice(json).map_err(|e| format!("release: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("release: no tag")?;
    let asset = |suffix: &str| {
        v["assets"].as_array().and_then(|a| {
            a.iter()
                .find(|x| x["name"].as_str().is_some_and(|n| n.ends_with(suffix)))
                .and_then(|x| x["browser_download_url"].as_str())
                .map(str::to_string)
        })
    };
    Ok(Release {
        version: tag.trim_start_matches('v').to_string(),
        zip: asset(".zip").ok_or("release: no zip")?,
        sum: asset(".zip.sha256").ok_or("release: no checksum")?,
        page: v["html_url"].as_str().unwrap_or(PAGE).to_string(),
    })
}

/// Whether version `a` is newer than `b` (dotted numbers; anything else counts as 0).
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split(['.', '-']).take(3).map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
}

/// Download `release` into the data folder, check it, unpack it: the unpacked folder.
fn fetch(release: &Release) -> Result<PathBuf, String> {
    let dir = crate::paths::data_dir().join("update").join(&release.version);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let zip = dir.join("hiumod.zip");
    curl(&["-o", &zip.to_string_lossy(), &release.zip])?;
    let sum = String::from_utf8_lossy(&curl(&[&release.sum])?).to_string();
    let want = sum.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    let got = sha256(&std::fs::read(&zip).map_err(|e| e.to_string())?);
    if want.len() != 64 || want != got {
        return Err(format!("checksum: the release says {want}, the download is {got}"));
    }
    let files = dir.join("files");
    std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;
    {
        use std::os::windows::process::CommandExt;
        // Its own pipes, as curl's: the panel let its console go, so there are no handles
        // to inherit, and starting it with them failed ("not supported", os error 50).
        let out = std::process::Command::new("tar.exe")
            .arg("-xf")
            .arg(&zip)
            .arg("-C")
            .arg(&files)
            .stdin(std::process::Stdio::null())
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| format!("tar.exe: {e}"))?;
        if !out.status.success() {
            return Err(format!("tar.exe: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
    }
    if !files.join("hiumod.exe").is_file() {
        return Err("the zip has no hiumod.exe".into());
    }
    Ok(files)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Put the files in `staged` over this install and start the new panel; the caller then
/// closes this one.
pub fn apply(staged: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let install = exe.parent().ok_or("no install folder")?.to_path_buf();
    for from in walk(staged, usize::MAX) {
        let rel = from.strip_prefix(staged).map_err(|e| e.to_string())?;
        let to = install.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if to.exists() {
            // In use (this executable, a running survey): renamed aside works where
            // overwriting does not.
            let aside = PathBuf::from(format!("{}{OLD}", to.display()));
            let _ = std::fs::remove_file(&aside);
            std::fs::rename(&to, &aside).map_err(|e| format!("{}: {e}", to.display()))?;
        }
        std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    std::process::Command::new(&exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .env(AFTER, std::process::id().to_string())
        .current_dir(&install)
        .spawn()
        .map_err(|e| format!("start {}: {e}", exe.display()))?;
    Ok(())
}

/// The files under `dir`, down `depth` folders.
fn walk(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                out.extend(walk(&p, depth - 1));
            }
        } else {
            out.push(p);
        }
    }
    out
}

/// At start: wait for the panel this one replaces to exit (up to 15 s), and remove the
/// files the update moved aside.
pub fn after_update() {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    if let Some(pid) = std::env::var(AFTER).ok().and_then(|p| p.parse::<u32>().ok()) {
        std::env::remove_var(AFTER);
        unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if !h.is_null() {
                WaitForSingleObject(h, 15_000);
                CloseHandle(h);
            }
        }
    }
    if let Some(install) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        // The zip's folders are one deep; a build folder's are many, and not ours to sweep.
        for p in walk(&install, 1).into_iter().filter(|p| p.to_string_lossy().ends_with(OLD)) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Open `url` in the default browser.
pub fn open(url: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, url) = (wide("open"), wide(url));
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0.0", "0.99.0"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn a_release_is_read_from_the_api() {
        let json = br#"{"tag_name":"v0.2.0","html_url":"https://example/r","assets":[
            {"name":"hiumod-0.2.0.zip.sha256","browser_download_url":"https://example/s"},
            {"name":"hiumod-0.2.0.zip","browser_download_url":"https://example/z"}]}"#;
        let r = release_of(json).unwrap();
        assert_eq!(
            (r.version.as_str(), r.zip.as_str(), r.sum.as_str()),
            ("0.2.0", "https://example/z", "https://example/s")
        );
    }

    #[test]
    fn sha256_is_hex() {
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
