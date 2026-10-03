//! The game outside of its memory: where it is installed, and whether it is running.

pub mod achievements;
#[cfg(windows)]
pub mod launch;
pub mod locate;
#[cfg(windows)]
pub mod process;
