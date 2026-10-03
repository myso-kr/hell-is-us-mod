//! The .NET 8 runtime the survey tool needs (tools/survey, framework-dependent), found
//! or installed (.spec/GUIDE.md §34).
//!
//! Installing is the player's choice (a button in the panel), and goes the way most
//! Windows tools fetch a prerequisite:
//!
//! 1. **winget** — Microsoft's own `Microsoft.DotNet.Runtime.8` package, machine-wide,
//!    kept up to date by Windows Update; Windows asks for an administrator's consent.
//! 2. When winget is missing, refused or fails: Microsoft's **dotnet-install** script
//!    (`dot.net/v1/dotnet-install.ps1`) into `Mods\dotnet` — this player only, no
//!    administrator, nothing on PATH, gone with the Mods folder.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

/// `CREATE_NO_WINDOW`: no console flashing up over the game.
const NO_WINDOW: u32 = 0x0800_0000;

/// Where the per-player runtime goes.
pub fn private_dir() -> PathBuf {
    crate::paths::data_dir().join("dotnet")
}

/// A `dotnet` host that has `Microsoft.NETCore.App 8.x`: on PATH, where winget puts it
/// (this process's PATH predates an install it just ran), or the per-player one.
pub fn dotnet() -> Option<PathBuf> {
    let mut hosts = vec![PathBuf::from("dotnet")];
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        hosts.push(PathBuf::from(pf).join("dotnet").join("dotnet.exe"));
    }
    hosts.push(private_dir().join("dotnet.exe"));
    hosts.into_iter().find(|h| has_8(h.as_path()))
}

fn has_8(host: &std::path::Path) -> bool {
    Command::new(host)
        .arg("--list-runtimes")
        .creation_flags(NO_WINDOW)
        .output()
        .is_ok_and(|o| runtimes_have_8(&String::from_utf8_lossy(&o.stdout)))
}

/// `dotnet --list-runtimes` names a .NET 8 base runtime.
fn runtimes_have_8(listing: &str) -> bool {
    listing.lines().any(|l| l.starts_with("Microsoft.NETCore.App 8."))
}

/// Whether a .NET 8 runtime is there, for a caller that cannot wait on a process
/// (the panel, each frame): the last answer, asked again in the background every
/// 15 s until it is yes. `None` before the first answer.
pub fn available() -> Option<bool> {
    static SEEN: Mutex<(Option<bool>, Option<std::time::Instant>, bool)> = Mutex::new((None, None, false));
    let mut s = SEEN.lock().unwrap();
    let stale = s.1.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(15));
    if s.0 != Some(true) && stale && !s.2 {
        s.2 = true;
        std::thread::spawn(|| {
            let yes = dotnet().is_some();
            let mut s = SEEN.lock().unwrap();
            *s = (Some(yes), Some(std::time::Instant::now()), false);
        });
    }
    s.0
}

/// Where an install stands, for the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Install {
    Idle,
    Winget,
    Script,
    Done,
    Failed(String),
}

static INSTALL: Mutex<Install> = Mutex::new(Install::Idle);

pub fn install_state() -> Install {
    INSTALL.lock().unwrap().clone()
}

/// Install in the background (once at a time): winget, then the script.
pub fn install() {
    {
        let mut s = INSTALL.lock().unwrap();
        if matches!(*s, Install::Winget | Install::Script) {
            return;
        }
        *s = Install::Winget;
    }
    std::thread::spawn(|| {
        let set = |s: Install| *INSTALL.lock().unwrap() = s;
        let winget = Command::new("winget")
            .args([
                "install",
                "--id",
                "Microsoft.DotNet.Runtime.8",
                "--exact",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity",
            ])
            .creation_flags(NO_WINDOW)
            .status();
        crate::logfile::line(&format!("runtime: winget → {winget:?}"));
        if dotnet().is_some() {
            return set(Install::Done);
        }
        set(Install::Script);
        match script() {
            Ok(()) if dotnet().is_some() => set(Install::Done),
            Ok(()) => set(Install::Failed("dotnet-install finished without a .NET 8 runtime".into())),
            Err(e) => set(Install::Failed(e)),
        }
    });
}

/// Microsoft's dotnet-install script, the runtime only, into `Mods\dotnet`.
fn script() -> Result<(), String> {
    let dir = private_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ps1 = dir.join("dotnet-install.ps1");
    let quote = |p: &PathBuf| p.display().to_string().replace('\'', "''");
    let command = format!(
        "$ProgressPreference='SilentlyContinue'; \
         Invoke-WebRequest -UseBasicParsing https://dot.net/v1/dotnet-install.ps1 -OutFile '{ps1}'; \
         & '{ps1}' -Runtime dotnet -Channel 8.0 -InstallDir '{dir}' -NoPath",
        ps1 = quote(&ps1),
        dir = quote(&dir),
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    crate::logfile::line(&format!("runtime: dotnet-install → {}", out.status));
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().find(|l| !l.trim().is_empty()).unwrap_or("dotnet-install failed").trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_base_runtime_8_counts() {
        let listing = "Microsoft.AspNetCore.App 8.0.31 [C:\\x]\nMicrosoft.NETCore.App 8.0.31 [C:\\y]\n";
        assert!(runtimes_have_8(listing));
        assert!(!runtimes_have_8("Microsoft.NETCore.App 9.0.1 [C:\\y]\nMicrosoft.AspNetCore.App 8.0.31 [C:\\x]"));
        assert!(!runtimes_have_8(""));
    }
}
