//! Find the Steam install and read which build it is.
//!
//! Steam keeps each library's games under `<library>\steamapps\common\<installdir>`
//! and describes each one in `<library>\steamapps\appmanifest_<appid>.acf`. The
//! manifest's `buildid` is the version this tool reports — it changes with every
//! patch Steam downloads, which is exactly when the game's memory may move.
//!
//! The libraries themselves are listed in `libraryfolders.vdf` under Steam's own
//! folder, which the registry names (`HKCU\Software\Valve\Steam\SteamPath`).

use std::path::{Path, PathBuf};

pub const APP_ID: u32 = 1620730;
/// Relative to the install root.
pub const EXE: &str = r"HellIsUs\Binaries\Win64\HellIsUs-Win64-Shipping.exe";

pub struct Install {
    /// `...\steamapps\common\Hell Is Us`.
    pub dir: PathBuf,
    pub exe: PathBuf,
    /// Steam's `buildid`, or `unknown` when there is no manifest beside the install.
    pub version: String,
}

impl Install {
    /// The folder the mod's own `Mods` folder goes in.
    pub fn root(&self) -> Option<&Path> {
        Some(&self.dir)
    }
}

/// `dir` is the install root (holding `HellIsUs\`). Without one, every Steam library
/// is searched.
pub fn find(dir: Option<&Path>) -> Result<Install, String> {
    let Some(dir) = dir else {
        return discover().ok_or_else(|| tr!("Steam 라이브러리에서 Hell Is Us 설치를 찾지 못함 — --game-dir 로 알려 주세요").into());
    };
    let exe = dir.join(EXE);
    if !exe.is_file() {
        return Err(trf!("{a0} 에 Hell Is Us 설치가 없음 — --game-dir 로 알려 주세요", a0 = dir.display()));
    }
    let version = dir
        .parent()
        .and_then(Path::parent)
        .and_then(|steamapps| std::fs::read_to_string(steamapps.join(format!("appmanifest_{APP_ID}.acf"))).ok())
        .and_then(|acf| vdf_value(&acf, "buildid"))
        .unwrap_or_else(|| "unknown".into());
    Ok(Install { dir: dir.to_path_buf(), exe, version })
}

/// The running executable's install: four directories up — `Win64`, `Binaries`,
/// `HellIsUs`, then the install root.
pub fn from_exe(exe: &Path) -> Result<Install, String> {
    let dir = exe.ancestors().nth(4).ok_or(tr!("예상과 다른 실행 파일 경로"))?;
    find(Some(dir))
}

/// The first `"key"  "value"` pair with this key, in Valve's KeyValues text format.
pub fn vdf_value(text: &str, key: &str) -> Option<String> {
    vdf_values(text, key).into_iter().next()
}

/// Every value given for `key`, in order.
pub fn vdf_values(text: &str, key: &str) -> Vec<String> {
    let quoted = format!("\"{key}\"");
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix(&quoted)?.trim();
            let v = rest.strip_prefix('"')?;
            Some(v[..v.find('"')?].replace("\\\\", "\\"))
        })
        .collect()
}

#[cfg(windows)]
fn steam_path() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, value) = (wide(r"Software\Valve\Steam"), wide("SteamPath"));
    let mut buf = [0u16; 520];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(0)]);
    Some(PathBuf::from(s.replace('/', "\\")))
}

#[cfg(not(windows))]
fn steam_path() -> Option<PathBuf> {
    None
}

/// Every Steam library: the registry's Steam folder, the default one, and every
/// path their `libraryfolders.vdf` lists.
fn libraries() -> Vec<PathBuf> {
    let mut steams: Vec<PathBuf> = steam_path().into_iter().collect();
    steams.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    let mut out = Vec::new();
    for s in steams {
        if let Ok(vdf) = std::fs::read_to_string(s.join("steamapps").join("libraryfolders.vdf")) {
            out.extend(vdf_values(&vdf, "path").into_iter().map(PathBuf::from));
        }
        out.push(s);
    }
    out.dedup();
    out
}

fn discover() -> Option<Install> {
    libraries().into_iter().find_map(|lib| {
        let acf = std::fs::read_to_string(lib.join("steamapps").join(format!("appmanifest_{APP_ID}.acf"))).ok()?;
        let dir = lib.join("steamapps").join("common").join(vdf_value(&acf, "installdir")?);
        find(Some(&dir)).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACF: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"1620730\"\n\t\"installdir\"\t\t\"Hell Is Us\"\n\t\"buildid\"\t\t\"24045435\"\n}\n";

    #[test]
    fn reads_values_from_a_manifest() {
        assert_eq!(vdf_value(ACF, "buildid").as_deref(), Some("24045435"));
        assert_eq!(vdf_value(ACF, "installdir").as_deref(), Some("Hell Is Us"));
        assert_eq!(vdf_value(ACF, "nope"), None);
    }

    #[test]
    fn reads_every_library_path_unescaped() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}";
        assert_eq!(vdf_values(vdf, "path"), [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
    }

    #[test]
    fn the_installed_game_is_found_without_a_path() {
        let Ok(i) = find(None) else {
            return; // no install here
        };
        assert!(i.exe.is_file());
        assert_ne!(i.version, "unknown");
    }
}
