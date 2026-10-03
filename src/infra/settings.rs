//! What the panel remembers between runs: `settings.txt` in the mod's data folder
//! (`<install root>\Mods\`, see paths.rs).
//!
//! Which cheats were on and at what value, every slider's last value, the tab that
//! was open, where the panel was. With `keep` on, the cheats that were on come back
//! on by themselves — once the game is up and the solo gate is open, never before.
//!
//! Plain lines, one fact each, so a hand edit is easy and a broken line costs only
//! itself:
//!
//! ```text
//! keep true
//! tab combat
//! pos 120 80
//! on god
//! on speed 2.5
//! value damage 5
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    crate::paths::data_dir().join("settings.txt")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Turn the cheats that were on back on next time.
    pub keep: bool,
    pub tab: Option<String>,
    /// The panel's top-left, in screen pixels.
    pub pos: Option<(i32, i32)>,
    /// Cheats that were on, with their slider value (0 for a toggle).
    pub on: Vec<(String, f32)>,
    /// Every slider's last value, on or not.
    pub values: BTreeMap<String, f32>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { keep: true, tab: None, pos: None, on: Vec::new(), values: BTreeMap::new() }
    }
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |x: &str| x.parse::<f32>().ok().filter(|v| v.is_finite());
        match f[..] {
            ["keep", v] => s.keep = v == "true",
            ["tab", t] => s.tab = Some(t.to_string()),
            ["pos", x, y] => s.pos = x.parse().ok().zip(y.parse().ok()),
            ["on", id] => s.on.push((id.to_string(), 0.0)),
            ["on", id, v] => {
                if let Some(v) = num(v) {
                    s.on.push((id.to_string(), v));
                }
            }
            ["value", id, v] => {
                if let Some(v) = num(v) {
                    s.values.insert(id.to_string(), v);
                }
            }
            _ => {}
        }
    }
    s
}

pub fn render(s: &Settings) -> String {
    let mut out = format!("keep {}\n", s.keep);
    if let Some(t) = &s.tab {
        out += &format!("tab {t}\n");
    }
    if let Some((x, y)) = s.pos {
        out += &format!("pos {x} {y}\n");
    }
    for (id, v) in &s.on {
        out += &if *v == 0.0 { format!("on {id}\n") } else { format!("on {id} {v}\n") };
    }
    for (id, v) in &s.values {
        out += &format!("value {id} {v}\n");
    }
    out
}

pub fn load() -> Settings {
    std::fs::read_to_string(path()).map(|t| parse(&t)).unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), String> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, render(s)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut s = Settings { keep: false, tab: Some("combat".into()), pos: Some((-1920, 40)), ..Settings::default() };
        s.on = vec![("god".into(), 0.0), ("speed".into(), 2.5)];
        s.values.insert("damage".into(), 5.0);
        assert_eq!(parse(&render(&s)), s);
    }

    #[test]
    fn a_broken_line_costs_only_itself() {
        let s = parse("keep true\non speed fast\npos 1\non god\nvalue jump NaN\nvalue gravity 0.5\n???\n");
        assert_eq!(s.on, vec![("god".to_string(), 0.0)]);
        assert_eq!(s.pos, None);
        assert_eq!(s.values.get("gravity"), Some(&0.5));
        assert!(!s.values.contains_key("jump"));
    }

    #[test]
    fn nothing_saved_means_defaults() {
        assert_eq!(parse(""), Settings::default());
    }
}
