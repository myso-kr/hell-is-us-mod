//! Where the last session left off (.spec/JOURNEY.md §3.8): the panel writes
//! `Mods\session.txt` every half minute while the hero is in play, and reads the previous
//! one once at start, before writing over it — the "previously" card.

use std::path::Path;

/// One session's last state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    /// Seconds since the Unix epoch.
    pub when: u64,
    pub world: String,
    /// The followed quest's journal key, if one was followed.
    pub quest: Option<String>,
    /// Where the hero stood (cm).
    pub at: [f32; 3],
}

impl Session {
    fn parse(text: &str) -> Option<Session> {
        let mut s = Session::default();
        for line in text.lines() {
            let (k, v) = line.split_once('\t')?;
            match k {
                "when" => s.when = v.parse().ok()?,
                "world" => s.world = v.to_string(),
                "quest" if !v.is_empty() => s.quest = Some(v.to_string()),
                "at" => {
                    let p: Vec<f32> = v.split(' ').filter_map(|x| x.parse().ok()).collect();
                    s.at = [*p.first()?, *p.get(1)?, *p.get(2)?];
                }
                _ => {}
            }
        }
        (s.when > 0 && !s.world.is_empty()).then_some(s)
    }

    fn render(&self) -> String {
        format!(
            "when\t{}\nworld\t{}\nquest\t{}\nat\t{} {} {}\n",
            self.when,
            self.world,
            self.quest.as_deref().unwrap_or(""),
            self.at[0],
            self.at[1],
            self.at[2]
        )
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn path() -> std::path::PathBuf {
    crate::paths::data_dir().join("session.txt")
}

pub fn read() -> Option<Session> {
    read_from(&path())
}

pub fn write(s: &Session) {
    let _ = std::fs::write(path(), s.render());
}

fn read_from(p: &Path) -> Option<Session> {
    Session::parse(&std::fs::read_to_string(p).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_round_trips() {
        let s = Session {
            when: 1_790_000_000,
            world: "AcasaMarshes".into(),
            quest: Some("Quest01".into()),
            at: [1.5, -2.0, 300.0],
        };
        assert_eq!(Session::parse(&s.render()), Some(s.clone()));
        let none = Session { quest: None, ..s };
        assert_eq!(Session::parse(&none.render()), Some(none));
    }

    #[test]
    fn a_broken_or_empty_file_is_no_session() {
        assert_eq!(Session::parse(""), None);
        assert_eq!(Session::parse("when\tx\nworld\tA\n"), None);
        assert_eq!(Session::parse("world\tA\n"), None, "no time");
    }
}
