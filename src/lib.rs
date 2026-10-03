//! The module list, so the tests can drive everything that does not need the game.
//!
//! The modules live in folders by role (.spec/ARCHITECTURE.md): `unreal` (memory and
//! reflection), `read` (the world, read through it), `cheat`, `guide`, `map`, `infra`,
//! `game` (the process from outside), `ui`. Each module is also re-exported at the top
//! (`crate::goals`, `hiumod::mem`), so a path names a module, not where it lives.

#[macro_use]
pub mod i18n;

pub mod cheat;
pub mod cli;
#[cfg(windows)]
pub mod engine;
pub mod game;
pub mod guide;
pub mod infra;
pub mod map;
pub mod read;
#[cfg(windows)]
pub mod ui;
pub mod unreal;

pub use cheat::{cheats, extras, hold};
pub use guide::{goals, missables, pathfind, quests, survey, tables};
pub use infra::{backup, gamedata, log, logfile, memstat, paths, runtime, settings, verify};
pub use map::{icons, minimap, raster, relief, symbols};
pub use read::{actors, attr, geometry, knowledge, navmesh, obstacles, puzzles, terrain};
pub use unreal::{anchors, gobjects, mem, names, player, probe, usmap};
