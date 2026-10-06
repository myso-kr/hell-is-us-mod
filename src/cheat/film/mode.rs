//! How a take moves: the hero walked along the route by the mod, the player's own hand with the
//! camera directed, or the camera alone in flight.

/// Who moves what in a take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The mod walks the hero along the route; any input stops it.
    #[default]
    Walk,
    /// The player plays — walks, fights — and the take only moves the camera, keeping them in
    /// view; input does not stop it, only the key or the card.
    Live,
    /// The camera flies alone along the path; the hero stays where it is.
    Flight,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Walk, Mode::Live, Mode::Flight];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Walk => tr!("FILM_MODE_WALK"),
            Mode::Live => tr!("FILM_MODE_LIVE"),
            Mode::Flight => tr!("FILM_MODE_FLIGHT"),
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Mode::Walk => tr!("FILM_MODE_WALK_ABOUT"),
            Mode::Live => tr!("FILM_MODE_LIVE_ABOUT"),
            Mode::Flight => tr!("FILM_FLIGHT_NOTE"),
        }
    }

    pub(super) fn word(self) -> &'static str {
        match self {
            Mode::Walk => "walk",
            Mode::Live => "live",
            Mode::Flight => "flight",
        }
    }

    pub(super) fn from_word(w: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.word() == w)
    }

    /// Whether the mod drives the hero (and so stops at the player's input).
    pub fn drives(self) -> bool {
        self == Mode::Walk
    }
}
