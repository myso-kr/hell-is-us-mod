//! Argument parsing.

use std::path::PathBuf;

pub const USAGE: &str = "\
hiumod — single-player cheats and a minimap for Hell Is Us (Steam)

USAGE
  hiumod                        start the game if needed, then the panel — F8 shows/hides it;
                                the panel closes when the game does
  hiumod ui                     the panel only; does not start the game
  hiumod doctor                 check everything; writes nothing
  hiumod list                   every attribute of every set, read live
  hiumod get <attribute>...     some of them, as Name or Set.Name
  hiumod pose                   where the hero stands and faces, read live until Ctrl+C
  hiumod set <cheat> <value>    a balance written once (none in the table yet)
  hiumod hold <cheat>...        e.g. god stamina speed=2 — until Ctrl+C;
                                every held cheat is in `hiumod doctor`'s list
  hiumod restore                put back what a killed hold left changed

OPTIONS
  --game-dir <dir>              the install root, holding HellIsUs\
                                (default: found in your Steam libraries)

Nothing is written unless you are in control of the hero. Back up your save.";

#[derive(Debug, PartialEq)]
pub enum Command {
    /// The panel; `true` starts the game first if it is not running.
    Ui(bool),
    Doctor,
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
