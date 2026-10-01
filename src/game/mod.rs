//! The game outside of its memory: where it is installed, and whether it is running.

#[cfg(windows)]
pub mod launch;
pub mod locate;
#[cfg(windows)]
pub mod process;
