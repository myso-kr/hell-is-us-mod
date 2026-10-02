//! The overlay thread: the minimap in the game window's top-right corner and the
//! compass strip at its top centre, both click-through layered windows
//! (ui/layered.rs) drawn by raster.rs — and the guide that picks what they point to.
//!
//! Not eframe viewports: eframe stops running frames while the panel is hidden (`),
//! and these have to keep drawing then.
//!
//! Keys, polled like the panel's ` and only while the game or the panel has focus, chosen in the
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
use crate::goals::{Gate, Goal, Tier};
use super::pen::Pen;
use super::tracker;
use crate::quests::Quest;
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
/// A goal's actor stands about this far above the floor it is on (cm).
const GOAL_FEET: f32 = 50.0;
/// Near a blocked goal, a goal this close (cm) may be what opens the way: a clue, a key,
/// a lever, a puzzle.
const HELPER: f32 = 4000.0;
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

/// What the tracker last drew: the journal, the followed quest, whether its places are
/// near, whether the guided goal is blocked, and the needs line.
type Tracked = (Vec<Quest>, Option<String>, bool, bool, String);

#[derive(Default)]
struct Route {
    path: crate::pathfind::Path,
    goal: Option<u64>,
    from: [f32; 2],
    at: Option<Instant>,
    /// A route being worked out off this thread (it can take a few hundred ms), and
    /// the goal it is for.
    pending: Option<std::thread::JoinHandle<(crate::pathfind::Path, u64)>>,
    /// Goals whose last route had to go through something.
    blocked: std::collections::HashSet<u64>,
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
/// Whether auto guiding can send the hero to `g` at all: not a place that pays out
/// only when something else happens there (a locked door), and not an item of a main
/// quest not begun.
fn reachable(g: &Goal, journal: &[Quest]) -> bool {
    g.gate != Gate::Conditional
        && !g.keys.iter().any(|k| journal.iter().any(|q| &q.key == k && q.status == crate::quests::Status::NotStarted))
}

/// Whether `g` is what auto guiding wants: a reachable place that moves the followed
/// quest along — or, following the main story with none of its places loaded, any
/// reachable quest goal.
fn wanted(g: &Goal, goals: &[Goal], followed: Option<&Quest>, journal: &[Quest]) -> bool {
    if !reachable(g, journal) {
        return false;
    }
    match followed {
        Some(q) if goals.iter().any(|x| x.serves(q) && reachable(x, journal)) => g.serves(q),
        Some(q) if !matches!(q.kind, crate::quests::Kind::Main(_)) => false,
        _ => g.tier == Tier::Quest,
    }
}

/// Auto guiding follows the quest: the nearest place that moves it along, kept until
/// it is used up, then the next — and a target it no longer wants (the quest was
/// changed, or one of its places came into range) gives way. A target picked by hand
/// is left alone until it is used up.
///
/// A goal whose route had to go through something (a locked door, a puzzle, another
/// way into a cellar) is `blocked`: auto guiding prefers one that can be walked to; when
/// every wanted goal is blocked, it goes to something that can be reached near the
/// nearest of them — a note, a key, a lever: what opens the way, often — and else to
/// the blocked goal itself, as near as the route gets.
pub fn settle_target(
    state: &mut MapState,
    goals: &[Goal],
    here: [f32; 3],
    followed: Option<&Quest>,
    journal: &[Quest],
    blocked: &std::collections::HashSet<u64>,
) {
    if state.target.is_some_and(|t| !goals.iter().any(|g| g.id == t)) {
        state.target = None;
        state.chosen = false;
    }
    if !state.guide_auto || state.chosen {
        return;
    }
    let near = |a: &&Goal, b: &&Goal| flat(a.at, here).total_cmp(&flat(b.at, here));
    let wanted_all: Vec<&Goal> =
        goals.iter().filter(|g| !state.skipped.contains(&g.id) && wanted(g, goals, followed, journal)).collect();
    let open = wanted_all.iter().copied().filter(|g| !blocked.contains(&g.id)).min_by(near);
    let pick = open.or_else(|| {
        let stuck = wanted_all.iter().copied().min_by(near)?;
        let helper = goals
            .iter()
            .filter(|g| g.id != stuck.id && !blocked.contains(&g.id) && g.gate == Gate::Open && !state.skipped.contains(&g.id))
            .filter(|g| flat(g.at, stuck.at) <= HELPER)
            .filter(|g| reachable(g, journal))
            .min_by(|a, b| flat(a.at, stuck.at).total_cmp(&flat(b.at, stuck.at)));
        Some(helper.unwrap_or(stuck))
    });
    // Keep the target while it is still what would be picked, or still wanted and not
    // blocked (nearness alone does not make the guide hop between goals).
    let keep = state.target.is_some_and(|t| {
        Some(t) == pick.map(|g| g.id)
            || wanted_all.iter().any(|g| g.id == t) && !blocked.contains(&t) && open.is_some()
    });
    if !keep {
        state.target = pick.map(|g| g.id);
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
    state.chosen = state.target.is_some();
}

pub fn run(shared: Arc<Shared>) {
    let (Some(mut map_window), Some(mut compass_window)) = (
        Layered::new("hiumod-minimap", "Hell Is Us Minimap", MAP_PX, MAP_PX),
        Layered::new("hiumod-compass", "Hell Is Us Compass", COMPASS_W, COMPASS_H),
    ) else {
        crate::journal::line("overlay: could not create its windows");
        return;
    };
    // The quest tracker: drawn again only when what it shows changes.
    let mut tracker_window = Layered::new("hiumod-tracker", "Hell Is Us Quests", tracker::W, tracker::H);
    let mut tracker_cv = Canvas::new(tracker::W as usize, tracker::H as usize);
    let mut pen = Pen::new(tracker::W, tracker::H);
    let mut tracked: Option<Tracked> = None;
    let mut tracker_used = 0;
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
        // Only while the game itself has focus: not while the panel does, nor anything else.
        let focused = in_game;
        let (pose, world, things, footprints, goals, paused, obstacles, journal, nav, needs) = match shared.snap.lock().unwrap().as_ref() {
            Some(s) => (
                s.pose,
                s.world.clone(),
                s.things.clone(),
                s.footprints.clone(),
                s.goals.clone(),
                s.paused,
                s.obstacles.clone(),
                s.journal.clone(),
                s.nav.clone(),
                s.needs.clone(),
            ),
            None => {
                (None, None, Vec::new(), Default::default(), Vec::new(), false, Default::default(), Vec::new(), Default::default(), Vec::new())
            }
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
                let chosen = state.quest.clone();
                settle_target(&mut state, &goals, p, crate::quests::followed(&journal, chosen.as_deref()), &journal, &route.blocked);
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
                                // Same goal: keep the way being followed unless this one
                                // is clearly better, so the compass does not swing.
                                // Whether it can be walked to at all: a route that has to go
                                // through (a locked door, a puzzle) marks its goal blocked.
                                if path.uncertain() {
                                    route.blocked.insert(id);
                                } else {
                                    route.blocked.remove(&id);
                                }
                                if id == g.id
                                    && (route.path.points.len() < 2
                                        || crate::pathfind::better(&route.path, &path, [p[0], p[1]], ROUTE_AHEAD))
                                {
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
                            let (from, to, feet, id, scene, nav) =
                                ([p[0], p[1]], [g.at[0], g.at[1]], p[2] - 90.0, g.id, obstacles.clone(), nav.clone());
                            let goal_feet = g.at[2] - GOAL_FEET;
                            route.pending = Some(std::thread::spawn(move || {
                                // The game's navmesh first: it knows stairs, cellars and
                                // closed doors. The obstacle grid where it has nothing.
                                let path = nav
                                    .route([from[0], from[1], feet], [to[0], to[1], goal_feet])
                                    .map(|(path, _)| path)
                                    .unwrap_or_else(|| {
                                        crate::pathfind::route(from, to, feet, &scene.obstacles, &scene.terrain, &trail)
                                    });
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
                    None => {
                        let blocked = std::mem::take(&mut route.blocked);
                        route = Route { blocked, ..Route::default() };
                    }
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
                                dz_m: (g.at[2] - p[2]) / 100.0,
                            }
                        })
                        .collect();
                    if let Some(markers) = state.markers.get(world) {
                        pins.extend(markers.iter().map(|m| Pin {
                            bearing: bearing(p, *m) - state.north_yaw,
                            rgb: [255, 200, 60],
                            target: false,
                            distance_m: flat(p, *m) / 100.0,
                            dz_m: (m[2] - p[2]) / 100.0,
                        }));
                    }
                    draw_compass(&mut compass_cv, yaw - state.north_yaw, &pins);
                    let x = r.left + (r.right - r.left - COMPASS_W) / 2;
                    compass_window.present(&compass_cv, x, r.top + 12);
                } else {
                    compass_window.hide();
                }
                if let (Some(w), Some(pen), true) = (tracker_window.as_mut(), pen.as_mut(), state.tracker) {
                    let followed = crate::quests::followed(&journal, state.quest.as_deref());
                    // Whether any place that moves the followed quest along is loaded.
                    let near = followed.is_none_or(|q| goals.iter().any(|g| g.serves(q)));
                    // The goal being guided to can only be reached through something.
                    let stuck = state.target.is_some_and(|t| route.blocked.contains(&t));
                    // What it still needs: here, and elsewhere (the survey).
                    let line = followed
                        .and_then(|q| needs.iter().find(|(k, _)| *k == q.key))
                        .map(|(_, list)| {
                            let w = crate::survey::Survey::world_of(world);
                            let here = list.iter().filter(|x| !x.done && x.world == w).count();
                            let away = list.iter().filter(|x| !x.done && x.world != w).count();
                            match (here, away) {
                                (0, 0) => String::new(),
                                (h, 0) => format!("필요한 것: 이 지역 {h}곳"),
                                (0, a) => format!("필요한 것: 다른 지역 {a}곳 — 장갑차로 이동"),
                                (h, a) => format!("필요한 것: 이 지역 {h}곳 · 다른 지역 {a}곳"),
                            }
                        })
                        .unwrap_or_default();
                    let now = (journal.clone(), followed.map(|q| q.key.clone()), near, stuck, line.clone());
                    if tracked.as_ref() != Some(&now) {
                        tracker_used = tracker::draw(&mut tracker_cv, pen, &journal, followed, near, stuck, &line);
                        tracked = Some(now);
                    }
                    if tracker_used > 0 {
                        // At the right, centred on the screen's middle.
                        let y = r.top + (r.bottom - r.top - tracker_used) / 2;
                        w.present(&tracker_cv, r.right - tracker::W - MARGIN, y);
                    } else {
                        w.hide();
                    }
                } else if let Some(w) = tracker_window.as_mut() {
                    w.hide();
                }
                if tick % 20 == 0 {
                    // Under the panel while it shows, so the two never trade places.
                    let panel = shared
                        .visible
                        .load(Ordering::SeqCst)
                        .then(|| shared.hwnd.load(Ordering::SeqCst) as windows_sys::Win32::Foundation::HWND);
                    map_window.keep_on_top(panel);
                    compass_window.keep_on_top(panel);
                    if let Some(w) = tracker_window.as_ref() {
                        w.keep_on_top(panel);
                    }
                    if let Some(w) = big_window.as_ref() {
                        w.keep_on_top(panel);
                    }
                }
            }
            _ => {
                map_window.hide();
                compass_window.hide();
                if let Some(w) = tracker_window.as_mut() {
                    w.hide();
                }
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
        Goal { tier, id, label: String::new(), detail: String::new(), at: [x, 0.0, 0.0], quests: vec![], tags: vec![], keys: vec![], gate: Gate::Open }
    }

    #[test]
    fn auto_guides_to_the_nearest_quest_goal_and_lets_go_of_a_vanished_one() {
        let mut s = MapState::default();
        let goals = [goal(1, Tier::Clue, 100.0), goal(2, Tier::Quest, 5000.0), goal(3, Tier::Quest, 900.0)];
        settle_target(&mut s, &goals, [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.target, Some(3));
        settle_target(&mut s, &goals[..2], [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.target, Some(2), "3 used up: the next quest goal");
        s.guide_auto = false;
        settle_target(&mut s, &goals[..1], [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.target, None, "auto off: nothing chosen for the player");
    }

    #[test]
    fn auto_follows_the_followed_quest_and_leaves_a_hand_picked_target() {
        let deed = Quest {
            key: "d".into(),
            kind: crate::quests::Kind::GoodDeed,
            name: String::new(),
            detail: String::new(),
            status: crate::quests::Status::Started,
            progress: None,
            leads: vec![],
            quest: None,
            tags: Some("Secrets.Facts.GoldenWatch".into()),
        };
        let mut hand_over = goal(4, Tier::Secret, 3000.0);
        hand_over.tags = vec!["Secrets.Facts.GoldenWatchCompleted".into()];
        let goals = [goal(3, Tier::Quest, 900.0), hand_over];
        let mut s = MapState::default();
        settle_target(&mut s, &goals, [0.0; 3], None, &[], &Default::default());
        assert_eq!(s.target, Some(3), "the main story: the nearest quest goal");
        settle_target(&mut s, &goals, [0.0; 3], Some(&deed), &[], &Default::default());
        assert_eq!(s.target, Some(4), "following the deed: its hand-over, though farther");
        settle_target(&mut s, &goals[..1], [0.0; 3], Some(&deed), &[], &Default::default());
        assert_eq!(s.target, None, "nothing of the deed loaded: no stand-in");
        let mut door = goal(5, Tier::Quest, 10.0);
        door.gate = Gate::Conditional;
        let mut s2 = MapState::default();
        settle_target(&mut s2, &[door, goal(6, Tier::Quest, 500.0)], [0.0; 3], None, &[], &Default::default());
        assert_eq!(s2.target, Some(6), "a locked door is not where auto guiding sends the hero");
        // Blocked: the wanted goal is behind a door; a note near it is reachable.
        let mut cellar = goal(7, Tier::Quest, 1000.0);
        cellar.at[2] = -1200.0;
        let note = goal(8, Tier::Clue, 1300.0);
        let blocked: std::collections::HashSet<u64> = [7].into_iter().collect();
        let mut s3 = MapState::default();
        settle_target(&mut s3, &[cellar, note], [0.0; 3], None, &[], &blocked);
        assert_eq!(s3.target, Some(8), "behind a door: first what is near it and reachable");
        s.target = Some(3);
        s.chosen = true;
        settle_target(&mut s, &goals, [0.0; 3], Some(&deed), &[], &Default::default());
        assert_eq!(s.target, Some(3), "picked by hand: kept");
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
