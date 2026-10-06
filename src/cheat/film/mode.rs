//! How a take moves: the hero walked along the route by the mod, or the camera alone in flight.
//! (A take the player plays was tried and taken out: directed shots changing under the player's
//! hands made it hard to play and tiring to watch.)

/// Who moves what in a take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The mod walks the hero along the route; any input stops it.
    #[default]
    Walk,
    /// The camera flies alone along the path; the hero stays where it is.
    Flight,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Walk, Mode::Flight];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Walk => tr!("FILM_MODE_WALK"),
            Mode::Flight => tr!("FILM_MODE_FLIGHT"),
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Mode::Walk => tr!("FILM_MODE_WALK_ABOUT"),
            Mode::Flight => tr!("FILM_FLIGHT_NOTE"),
        }
    }

    pub(super) fn word(self) -> &'static str {
        match self {
            Mode::Walk => "walk",
            Mode::Flight => "flight",
        }
    }

    /// From film.txt: the taken-out take the player played reads as a walk.
    pub(super) fn from_word(w: &str) -> Option<Mode> {
        match w {
            "live" => Some(Mode::Walk),
            _ => Mode::ALL.into_iter().find(|m| m.word() == w),
        }
    }

    /// Whether the mod drives the hero.
    pub fn drives(self) -> bool {
        self == Mode::Walk
    }
}
