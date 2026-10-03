//! The game data the guide reads — `Mods\survey` (the maps) and `Mods\locale` (the
//! game's text) — made on this machine without the player typing `doctor survey`
//! (.spec/SURVEY.md §8).
//!
//! Once the hero is in control and the .NET 8 runtime is there (runtime.rs), the panel
//! asks `next`: the survey when there is none or it was made on another Steam build
//! (`Mods\survey\BUILD`), else the text when it is missing. It runs as this same
//! executable, `doctor survey|locale`, in the background; when it ends, `generation`
//! moves on and the guide reads the new files.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Survey,
    Locale,
    /// Only the game's own tables (`doctor tables`): what a survey made before a table
    /// existed lacks.
    Tables,
}

impl Kind {
    fn arg(self) -> &'static str {
        match self {
            Kind::Survey => "survey",
            Kind::Locale => "locale",
            Kind::Tables => "tables",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    Idle,
    Running(Kind),
    Done,
    Failed(Kind, String),
}

static RUN: Mutex<Run> = Mutex::new(Run::Idle);
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn state() -> Run {
    RUN.lock().unwrap().clone()
}

/// Moves on each time new game data was written; the guide reloads when it has.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

fn survey_dir() -> PathBuf {
    crate::paths::data_dir().join("survey")
}

/// What is missing for Steam build `build`, if anything.
pub fn next(build: &str) -> Option<Kind> {
    needed(&survey_dir(), &crate::i18n::names::dir(), build)
}

fn needed(survey: &Path, locale: &Path, build: &str) -> Option<Kind> {
    let has_json = std::fs::read_dir(survey)
        .map(|d| d.flatten().any(|e| e.path().extension().is_some_and(|x| x == "json")))
        .unwrap_or(false);
    let stamp = std::fs::read_to_string(survey.join("BUILD")).ok().map(|s| s.trim().to_string());
    match (has_json, stamp) {
        (false, _) => return Some(Kind::Survey),
        (true, Some(s)) if s != build => return Some(Kind::Survey),
        // Made before stamps were written: taken as this build's, and stamped.
        (true, None) => {
            let _ = std::fs::write(survey.join("BUILD"), build);
        }
        _ => {}
    }
    // facts.tsv came later (the clue board): a locale without it is read again.
    if !locale.join("names.tsv").exists() || !locale.join("facts.tsv").exists() {
        return Some(Kind::Locale);
    }
    // recipes.json came later (the shard budget): only the tables are read for it.
    (!survey.join("recipes.json").exists()).then_some(Kind::Tables)
}

/// Run `doctor <kind>` in the background (one at a time); a survey that succeeds is
/// stamped with `build`.
pub fn start(kind: Kind, build: &str) {
    {
        let mut r = RUN.lock().unwrap();
        if matches!(*r, Run::Running(_)) {
            return;
        }
        *r = Run::Running(kind);
    }
    let build = build.to_string();
    std::thread::spawn(move || {
        let result = std::env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|exe| {
                std::process::Command::new(exe)
                    .args(["doctor", kind.arg()])
                    .creation_flags(NO_WINDOW)
                    .stdin(std::process::Stdio::null())
                    .output()
                    .map_err(|e| e.to_string())
            })
            .and_then(|out| {
                crate::logfile::line(&format!("game data: doctor {} → {}", kind.arg(), out.status));
                if out.status.success() {
                    Ok(())
                } else {
                    let text = String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
                    Err(text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim().to_string())
                }
            });
        if result.is_ok() && kind == Kind::Survey {
            let _ = std::fs::write(survey_dir().join("BUILD"), &build);
        }
        GENERATION.fetch_add(1, Ordering::Relaxed);
        *RUN.lock().unwrap() = match result {
            Ok(()) => Run::Done,
            Err(e) => Run::Failed(kind, e),
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("hiumod-gamedata-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (s, l) = (root.join("survey"), root.join("locale"));
        std::fs::create_dir_all(&s).unwrap();
        std::fs::create_dir_all(&l).unwrap();
        (s, l)
    }

    #[test]
    fn no_survey_or_another_build_means_survey() {
        let (s, l) = dirs("a");
        assert_eq!(needed(&s, &l, "100"), Some(Kind::Survey));
        std::fs::write(s.join("Jeljin.json"), "{}").unwrap();
        std::fs::write(s.join("BUILD"), "99").unwrap();
        assert_eq!(needed(&s, &l, "100"), Some(Kind::Survey));
    }

    #[test]
    fn a_current_survey_without_text_means_locale_then_nothing() {
        let (s, l) = dirs("b");
        std::fs::write(s.join("Jeljin.json"), "{}").unwrap();
        // no stamp yet: taken as this build's
        assert_eq!(needed(&s, &l, "100"), Some(Kind::Locale));
        assert_eq!(std::fs::read_to_string(s.join("BUILD")).unwrap(), "100");
        std::fs::write(l.join("names.tsv"), "").unwrap();
        assert_eq!(needed(&s, &l, "100"), Some(Kind::Locale), "a locale from before facts.tsv");
        std::fs::write(l.join("facts.tsv"), "").unwrap();
        assert_eq!(needed(&s, &l, "100"), Some(Kind::Tables), "a survey from before recipes.json");
        std::fs::write(s.join("recipes.json"), "[]").unwrap();
        assert_eq!(needed(&s, &l, "100"), None);
    }
}
