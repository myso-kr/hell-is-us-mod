//! The overlay thread: the minimap in the game window's top-right corner and the
//! compass strip at its top centre, both click-through layered windows
//! (ui/layered.rs) drawn by raster.rs — and the guide that picks what they point to.
//!
//! Not eframe viewports: eframe stops running frames while the panel is hidden (F8),
//! and these have to keep drawing then.
//!
//! Keys, polled like F8 and only while the game or the panel has focus, chosen in the
//! panel: show/hide the map (F9), drop or remove a marker (F6), show/hide the compass
//! (F10), move the guide to the next place (F11), the big map in the middle (F3).
//! Only the worker's snapshot is read here — never the game's memory.
//!
//! While a game menu is open — the game shows its mouse cursor, or is paused — every
//! overlay hides (panel setting), so the inventory and menus are never covered.
//!
//! The walking route to the guide's goal (pathfind.rs) is worked out here, at most
//! once a second and whenever the goal changes or the hero has moved on.

use super::hotkey::{game_window, pid_of};
use super::layered::{pump, Layered};
use super::Shared;
use crate::goals::{Goal, Tier};
use crate::minimap::{Display, MapState, ReliefMode, View};
use crate::raster::{draw_compass, draw_map, Canvas, Pin};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F1};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorInfo, GetForegroundWindow, CURSORINFO, CURSOR_SHOWING};

/// The minimap is this many pixels square.
const MAP_PX: i32 = 240;
/// The compass strip.
const COMPASS_W: i32 = 560;
const COMPASS_H: i32 = 60;
/// Gap from the game window's edges.
const MARGIN: i32 = 24;
const FRAME: Duration = Duration::from_millis(50);
/// The big map takes this share of the game window's height, and is drawn every
/// third frame — it is many more pixels.
const BIG_SHARE: f32 = 0.8;
const BIG_EVERY: u32 = 3;
/// Work the route out again after this long, or when the hero has moved this far (cm).
const ROUTE_EVERY: Duration = Duration::from_secs(1);
const ROUTE_MOVED: f32 = 500.0;
/// The compass points at the route this far ahead (cm), not at the goal itself.
const ROUTE_AHEAD: f32 = 800.0;
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

/// Whether the system cursor is showing: in game, it only is while a menu wants the
/// mouse.
fn cursor_shown() -> bool {
    let mut ci: CURSORINFO = unsafe { std::mem::zeroed() };
    ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
    unsafe { GetCursorInfo(&mut ci) != 0 && ci.flags & CURSOR_SHOWING != 0 }
}

/// What the overlay thread knows about the current route.
/// The landscape baked for the map, and the bake under way.
#[derive(Default)]
struct Baking {
    done: Option<Arc<crate::relief::Relief>>,
    /// The scene it was baked from.
    from: Option<Arc<crate::obstacles::Scene>>,
    pending: Option<std::thread::JoinHandle<(crate::relief::Relief, Arc<crate::obstacles::Scene>)>>,
}

/// Bake again when the hero is this far from the baked square's centre (cm), or the
/// scene changes; the square reaches this far past the widest map's edge.
const RELIEF_MOVED: f32 = 10_000.0;
const RELIEF_SPARE: f32 = 15_000.0;

