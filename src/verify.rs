//! What the player found when they tried each cheat: `verify.txt` in the mod's
//! data folder (paths.rs).
//!
//! The panel's debug tab writes it, one `id ok` or `id fail` per line; `doctor`
//! prints it beside each cheat. It is a report, not a switch — a cheat becomes
//! `verified` in the table only when someone reads the report and changes the row.

use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::hold::default_path().with_file_name("verify.txt")
}

pub type Marks = BTreeMap<String, bool>;

pub fn parse(text: &str) -> Marks {
    text.lines()
        .filter_map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [id, "ok"] => Some((id.to_string(), true)),
            [id, "fail"] => Some((id.to_string(), false)),
            _ => None,
        })
        .collect()
}

pub fn render(marks: &Marks) -> String {
    marks.iter().map(|(id, ok)| format!("{id} {}\n", if *ok { "ok" } else { "fail" })).collect()
}

pub fn load() -> Marks {
    std::fs::read_to_string(path()).map(|t| parse(&t)).unwrap_or_default()
}

pub fn save(marks: &Marks) -> Result<(), String> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, render(marks)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_skips_what_it_does_not_understand() {
        let m = parse("jump ok\ncrit fail\nnonsense\nloot maybe\n");
        assert_eq!(m.get("jump"), Some(&true));
        assert_eq!(m.get("crit"), Some(&false));
        assert_eq!(m.len(), 2);
        assert_eq!(parse(&render(&m)), m);
    }
}
