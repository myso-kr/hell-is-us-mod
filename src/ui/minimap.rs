//! The overlay thread: the minimap in the game window's top-right corner and the
//! compass strip at its top centre, both click-through layered windows
//! (ui/layered.rs) drawn by raster.rs — and the guide that picks what they point to.
//!
//! Not eframe viewports: eframe stops running frames while the panel is hidden (F8),
//! and these have to keep drawing then.
//!
//! Keys, polled like F8 and only while the game or the panel has focus, chosen in the
//! panel: show/hide the map (F9), drop or remove a marker (F6), show/hide the compass
//! (F10), move the guide to the next place (F11). Only the worker's snapshot is read
//! here — never the game's memory.

use super::hotkey::{game_window, pid_of};
use super::layered::{pump, Layered};
use super::Shared;
use crate::goals::{Goal, Tier};
use crate::minimap::{MapState, View};
use crate::raster::{draw_compass, draw_map, Canvas, Pin};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F1};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// The minimap is this many pixels square.
const MAP_PX: i32 = 240;
/// The compass strip.
const COMPASS_W: i32 = 560;
const COMPASS_H: i32 = 60;
/// Gap from the game window's edges.
const MARGIN: i32 = 24;
const FRAME: Duration = Duration::from_millis(50);
const SAVE_EVERY: Duration = Duration::from_secs(10);

pub fn path() -> std::path::PathBuf {
    crate::paths::data_dir().join("minimap.txt")
}

pub fn load() -> MapState {
    std::fs::read_to_string(path()).map(|t| MapState::parse(&t)).unwrap_or_default()
}

fn save(state: &mut MapState) {
    if state.dirty && std::fs::write(path(), state.render()).is_ok() {
        state.dirty = false;
    }
}

fn pressed(key: u16, was: &mut bool) -> bool {
    let down = unsafe { GetAsyncKeyState(key as i32) } as u16 & 0x8000 != 0;
    let edge = down && !*was;
    *was = down;
    edge
}