#[derive(Default)]
struct Route {
    path: crate::pathfind::Path,
    goal: Option<u64>,
    from: [f32; 2],
    at: Option<Instant>,
    /// A route being worked out off this thread (it can take a few hundred ms), and
    /// the goal it is for.
    pending: Option<std::thread::JoinHandle<(crate::pathfind::Path, u64)>>,
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
    // The big map's window is made at the game window's size, and again if that changes.
    let mut big_window: Option<Layered> = None;
    let mut big_cv = Canvas::new(1, 1);
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
    let mut was = [false; 4];
    let mut route = Route::default();
    let mut baking = Baking::default();
    let mut saved = Instant::now();
    let mut tick = 0u32;
    let mut frame_start = Instant::now();
    while !shared.quit.load(Ordering::SeqCst) {
        pump();
        // A frame every FRAME, whatever the last one took to draw.
        let spent = frame_start.elapsed();
        std::thread::sleep(FRAME.saturating_sub(spent).max(Duration::from_millis(5)));
        frame_start = Instant::now();
        tick = tick.wrapping_add(1);

        let game = shared.game_pid.load(Ordering::SeqCst);
        let focus = pid_of(unsafe { GetForegroundWindow() });
        let in_game = game != 0 && focus == game;
        let focused = in_game || (game != 0 && focus == std::process::id());
        let (pose, world, things, footprints, goals, paused, obstacles) = match shared.snap.lock().unwrap().as_ref() {
            Some(s) => (
                s.pose,
                s.world.clone(),
                s.things.clone(),
                s.footprints.clone(),
                s.goals.clone(),
                s.paused,
                s.obstacles.clone(),
            ),
            None => (None, None, Vec::new(), Default::default(), Vec::new(), false, Default::default()),
        };
        let here = pose.map(|(p, yaw)| ([p[0] as f32, p[1] as f32, p[2] as f32], yaw as f32));

        let mut state = shared.map.lock().unwrap();
        // The panel changed the icon size: rasterise them again, once.
        if state.icon_px != icon_px {
            icon_px = state.icon_px;
            icons = make(icon_px);
        }
        let keys = state.keys();
        let mut now = [false; 4];
        for i in 0..4 {
            now[i] = pressed(fkey(keys[i]), &mut was[i]) && focused;
        }
        let [toggle_now, marker_now, compass_now, cycle_now] = now;
        if toggle_now {
            state.display = state.next_display();
            state.dirty = true;
        }
        if compass_now {
            state.compass = !state.compass;
            state.dirty = true;
        }
        // A game menu is open: the game shows its cursor (only while it has focus —
        // the panel shows one too), or it is paused.
        let menu = state.hide_in_menus && ((in_game && cursor_shown()) || paused);
        *shared.menu.lock().unwrap() = (in_game && cursor_shown(), paused);

        let window = game_window(game).filter(|_| focused && !menu).map(|(_, r)| r);
        match (here, world.as_deref(), window) {
            (Some((p, yaw)), Some(world), Some(r)) => {
                state.observe(world, p);
                if marker_now {
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
                if cycle_now {
                    cycle(&mut state, &goals, p);
                }

                // The route to the goal, when it is due.
                let goal = state.target.and_then(|t| goals.iter().find(|g| g.id == t));
                match goal.filter(|_| state.route) {
                    Some(g) => {
                        let moved = ((p[0] - route.from[0]).powi(2) + (p[1] - route.from[1]).powi(2)).sqrt();
                        let due = route.goal != Some(g.id)
                            || moved > ROUTE_MOVED
                            || route.at.is_none_or(|t| t.elapsed() >= ROUTE_EVERY);
                        if route.pending.as_ref().is_some_and(|h| h.is_finished()) {
                            if let Ok((path, id)) = route.pending.take().unwrap().join() {
                                if id == g.id {
                                    route.path = path;
                                }
                            }
                        }
                        if due && route.pending.is_none() {
                            let trail: Vec<[f32; 2]> = state
                                .trails
                                .get(world)
                                .map(|t| t.iter().flatten().map(|q| [q[0], q[1]]).collect())
                                .unwrap_or_default();
                            let (from, to, feet, id, scene) =
                                ([p[0], p[1]], [g.at[0], g.at[1]], p[2] - 90.0, g.id, obstacles.clone());
                            route.pending = Some(std::thread::spawn(move || {
                                let path =
                                    crate::pathfind::route(from, to, feet, &scene.obstacles, &scene.terrain, &trail);
                                (path, id)
                            }));
                            if route.goal != Some(g.id) {
                                route.path = Default::default();
                            }
                            route.goal = Some(g.id);
                            route.from = [p[0], p[1]];
                            route.at = Some(Instant::now());
                        }
                    }
                    None => route = Route::default(),
                }
                // Keep the drawn route starting at the hero.
                let mut path = route.path.clone();
                if let Some(first) = path.points.first_mut() {
                    *first = [p[0], p[1]];
                }
                *shared.route_uncertain.lock().unwrap() = path.uncertain();

                // The landscape for the map, baked off this thread when the hero has
                // moved far, the map grew, or the scene changed.
                if baking.pending.as_ref().is_some_and(|h| h.is_finished()) {
                    if let Ok((rel, scene)) = baking.pending.take().unwrap().join() {
                        baking.done = Some(Arc::new(rel));
                        baking.from = Some(scene);
                    }
                }
                let half = state.radius_m.max(state.big_radius_m) * 100.0 + RELIEF_SPARE;
                let stale = baking.done.as_ref().is_none_or(|r| {
                    let (c, h) = r.extent();
                    (p[0] - c[0]).hypot(p[1] - c[1]) > RELIEF_MOVED
                        || h < half - RELIEF_SPARE / 2.0
                        || h > half * 2.0
                        || (p[2] - 90.0 - r.feet).abs() > crate::relief::TINT_MOVED
                }) || baking.from.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &obstacles));
                if state.relief != ReliefMode::Off && !obstacles.terrain.is_empty() && stale && baking.pending.is_none()
                {
                    let (scene, centre, feet) = (obstacles.clone(), [p[0], p[1]], p[2] - 90.0);
                    baking.pending = Some(std::thread::spawn(move || {
                        (crate::relief::Relief::bake(&scene, centre, half, feet), scene)
                    }));
                }
                let relief = baking.done.clone().filter(|_| state.relief != ReliefMode::Off);

                if state.display == Display::Big {
                    map_window.hide();
                    let side = ((r.bottom - r.top) as f32 * BIG_SHARE) as i32;
                    if big_window.as_ref().is_none_or(|w| w.w != side) {
                        big_window = Layered::new("hiumod-bigmap", "Hell Is Us Map", side, side);
                        big_cv = Canvas::new(side as usize, side as usize);
                    }
                    if let Some(w) = big_window.as_mut() {
                        if tick % BIG_EVERY == 0 || !w.is_shown() {
                            let view = View {
                                center: p,
                                yaw_deg: yaw,
                                heading_up: false,
                                scale: (side as f32 / 2.0 - 14.0) / (state.big_radius_m * 100.0),
                                north_deg: state.north_yaw,
                                outline: state.big_outline,
                            };
                            draw_map(
                                &mut big_cv,
                                &state,
                                world,
                                &view,
                                &things,
                                icons.as_ref(),
                                &footprints,
                                &goals,
                                &path,
                                relief.as_deref(),
                            );
                            let x = r.left + (r.right - r.left - side) / 2;
                            let y = r.top + (r.bottom - r.top - side) / 2;
                            w.present_alpha(&big_cv, x, y, (state.big_alpha as u32 * 255 / 100) as u8);
                        }
                    }
                } else {
                    if let Some(w) = big_window.as_mut() {
                        w.hide();
                    }
                    if state.display == Display::Mini {
                        let view = View {
                            center: p,
                            yaw_deg: yaw,
                            heading_up: state.heading_up,
                            scale: (MAP_PX as f32 / 2.0 - 14.0) / (state.radius_m * 100.0),
                            north_deg: state.north_yaw,
                            outline: state.mini_outline,
                        };
                        draw_map(
                            &mut map_cv,
                            &state,
                            world,
                            &view,
                            &things,
                            icons.as_ref(),
                            &footprints,
                            &goals,
                            &path,
                            relief.as_deref(),
                        );
                        map_window.present(&map_cv, r.right - MAP_PX - MARGIN, r.top + MARGIN + 24);
                    } else {
                        map_window.hide();
                    }
                }

                if state.compass {
                    let mut pins: Vec<Pin> = goals
                        .iter()
                        .filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0 || Some(g.id) == state.target)
                        .map(|g| {
                            let target = Some(g.id) == state.target;
                            // The target is pointed at along its route, and its distance is the route's.
                            let (aim, distance) = match (target, path.points.len() >= 2) {
                                (true, true) => {
                                    let next = crate::pathfind::next_point(&path.points, [p[0], p[1]], ROUTE_AHEAD)
                                        .unwrap_or([g.at[0], g.at[1]]);
                                    ([next[0], next[1], g.at[2]], crate::pathfind::length(&path.points))
                                }
                                _ => (g.at, flat(p, g.at)),
                            };
                            Pin {
                                bearing: bearing(p, aim) - state.north_yaw,
                                rgb: g.tier.rgb(),
                                target,
                                distance_m: distance / 100.0,
                            }
                        })
                        .collect();
                    if let Some(markers) = state.markers.get(world) {
                        pins.extend(markers.iter().map(|m| Pin {
                            bearing: bearing(p, *m) - state.north_yaw,
                            rgb: [255, 200, 60],
                            target: false,
                            distance_m: flat(p, *m) / 100.0,
                        }));
                    }
                    draw_compass(&mut compass_cv, yaw - state.north_yaw, &pins);
                    let x = r.left + (r.right - r.left - COMPASS_W) / 2;
                    compass_window.present(&compass_cv, x, r.top + 12);
                } else {
                    compass_window.hide();
                }
                if tick % 20 == 0 {
                    map_window.keep_on_top();
                    compass_window.keep_on_top();
                    if let Some(w) = big_window.as_ref() {
                        w.keep_on_top();
                    }
                }
            }
            _ => {
                map_window.hide();
                compass_window.hide();
                if let Some(w) = big_window.as_mut() {
                    w.hide();
                }
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
