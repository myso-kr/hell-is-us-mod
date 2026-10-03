//! The game outside of its memory: where it is installed, and whether it is running.

/// The Steam build the mod was last checked on (`doctor` passes, cheats seen in play).
pub const TESTED_BUILD: &str = "24045435";

pub mod achievements;
#[cfg(windows)]
pub mod launch;
pub mod locate;
#[cfg(windows)]
pub mod process;
