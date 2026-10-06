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
    /// The director: each beat of the route its own shot, chosen from the route and what stands
    /// about it (director/).
    Director,
}

impl Lens {
    pub const ALL: [Lens; 5] = [Lens::Director, Lens::Follow, Lens::Spotlight, Lens::Orbit, Lens::Free];

    pub fn label(self) -> &'static str {
        match self {
            Lens::Free => tr!("FILM_LENS_FREE"),
            Lens::Follow => tr!("FILM_LENS_FOLLOW"),
            Lens::Spotlight => tr!("FILM_LENS_SPOTLIGHT"),
            Lens::Orbit => tr!("FILM_LENS_ORBIT"),
            Lens::Director => tr!("FILM_LENS_DIRECTOR"),
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
            Lens::Director => "director",
        }
    }
}
