//! The camera's modes for a take.

/// How the camera moves while filming.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lens {
    /// The player's camera, untouched.
    Free,
    /// ActiveTrack: the way ahead, a little down.
    #[default]
    Follow,
    /// Spotlight: on the route's end the whole way.
    Spotlight,
    /// Circle: about the hero, `ORBIT_DPS` a second.
    Orbit,
}

impl Lens {
    pub const ALL: [Lens; 4] = [Lens::Free, Lens::Follow, Lens::Spotlight, Lens::Orbit];

    pub fn label(self) -> &'static str {
        match self {
            Lens::Free => tr!("FILM_LENS_FREE"),
            Lens::Follow => tr!("FILM_LENS_FOLLOW"),
            Lens::Spotlight => tr!("FILM_LENS_SPOTLIGHT"),
            Lens::Orbit => tr!("FILM_LENS_ORBIT"),
        }
    }
}

impl Lens {
    pub(super) fn word(self) -> &'static str {
        match self {
            Lens::Free => "free",
            Lens::Follow => "follow",
            Lens::Spotlight => "spotlight",
            Lens::Orbit => "orbit",
        }
    }
}
