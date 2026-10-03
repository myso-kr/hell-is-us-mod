//! Argument parsing.

use std::path::PathBuf;

/// The help, in the game's language (`assets/i18n/help/<culture>.txt`; English when
/// there is none for it).
pub fn usage() -> &'static str {
    let culture = crate::i18n::culture();
    let pick = crate::i18n::culture::best(&culture, |c| HELP.iter().any(|(h, _)| *h == c)).unwrap_or_else(|| "en".into());
    HELP.iter().find(|(h, _)| *h == pick).map_or(HELP[0].1, |(_, t)| t).trim_end()
}

/// The help texts shipped, by culture; English first (the fallback).
pub const HELP: [(&str, &str); 12] = [
    ("en", include_str!("../assets/i18n/help/en.txt")),
    ("de", include_str!("../assets/i18n/help/de.txt")),
    ("es", include_str!("../assets/i18n/help/es.txt")),
    ("fr", include_str!("../assets/i18n/help/fr.txt")),
    ("it", include_str!("../assets/i18n/help/it.txt")),
    ("ja", include_str!("../assets/i18n/help/ja.txt")),
    ("ko", include_str!("../assets/i18n/help/ko.txt")),
    ("pl", include_str!("../assets/i18n/help/pl.txt")),
    ("pt-BR", include_str!("../assets/i18n/help/pt-BR.txt")),
    ("ru", include_str!("../assets/i18n/help/ru.txt")),
    ("tr", include_str!("../assets/i18n/help/tr.txt")),
    ("zh-Hans", include_str!("../assets/i18n/help/zh-Hans.txt")),
];

#[derive(Debug, PartialEq)]
pub enum Command {
    /// The panel; `true` starts the game first if it is not running.
    Ui(bool),
    Doctor,
    /// `doctor <inspect|find|dump|watch|scan> …`
    Probe(Vec<String>),
    List,
    Get(Vec<String>),
    Set(String, f32),
    Hold(Vec<String>),
    Restore,
    Pose,
}

#[derive(Debug, PartialEq)]
pub struct Options {
    pub command: Command,
    pub game_dir: Option<PathBuf>,
}

pub enum Parsed {
    Run(Options),
    Help,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
    let mut game_dir = None;
    let mut words = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" | "help" => return Ok(Parsed::Help),
            "--game-dir" => game_dir = Some(PathBuf::from(it.next().ok_or("--game-dir needs a path")?)),
            _ if a.starts_with("--") => return Err(format!("unknown option {a}")),
            _ => words.push(a),
        }
    }
    let rest = |n: usize| words[n..].to_vec();
    let command = match words.first().map(String::as_str) {
        None => Command::Ui(true),
        Some("ui") => Command::Ui(false),
        Some("doctor") if words.len() > 1 => Command::Probe(rest(1)),
        Some("doctor") => Command::Doctor,
        Some("list") => Command::List,
        Some("restore") => Command::Restore,
        Some("pose") => Command::Pose,
        Some("get") if words.len() > 1 => Command::Get(rest(1)),
        Some("set") if words.len() == 3 => {
            let v = words[2].parse().map_err(|_| format!("{} is not a number", words[2]))?;
            Command::Set(words[1].clone(), v)
        }
        Some("hold") if words.len() > 1 => Command::Hold(rest(1)),
        Some(x) => return Err(format!("`{x}` is not a command, or is missing its arguments")),
    };
    Ok(Parsed::Run(Options { command, game_dir }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Result<Options, String> {
        match parse(s.split_whitespace().map(String::from))? {
            Parsed::Run(o) => Ok(o),
            Parsed::Help => Err("help".into()),
        }
    }

    #[test]
    fn commands() {
        assert_eq!(run("doctor").unwrap().command, Command::Doctor);
        assert_eq!(run("restore").unwrap().command, Command::Restore);
        assert_eq!(run("pose").unwrap().command, Command::Pose);
        assert_eq!(run("set emeralds 500").unwrap().command, Command::Set("emeralds".into(), 500.0));
        assert_eq!(run("hold god speed=2").unwrap().command, Command::Hold(vec!["god".into(), "speed=2".into()]));
        assert_eq!(run("--game-dir D:\\G doctor").unwrap().game_dir, Some(PathBuf::from("D:\\G")));
    }

    #[test]
    fn mistakes() {
        assert!(run("set emeralds").is_err());
        assert!(run("set emeralds lots").is_err());
        assert!(run("hold").is_err());
        assert!(run("fly").is_err());
        assert!(run("--turbo list").is_err());
        assert_eq!(run("").unwrap().command, Command::Ui(true));
        assert_eq!(run("ui").unwrap().command, Command::Ui(false));
        assert_eq!(run("--help").unwrap_err(), "help");
    }
}
