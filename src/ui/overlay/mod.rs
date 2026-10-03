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
//! The walking route to the guide's goal (route.rs), the landscape bake (bake.rs) and
//! what the windows show (hud.rs) are worked out beside this loop.

mod bake;
mod hud;
mod route;

use super::hotkey::{game_window, pid_of};
use super::layered::{pump, Layered};
use super::pen::Pen;
use super::tracker;
use super::Shared;
use crate::guide::target::{cycle, settle_target};
use crate::minimap::{Display, MapState, View};
use crate::quests::Quest;
use crate::raster::{draw_compass, draw_map, Canvas};
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
/// The panel's hero map: its side (px), how far it reaches (m) and how often (frames).
const HERO_PX: usize = 176;
const HERO_RADIUS_M: f32 = 120.0;
const HERO_EVERY: u32 = 20;
/// The Map page's preview: redrawn this often (frames) while the page shows, so a
/// setting moved is seen at once.
const PREVIEW_EVERY: u32 = 4;
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

/// What the tracker last drew: the journal, the followed quest, whether its places are
/// near, whether the guided goal is blocked, and the needs line.
type Tracked = (Arc<Vec<Quest>>, Option<String>, bool, bool, String);

pub fn run(shared: Arc<Shared>) {
    let (Some(mut map_window), Some(mut compass_window)) = (
        Layered::new("hiumod-minimap", "Hell Is Us Minimap", MAP_PX, MAP_PX),
        Layered::new("hiumod-compass", "Hell Is Us Compass", COMPASS_W, COMPASS_H),
    ) else {
        crate::logfile::line("overlay: could not create its windows");
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
            crate::logfile::line(&format!("minimap: {e} — drawing dots"));
            None
        }
    };
    let mut icon_px = shared.map.lock().unwrap().icon_px;
    let mut icons = make(icon_px);
    let mut was = [false; 4];
    let mut route = route::Route::default();
    let mut baking = bake::Baking::default();
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
        let (pose, world, things, footprints, goals, paused, obstacles, journal, nav, needs, deadlines, puzzle_near) =
            match shared.snap.lock().unwrap().as_ref() {
                Some(s) => (
                    s.pose,
                    s.world.clone(),
                    hud::with_survey(&s.things, s),
                    s.footprints.clone(),
                    s.goals.clone(),
                    s.paused,
                    s.obstacles.clone(),
                    s.journal.clone(),
                    s.nav.clone(),
                    s.needs.clone(),
                    s.deadlines.clone(),
                    hud::puzzle_near(s),
                ),
                None => (
                    None,
                    None,
                    Vec::new(),
                    Default::default(),
                    Vec::new(),
                    false,
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    false,
                ),
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

        // The panel's hero map, while the panel shows (the game has no focus then, so
        // the minimap below is not drawn): north up, the relief and the places, no route.
        if tick % HERO_EVERY == 0 && shared.visible.load(Ordering::SeqCst) {
            if let (Some((p, yaw)), Some(world)) = (here, world.as_deref()) {
                let view = View {
                    center: p,
                    yaw_deg: yaw,
                    heading_up: false,
                    scale: (HERO_PX as f32 / 2.0 - 14.0) / (HERO_RADIUS_M * 100.0),
                    north_deg: state.north_yaw,
                    outline: false,
                };
                let relief = baking.relief(&state, p, &obstacles);
                let goals = hud::with_pins(&goals, &state, world);
                let mut cv = Canvas::new(HERO_PX, HERO_PX);
                // Dots are for seeing the game through the map; the hero sits on the panel.
                let dots = std::mem::replace(&mut state.dots, false);
                draw_map(
                    &mut cv,
                    &state,
                    world,
                    &view,
                    &things,
                    icons.as_ref(),
                    &footprints,
                    &goals,
                    &Default::default(),
                    relief.as_deref(),
                );
                state.dots = dots;
                let mut hero = shared.hero.lock().unwrap();
                let n = hero.as_ref().map_or(0, |h| h.2) + 1;
                *hero = Some((HERO_PX, cv.px, n));
            }
        }

        // The Map page's preview: the map the settings make now, the big map's when that
        // is the display (its opacity too), else the minimap's.
        if tick % PREVIEW_EVERY == 0
            && shared.visible.load(Ordering::SeqCst)
            && shared.preview_wanted.load(Ordering::SeqCst)
        {
            if let (Some((p, yaw)), Some(world)) = (here, world.as_deref()) {
                let big = state.display == Display::Big;
                let (radius, heading_up, outline) = if big {
                    (state.big_radius_m, false, state.big_outline)
                } else {
                    (state.radius_m, state.heading_up, state.mini_outline)
                };
                let side = MAP_PX as usize;
                let view = View {
                    center: p,
                    yaw_deg: yaw,
                    heading_up,
                    scale: (side as f32 / 2.0 - 14.0) / (radius * 100.0),
                    north_deg: state.north_yaw,
                    outline,
                };
                let relief = baking.relief(&state, p, &obstacles);
                let goals = hud::with_pins(&goals, &state, world);
                let mut cv = Canvas::new(side, side);
                draw_map(
                    &mut cv,
                    &state,
                    world,
                    &view,
                    &things,
                    icons.as_ref(),
                    &footprints,
                    &goals,
                    &Default::default(),
                    relief.as_deref(),
                );
                if big && state.big_alpha < 100 {
                    // Premultiplied: every channel scales with the opacity.
                    let a = state.big_alpha as u32;
                    for px in cv.px.iter_mut() {
                        let c = |shift: u32| ((*px >> shift & 0xFF) * a / 100) << shift;
                        *px = c(24) | c(16) | c(8) | c(0);
                    }
                }
                let mut preview = shared.preview.lock().unwrap();
                let n = preview.as_ref().map_or(0, |h| h.2) + 1;
                *preview = Some((side, cv.px, n));
            }
        }

        let window = game_window(game).filter(|_| focused && !menu).map(|(_, r)| r);
        match (here, world.as_deref(), window) {
            (Some((p, yaw)), Some(world), Some(r)) => {
                state.observe(world, p);
                if marker_now {
                    let added = state.toggle_marker(world, p);
                    crate::logfile::line(&format!(
                        "minimap: marker {} at ({:.0}, {:.0}, {:.0}) in {world}",
                        if added { "added" } else { "removed" },
                        p[0],
                        p[1],
                        p[2]
                    ));
                }
                let goals = hud::with_pins(&goals, &state, world);
                let chosen = state.quest.clone();
                settle_target(
                    &mut state,
                    &goals,
                    p,
                    crate::quests::followed(&journal, chosen.as_deref()),
                    &journal,
                    &route.blocked,
                );
                if cycle_now {
                    cycle(&mut state, &goals, p);
                }

                // The route to the goal, when it is due.
                let goal = state.target.and_then(|t| goals.iter().find(|g| g.id == t)).filter(|_| state.route);
                let trail = || {
                    state
                        .trails
                        .get(world)
                        .map(|t| t.iter().flatten().map(|q| [q[0], q[1]]).collect())
                        .unwrap_or_default()
                };
                route.follow(goal, p, trail, &obstacles, &nav);
                let path = route.drawn(p);
                *shared.route_uncertain.lock().unwrap() = path.uncertain();

                let relief = baking.relief(&state, p, &obstacles);

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
                    let pins = hud::compass_pins(&goals, &state, world, p, &path);
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
                    // A deadline due now comes first; then what it still needs.
                    let mut line = hud::needs_line(followed, &needs, &deadlines, world);
                    if puzzle_near {
                        let hint = tr!("PUZZLE_NEARBY_THE_ANSWER_IS_IN");
                        line = if line.is_empty() { hint.to_string() } else { format!("{line}\n{hint}") };
                    }
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
