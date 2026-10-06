//! Where a take's way comes from: the guide's route, points the player took, or a recording of the
//! player's own walk.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// The guide's route as the overlay draws it.
    #[default]
    Guide,
    /// The points taken in the game (Ctrl+the key) or on the 3D map, each leg on the navmesh.
    Points,
    /// A recording (Ctrl+Shift+the key), followed as it was walked.
    Recording,
    /// A tour of the world worked out from the navmesh alone (tour.rs).
    Tour,
}

impl Source {
    pub const ALL: [Source; 4] = [Source::Guide, Source::Points, Source::Recording, Source::Tour];

    pub(super) fn word(self) -> &'static str {
        match self {
            Source::Guide => "guide",
            Source::Points => "points",
            Source::Recording => "recording",
            Source::Tour => "tour",
        }
    }

    pub(super) fn from_word(w: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|s| s.word() == w)
    }
}
