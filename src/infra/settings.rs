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

/// What the player agreed the mod may show or change, one bit each (.spec/CONSENT.md):
/// the map and the guide, where hidden things are, answers and spoilers, cheats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Consent(pub u8);

/// The consent now, for the worker (which builds the guide's goals and does not see the
/// panel): set by the panel whenever it changes.
pub static LIVE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Accessibility (the Map page's tab, kept in the map settings): colours told apart by
/// colour-blind players (Okabe–Ito), and the overlays' cards and text on solid backs. Set by
/// the overlay and the panel from the map settings; read wherever a colour is chosen.
pub static SAFE_COLOURS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub static HIGH_CONTRAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn safe_colours() -> bool {
    SAFE_COLOURS.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn high_contrast() -> bool {
    HIGH_CONTRAST.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether the consent now grants `bit`.
pub fn live(bit: u8) -> bool {
    LIVE.load(std::sync::atomic::Ordering::Relaxed) & bit != 0
}

impl Consent {
    /// The maps: minimap, big map, compass, trail, pins.
    pub const MAP: u8 = 1;
    /// Where hidden things are: on the maps, in the lists, guided to.
    pub const PLACES: u8 = 2;
    /// Puzzles' answers, and guiding that would give one away (a slot, an order).
    pub const ANSWERS: u8 = 4;
    pub const CHEATS: u8 = 8;
    /// The guide: the story's next place, routes, rings, the Guide page.
    pub const GUIDE: u8 = 16;
    /// What must come first: the guide goes to the key, the lever, the drain first.
    pub const STEPS: u8 = 32;
    /// A warning before something is missed for good: the banner, the deadlines.
    pub const MISSABLES: u8 = 64;
    /// Panels over the game: the tracker, its context lines, the banner.
    pub const HUD: u8 = 128;
    pub const ALL: u8 = 255;
    /// Their names in settings.txt.
    const NAMES: [(u8, &'static str); 8] = [
        (Self::MAP, "map"),
        (Self::PLACES, "places"),
        (Self::ANSWERS, "answers"),
        (Self::CHEATS, "cheats"),
        (Self::GUIDE, "guide"),
        (Self::STEPS, "steps"),
        (Self::MISSABLES, "missables"),
        (Self::HUD, "hud"),
    ];

    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }
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
    /// What the player agreed to; `None` until they chose (then nothing is shown).
    pub consent: Option<Consent>,
    /// The panel's backdrop moves (contours drifting, a route finding its way).
    pub motion: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            keep: true,
            tab: None,
            pos: None,
            on: Vec::new(),
            values: BTreeMap::new(),
            consent: None,
            motion: true,
        }
    }
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |x: &str| x.parse::<f32>().ok().filter(|v| v.is_finite());
        match f[..] {
            ["keep", v] => s.keep = v == "true",
            ["motion", v] => s.motion = v == "true",
            ["consent", ref granted @ ..] => {
                let mut bits =
                    Consent::NAMES.iter().filter(|(_, n)| granted.contains(n)).fold(0, |b, (bit, _)| b | bit);
                // Written before there were eight (no `v2`): what was the map's goes on as the
                // guide's and the HUD's, the answers' as the steps' and the missables'.
                if !granted.contains(&"v2") {
                    let had = bits;
                    if had & Consent::MAP != 0 {
                        bits |= Consent::GUIDE | Consent::HUD;
                    }
                    if had & Consent::ANSWERS != 0 {
                        bits |= Consent::STEPS | Consent::MISSABLES;
                    }
                }
                s.consent = Some(Consent(bits));
            }
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
    let mut out = format!("keep {}\nmotion {}\n", s.keep, s.motion);
    if let Some(c) = s.consent {
        let mut names: Vec<&str> = vec!["v2"];
        names.extend(Consent::NAMES.iter().filter(|(b, _)| c.has(*b)).map(|(_, n)| *n));
        out += &format!("consent {}\n", names.join(" ")).replace(" \n", "\n");
    }
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