fn fkey(n: u8) -> u16 {
    VK_F1 + n as u16 - 1
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Which way `to` lies from `from`, as a UE yaw in degrees.
fn bearing(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

/// Keep the guide's target sensible: drop it when its place is gone (used up,
/// streamed out); with auto on, take the nearest quest goal when there is none.
pub fn settle_target(state: &mut MapState, goals: &[Goal], here: [f32; 3]) {
    if state.target.is_some_and(|t| !goals.iter().any(|g| g.id == t)) {
        state.target = None;
    }
    if state.target.is_none() && state.guide_auto {
        state.target = goals
            .iter()
            .filter(|g| g.tier == Tier::Quest)
            .min_by(|a, b| flat(a.at, here).total_cmp(&flat(b.at, here)))
            .map(|g| g.id);
    }
}

/// The next goal, by distance, after the current target — among the tiers shown.
fn cycle(state: &mut MapState, goals: &[Goal], here: [f32; 3]) {
    let mut shown: Vec<&Goal> = goals.iter().filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0).collect();
    shown.sort_by(|a, b| flat(a.at, here).total_cmp(&flat(b.at, here)));
    let next = match state.target.and_then(|t| shown.iter().position(|g| g.id == t)) {
        Some(i) => shown.get(i + 1).or(shown.first()),
        None => shown.first(),
    };
    state.target = next.map(|g| g.id);
}

pub fn run(shared: Arc<Shared>) {
    let (Some(mut map_window), Some(mut compass_window)) = (
        Layered::new("hiumod-minimap", "Hell Is Us Minimap", MAP_PX, MAP_PX),
        Layered::new("hiumod-compass", "Hell Is Us Compass", COMPASS_W, COMPASS_H),
    ) else {
        crate::journal::line("overlay: could not create its windows");
        return;
    };
    let mut map_cv = Canvas::new(MAP_PX as usize, MAP_PX as usize);
    let mut compass_cv = Canvas::new(COMPASS_W as usize, COMPASS_H as usize);
    // Without icons the map still works, with dots.
    let make = |px: u8| match crate::icons::Icons::new(px as usize) {
        Ok(i) => Some(i),
        Err(e) => {
            crate::journal::line(&format!("minimap: {e} — drawing dots"));
            None
        }
    };
    let mut icon_px = shared.map.lock().unwrap().icon_px;
    let mut icons = make(icon_px);
    let (mut marker_was, mut toggle_was, mut compass_was, mut cycle_was) = (false, false, false, false);
    let mut saved = Instant::now();
    let mut tick = 0u32;
    while !shared.quit.load(Ordering::SeqCst) {
        pump();
        std::thread::sleep(FRAME);
        tick = tick.wrapping_add(1);

        let game = shared.game_pid.load(Ordering::SeqCst);
        let focus = pid_of(unsafe { GetForegroundWindow() });
        let focused = game != 0 && (focus == game || focus == std::process::id());
        let (pose, world, things, footprints, goals) = match shared.snap.lock().unwrap().as_ref() {
            Some(s) => (s.pose, s.world.clone(), s.things.clone(), s.footprints.clone(), s.goals.clone()),
            None => (None, None, Vec::new(), Default::default(), Vec::new()),
        };
        let here = pose.map(|(p, yaw)| ([p[0] as f32, p[1] as f32, p[2] as f32], yaw as f32));

        let mut state = shared.map.lock().unwrap();
        // The panel changed the icon size: rasterise them again, once.
        if state.icon_px != icon_px {
            icon_px = state.icon_px;
            icons = make(icon_px);
        }
        let toggle_now = pressed(fkey(state.toggle_key), &mut toggle_was);
        let marker_now = pressed(fkey(state.marker_key), &mut marker_was);
        let compass_now = pressed(fkey(state.compass_key), &mut compass_was);
        let cycle_now = pressed(fkey(state.cycle_key), &mut cycle_was);
        if focused && toggle_now {
            state.show = !state.show;
            state.dirty = true;
        }
        if focused && compass_now {
            state.compass = !state.compass;
            state.dirty = true;
        }

        let window = game_window(game).filter(|_| focused).map(|(_, r)| r);
        match (here, world.as_deref(), window) {
            (Some((p, yaw)), Some(world), Some(r)) => {
                state.observe(world, p);
                if focused && marker_now {
                    let added = state.toggle_marker(world, p);
                    crate::journal::line(&format!(
                        "minimap: marker {} at ({:.0}, {:.0}, {:.0}) in {world}",
                        if added { "added" } else { "removed" },
                        p[0],
                        p[1],
                        p[2]
                    ));
                }
                settle_target(&mut state, &goals, p);
                if focused && cycle_now {
                    cycle(&mut state, &goals, p);
                }

                if state.show {
                    let view = View {
                        center: p,
                        yaw_deg: yaw,
                        heading_up: state.heading_up,
                        scale: (MAP_PX as f32 / 2.0 - 14.0) / (state.radius_m * 100.0),
                    };
                    draw_map(&mut map_cv, &state, world, &view, &things, icons.as_ref(), &footprints, &goals);
                    map_window.present(&map_cv, r.right - MAP_PX - MARGIN, r.top + MARGIN + 24);
                } else {
                    map_window.hide();
                }

                if state.compass {
                    let mut pins: Vec<Pin> = goals
                        .iter()
                        .filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0 || Some(g.id) == state.target)
                        .map(|g| Pin {
                            bearing: bearing(p, g.at),
                            rgb: g.tier.rgb(),
                            target: Some(g.id) == state.target,
                            distance_m: flat(p, g.at) / 100.0,
                        })
                        .collect();
                    if let Some(markers) = state.markers.get(world) {
                        pins.extend(markers.iter().map(|m| Pin {
                            bearing: bearing(p, *m),
                            rgb: [255, 200, 60],
                            target: false,
                            distance_m: flat(p, *m) / 100.0,
                        }));
                    }
                    draw_compass(&mut compass_cv, yaw, &pins);
                    let x = r.left + (r.right - r.left - COMPASS_W) / 2;
                    compass_window.present(&compass_cv, x, r.top + 12);
                } else {
                    compass_window.hide();
                }
                if tick % 20 == 0 {
                    map_window.keep_on_top();
                    compass_window.keep_on_top();
                }
            }
            _ => {
                map_window.hide();
                compass_window.hide();
            }
        }
        if saved.elapsed() >= SAVE_EVERY {
            save(&mut state);
            saved = Instant::now();
        }
    }
    save(&mut shared.map.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(id: u64, tier: Tier, x: f32) -> Goal {
        Goal { tier, id, label: String::new(), detail: String::new(), at: [x, 0.0, 0.0] }
    }

    #[test]
    fn auto_guides_to_the_nearest_quest_goal_and_lets_go_of_a_vanished_one() {
        let mut s = MapState::default();
        let goals = [goal(1, Tier::Clue, 100.0), goal(2, Tier::Quest, 5000.0), goal(3, Tier::Quest, 900.0)];
        settle_target(&mut s, &goals, [0.0; 3]);
        assert_eq!(s.target, Some(3));
        settle_target(&mut s, &goals[..2], [0.0; 3]);
        assert_eq!(s.target, Some(2), "3 used up: the next quest goal");
        s.guide_auto = false;
        settle_target(&mut s, &goals[..1], [0.0; 3]);
        assert_eq!(s.target, None, "auto off: nothing chosen for the player");
    }

    #[test]
    fn the_cycle_key_walks_the_shown_goals_by_distance() {
        let mut s = MapState { guide_auto: false, ..MapState::default() };
        let goals = [goal(1, Tier::Clue, 300.0), goal(2, Tier::Quest, 100.0), goal(3, Tier::Secret, 200.0)];
        cycle(&mut s, &goals, [0.0; 3]);
        assert_eq!(s.target, Some(2));
        cycle(&mut s, &goals, [0.0; 3]);
        assert_eq!(s.target, Some(3));
        s.goal_tiers = 0b011; // no clues
        cycle(&mut s, &goals, [0.0; 3]);
        assert_eq!(s.target, Some(2), "wraps, skipping the hidden tier");
    }

    #[test]
    fn bearings_follow_unreal_yaw() {
        assert_eq!(bearing([0.0; 3], [100.0, 0.0, 0.0]), 0.0);
        assert_eq!(bearing([0.0; 3], [0.0, 100.0, 0.0]), 90.0);
    }
}
