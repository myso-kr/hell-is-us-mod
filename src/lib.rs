//! The module list, so the tests can drive everything that does not need the game.

pub mod actors;
pub mod anchors;
pub mod attr;
pub mod cheats;
pub mod cli;
#[cfg(windows)]
pub mod engine;
pub mod game;
pub mod geometry;
pub mod goals;
pub mod gobjects;
pub mod hold;
pub mod icons;
pub mod journal;
pub mod knowledge;
pub mod log;
pub mod mem;
pub mod minimap;
pub mod names;
pub mod paths;
pub mod player;
pub mod raster;
pub mod settings;
#[cfg(windows)]
pub mod ui;
pub mod verify;
