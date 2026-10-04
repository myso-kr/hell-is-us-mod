//! The overlay thread: the minimap in the game window's top-right corner and the
//! compass strip at its top centre, both click-through layered windows
//! (ui/layered.rs) drawn by raster.rs — and the guide that picks what they point to.
//!
//! Not eframe viewports: eframe stops running frames while the panel is hidden (`),
//! and these have to keep drawing then.
//!
//! Keys, polled like the panel's ` and only while the game or the panel has focus, chosen in the
//! panel: step the map's display (F2), drop or remove a marker (F5), show/hide the
//! compass (F3), move the guide to the next place (F4).
//! Only the worker's snapshot is read here — never the game's memory.
//!
//! While a game menu is open — the game shows its mouse cursor, or is paused — every
//! overlay hides (panel setting), so the inventory and menus are never covered.
//!
//! The walking route to the guide's goal (route.rs), the landscape bake (bake.rs) and
//! what the windows show (hud.rs) are worked out beside this loop.

mod bake;
mod bigmap;
mod glide;
mod hud;
mod marker;
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
/// While the big map shows: it covers the screen, so it is drawn more often (it costs
/// little a frame once its ground is drawn: see bigmap.rs).
const FRAME_BIG: Duration = Duration::from_millis(30);
/// The panel's hero map: its side (px), how far it reaches (m) and how often.
const HERO_PX: usize = 176;
const HERO_RADIUS_M: f32 = 120.0;
const HERO_EVERY: Duration = Duration::from_secs(1);
/// The Map page's preview: redrawn this often while the page shows, so a setting moved
/// is seen at once.
const PREVIEW_EVERY: Duration = Duration::from_millis(200);
/// The big map's preview width (px): the panel shows it at about this size.
const PREVIEW_BIG_W: usize = 640;
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
/// near, whether the guided goal is blocked, the needs line, and the quests followed
/// besides with their colours.
type Tracked = (Arc<Vec<Quest>>, Option<String>, bool, bool, String, Vec<(String, [u8; 3])>);

/// A ring to draw in the game's view (marker.rs): where on the screen, the hero's
/// distance, the colour (`None`, the auto guide's) and whether in focus.
type Mark = (i32, i32, f32, Option<[u8; 3]>, bool);

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
    // The target marked in the game's view (marker.rs).
    let mut marker_windows: Vec<Option<Layered>> = Vec::new();
    let mut marker_cv = Canvas::new(marker::W as usize, marker::H as usize);
    let mut tracker_cv = Canvas::new(tracker::W as usize, tracker::H as usize);
    let mut pen = Pen::new(tracker::W, tracker::H);
    let mut tracked: Option<Tracked> = None;
    let mut tracker_used = 0;
    // The big map's window is made at the game window's size, and again if that changes.
    let mut big_window: Option<Layered> = None;
    let mut big_cv = Canvas::new(1, 1);
    // The big map is drawn at half size, then shown at full: these are its half-size
    // canvas and icons.
    let mut big_scroll = bigmap::Scroll::default();
    // Where the big map was last shown, and how faded: none while it is hidden.
    const NOT_SHOWN: (i32, i32, u8) = (i32::MIN, i32::MIN, 0);
    let mut big_at = NOT_SHOWN;
    let mut glide = glide::Glide::default();
    // The game opened for reading the pose each frame (player::PoseSource).
    let mut reader: Option<crate::game::process::Reader> = None;
    // What the maps show, kept between frames (hud::Shown, hud::Pinned).
    let (mut shown, mut pinned) = (hud::Shown::default(), hud::Pinned::default());
    let mut big_on = false;
    let (mut hero_at, mut preview_at) = (Instant::now(), Instant::now());
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
    // Half-size icons for the Map page's big-map preview, which is drawn small.
    let mut icons_small = make((icon_px / 2).max(8));
    let mut was = [false; 4];
    // A route per thing followed (guide/track.rs), and since when a goal followed has been
    // gone.
    let mut routes: std::collections::HashMap<u64, route::Route> = Default::default();
    let mut missing = std::collections::HashMap::new();
    let mut baking = bake::Baking::default();
    let mut saved = Instant::now();
    let mut tick = 0u32;
    let mut frame_start = Instant::now();
    while !shared.quit.load(Ordering::SeqCst) {
        pump();
        // A frame every FRAME, whatever the last one took to draw.
        let spent = frame_start.elapsed();
        let frame = if big_on { FRAME_BIG } else { FRAME };
        std::thread::sleep(frame.saturating_sub(spent).max(Duration::from_millis(5)));
        big_on = false;
        frame_start = Instant::now();
        // Where the frames' time goes, every half a minute (prof.rs).
        if let Some(l) = crate::prof::report("overlay", Duration::from_secs(30)) {
            crate::logfile::line(&l);
        }
        let _frame = crate::prof::span("frame");
        tick = tick.wrapping_add(1);

        let game = shared.game_pid.load(Ordering::SeqCst);
        let focus = pid_of(unsafe { GetForegroundWindow() });
        let in_game = game != 0 && focus == game;
        // Only while the game itself has focus: not while the panel does, nor anything else.
        let focused = in_game;
        // What the player agreed to (settings::Consent): without "where hidden things are"
        // the maps show the land, the enemies (seen in a fight anyway) and their own pins.
        let consent = crate::settings::Consent(shared.consent.load(Ordering::SeqCst));
        let places = consent.has(crate::settings::Consent::PLACES);
        let (
            pose_src,
            pose,
            world,
            things,
            footprints,
            goals,
            paused,
            obstacles,
            journal,
            nav,
            needs,
            deadlines,
            puzzle_near,
            slot_puzzles,
        ) = match shared.snap.lock().unwrap().as_ref() {
            Some(s) => (
                s.pose_src,
                s.pose,
                s.world.clone(),
                shown.things(s, places),
                s.footprints.clone(),
                if places { s.goals.clone() } else { Default::default() },
                s.paused,
                s.obstacles.clone(),
                s.journal.clone(),
                s.nav.clone(),
                s.needs.clone(),
                s.deadlines.clone(),
                hud::puzzle_near(s),
                s.slot_puzzles.clone(),
            ),
            None => (
                None,
                None,
                None,
                Default::default(),
                Default::default(),
                Default::default(),
                false,
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                false,
                Default::default(),
            ),
        };
        // The pose read now, from where the worker found it: the worker's own reading
        // comes only after its whole step, a second late with the guide's reading.
        let fast = pose_src.and_then(|src| {
            let _t = crate::prof::span("pose");
            if reader.as_ref().map(|r| r.pid) != Some(game) {
                reader = crate::game::process::Reader::open(game);
            }
            src.read(reader.as_ref()?)
        });
        let to_f32 = |(p, yaw): ([f64; 3], f64)| ([p[0] as f32, p[1] as f32, p[2] as f32], yaw as f32);
        // Read this frame, drawn as read; else the worker's readings, glided between
        // (glide.rs). The glide follows the frame's pose either way, to take over smoothly.
        let glided = glide.see(fast.or(pose).map(to_f32), Instant::now());
        let here = fast.map(to_f32).or(glided);

        let haze_links = shared.snap.lock().unwrap().as_ref().map(|s| s.haze_links.clone()).unwrap_or_default();
        let mut state = shared.map.lock().unwrap();
        if state.haze_links != *haze_links {
            state.haze_links = (*haze_links).clone();
        }
        // The panel changed the icon size: rasterise them again, once.
        if state.icon_px != icon_px {
            icon_px = state.icon_px;
            icons = make(icon_px);
            icons_small = make((icon_px / 2).max(8));
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
        if hero_at.elapsed() >= HERO_EVERY && shared.visible.load(Ordering::SeqCst) {
            hero_at = Instant::now();
            if let (Some((p, yaw)), Some(world)) = (here, world.as_deref()) {
                let view = View {
                    center: p,
                    yaw_deg: yaw,
                    heading_up: false,
                    scale: (HERO_PX as f32 / 2.0 - 14.0) / (HERO_RADIUS_M * 100.0),
                    north_deg: state.north_yaw,
                    outline: false,
                    full: false,
                };
                let relief = baking.relief(&state, p, &obstacles);
                let goals = pinned.get(&goals, &state, world);
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
                    &[],
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
        if preview_at.elapsed() >= PREVIEW_EVERY
            && shared.visible.load(Ordering::SeqCst)
            && shared.preview_wanted.load(Ordering::SeqCst)
        {
            preview_at = Instant::now();
            if let (Some((p, yaw)), Some(world)) = (here, world.as_deref()) {
                let relief = baking.relief(&state, p, &obstacles);
                let goals = pinned.get(&goals, &state, world);
                // The minimap, as it draws (solid: dots are the big map's).
                let side = MAP_PX as usize;
                let view = View {
                    center: p,
                    yaw_deg: yaw,
                    heading_up: state.heading_up,
                    scale: (side as f32 / 2.0 - 14.0) / (state.radius_m * 100.0),
                    north_deg: state.north_yaw,
                    outline: state.mini_outline,
                    full: false,
                };
                let mut cv = Canvas::new(side, side);
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
                    &[],
                    relief.as_deref(),
                );
                state.dots = dots;
                {
                    let mut preview = shared.preview.lock().unwrap();
                    let n = preview.as_ref().map_or(0, |h| h.2) + 1;
                    *preview = Some((side, cv.px, n));
                }
                // The big map, as it covers the game window, made small.
                if let Some((_, gr)) = game_window(game) {
                    // Drawn straight at the preview's size (the game window's shape), not
                    // at the screen's and made small: a fraction of the work.
                    let (gw, gh) = ((gr.right - gr.left).max(2) as usize, (gr.bottom - gr.top).max(2) as usize);
                    let w = PREVIEW_BIG_W.min(gw) & !1;
                    let h = (gh * w / gw).max(2) & !1;
                    let mut small = Canvas::new(w, h);
                    bigmap::frame(
                        &mut bigmap::Scroll::default(),
                        &mut small,
                        &state,
                        world,
                        (p, yaw),
                        &things,
                        icons_small.as_ref(),
                        &footprints,
                        &goals,
                        &[],
                        relief.as_ref(),
                    );
                    if state.big_alpha < 100 {
                        // Premultiplied: every channel scales with the opacity.
                        let a = state.big_alpha as u32;
                        for px in small.px.iter_mut() {
                            let c = |shift: u32| ((*px >> shift & 0xFF) * a / 100) << shift;
                            *px = c(24) | c(16) | c(8) | c(0);
                        }
                    }
                    let mut preview = shared.preview_big.lock().unwrap();
                    let n = preview.as_ref().map_or(0, |h| h.3) + 1;
                    *preview = Some((w, h, small.px, n));
                }
            }
        }

        // No map at all without the map's consent: no minimap, big map, compass or tracker.
        let game_win = game_window(game);
        let window =
            game_win.filter(|_| focused && !menu && consent.has(crate::settings::Consent::MAP)).map(|(_, r)| r);
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
                // The places followed matched to the goals of now, the gone let go: a goal
                // taken or done, a choice puzzle's groove once its set is done.
                let before = pinned.get(&goals, &state, world);
                state.relink(&before, world, &mut missing);
                for set in slot_puzzles.iter().flat_map(|p| &p.sets) {
                    if let (crate::slots::State::Done, Some(right)) = (set.state(), set.answer()) {
                        state.done_at(world, right.groove);
                    }
                }
                let goals = pinned.get(&goals, &state, world);

                // What can only be reached through something, by any route.
                let blocked: std::collections::HashSet<u64> =
                    routes.values().flat_map(|r| r.blocked.iter().copied()).collect();
                settle_target(&mut state, &goals, p, crate::quests::followed(&journal, None), &journal, &blocked);
                // Each quest followed to its own next goal (track.rs).
                state.resolve_quests(&goals, &journal, p, &blocked);
                // The cycle key: the focus through what is followed; with one or none, the
                // auto guide's next goal.
                if cycle_now && !state.cycle_focus() {
                    cycle(&mut state, &goals, p);
                }

                // A route to each thing followed, when due (the one in focus more often).
                let followed = state.followed();
                routes.retain(|id, _| followed.iter().any(|f| f.id == *id));
                let mut drawn: Vec<crate::raster::Drawn> = Vec::new();
                let mut uncertain = std::collections::HashSet::new();
                for f in &followed {
                    let Some(g) = goals.iter().find(|g| g.id == f.id) else { continue };
                    let path = if state.route {
                        let trail = || {
                            state
                                .trails
                                .get(world)
                                .map(|t| t.iter().flatten().map(|q| [q[0], q[1]]).collect())
                                .unwrap_or_default()
                        };
                        let r = routes.entry(f.id).or_default();
                        r.follow(Some(g), p, trail, &obstacles, &nav, !f.focus);
                        r.drawn(p)
                    } else {
                        Default::default()
                    };
                    if path.uncertain() {
                        uncertain.insert(f.id);
                    }
                    drawn.push(crate::raster::Drawn { id: f.id, path, colour: f.colour, focus: f.focus });
                }
                *shared.route_uncertain.lock().unwrap() = uncertain;

                let relief = baking.relief(&state, p, &obstacles);

                if state.display == Display::Big {
                    map_window.hide();
                    // The whole game window is its canvas: centred on the hero, fading out
                    // toward the edges (as Diablo's and Path of Exile's overlay maps).
                    // As wide as it is tall, the game window's short side: past the map's
                    // circle (raster::map_radius, fade_edges) nothing shows, and a window
                    // as wide as the screen drew, copied and composed 44 % more pixels,
                    // all of them clear, every frame.
                    let side = (r.right - r.left).min(r.bottom - r.top);
                    let (gw, gh) = (side, side);
                    let (bx, by) = (r.left + (r.right - r.left - side) / 2, r.top + (r.bottom - r.top - side) / 2);
                    if big_window.as_ref().is_none_or(|w| (w.w, w.h) != (gw, gh)) {
                        big_window = Layered::new("hiumod-bigmap", "Hell Is Us Map", gw, gh);
                        big_cv = Canvas::new(gw as usize, gh as usize);
                        big_scroll = bigmap::Scroll::default();
                        big_at = NOT_SHOWN;
                    }
                    if let Some(w) = big_window.as_mut() {
                        big_on = true;
                        let t = crate::prof::span("big.draw");
                        let drawn = bigmap::frame(
                            &mut big_scroll,
                            &mut big_cv,
                            &state,
                            world,
                            (p, yaw),
                            &things,
                            icons.as_ref(),
                            &footprints,
                            &goals,
                            &drawn,
                            relief.as_ref(),
                        );
                        drop(t);
                        // Shown again only when drawn anew, or moved, or faded otherwise.
                        let alpha = (state.big_alpha as u32 * 255 / 100) as u8;
                        if drawn || big_at != (bx, by, alpha) {
                            let _t = crate::prof::span("big.present");
                            w.present_alpha(&big_cv, bx, by, alpha);
                            big_at = (bx, by, alpha);
                        }
                    }
                } else {
                    if let Some(w) = big_window.as_mut() {
                        w.hide();
                    }
                    big_at = NOT_SHOWN;
                    if state.display == Display::Mini {
                        let view = View {
                            center: p,
                            yaw_deg: yaw,
                            heading_up: state.heading_up,
                            scale: (MAP_PX as f32 / 2.0 - 14.0) / (state.radius_m * 100.0),
                            north_deg: state.north_yaw,
                            outline: state.mini_outline,
                            full: false,
                        };
                        // Dots are the big map's: the minimap is small, in a corner.
                        let dots = std::mem::replace(&mut state.dots, false);
                        let t = crate::prof::span("mini.draw");
                        draw_map(
                            &mut map_cv,
                            &state,
                            world,
                            &view,
                            &things,
                            icons.as_ref(),
                            &footprints,
                            &goals,
                            &drawn,
                            relief.as_deref(),
                        );
                        state.dots = dots;
                        drop(t);
                        let _t = crate::prof::span("mini.present");
                        map_window.present(&map_cv, r.right - MAP_PX - MARGIN, r.top + MARGIN + 24);
                    } else {
                        map_window.hide();
                    }
                }

                // Each thing followed marked where it stands in the game's view, when near
                // (marker.rs): a ring in its colour, one window each.
                let cam = pose_src.and_then(|s| s.camera(reader.as_ref()?)).filter(|_| !paused);
                let client = game_win.and_then(|(h, _)| marker::client(h));
                let marks: Vec<Mark> = followed
                    .iter()
                    .filter_map(|f| {
                        let g = goals.iter().find(|g| g.id == f.id)?;
                        if (g.at[0] - p[0]).hypot(g.at[1] - p[1]) >= marker::NEAR {
                            return None;
                        }
                        let ((left, top, w, h), cam) = (client?, cam.as_ref()?);
                        let (x, y, _) = marker::project(cam, g.at, w as f32, h as f32)?;
                        // How far from the hero, not from the camera behind them.
                        let far =
                            ((g.at[0] - p[0]).powi(2) + (g.at[1] - p[1]).powi(2) + (g.at[2] - p[2]).powi(2)).sqrt();
                        let on = (0.0..w as f32).contains(&x) && (0.0..h as f32).contains(&y);
                        on.then_some((left + x as i32, top + y as i32, far, f.colour, f.focus))
                    })
                    .collect();
                while marker_windows.len() < marks.len() {
                    marker_windows.push(Layered::new("hiumod-marker", "Hell Is Us Marker", marker::W, marker::H));
                }
                for (i, w) in marker_windows.iter_mut().enumerate() {
                    let Some(w) = w.as_mut() else { continue };
                    match marks.get(i) {
                        Some(&(x, y, far, colour, focus)) => {
                            marker::draw(&mut marker_cv, far, colour, focus);
                            w.present(&marker_cv, x - marker::CX, y - marker::CY);
                        }
                        None => w.hide(),
                    }
                }

                if state.compass {
                    let pins = hud::compass_pins(&goals, &state, world, p, &drawn);
                    draw_compass(&mut compass_cv, yaw - state.north_yaw, &pins);
                    let x = r.left + (r.right - r.left - COMPASS_W) / 2;
                    compass_window.present(&compass_cv, x, r.top + 12);
                } else {
                    compass_window.hide();
                }
                if let (Some(w), Some(pen), true) = (tracker_window.as_mut(), pen.as_mut(), state.tracker) {
                    let followed = crate::quests::followed(&journal, state.focused_quest());
                    // Whether any place that moves the followed quest along is loaded.
                    let near = followed.is_none_or(|q| goals.iter().any(|g| g.serves(q)));
                    // The goal being guided to can only be reached through something.
                    let stuck = state.focused().is_some_and(|t| blocked.contains(&t));
                    // A deadline due now comes first; then what it still needs.
                    let mut line = hud::needs_line(followed, &needs, &deadlines, world);
                    if puzzle_near {
                        let hint = tr!("PUZZLE_NEARBY_THE_ANSWER_IS_IN");
                        line = if line.is_empty() { hint.to_string() } else { format!("{line}\n{hint}") };
                    }
                    // The quests followed besides, in their colours.
                    let besides: Vec<(String, [u8; 3])> =
                        state.tracks.iter().filter_map(|x| Some((x.quest.clone()?, x.rgb()))).collect();
                    let now =
                        (journal.clone(), followed.map(|q| q.key.clone()), near, stuck, line.clone(), besides.clone());
                    if tracked.as_ref() != Some(&now) {
                        tracker_used =
                            tracker::draw(&mut tracker_cv, pen, &journal, followed, near, stuck, &line, &besides);
                        tracked = Some(now);
                    }
                    if tracker_used > 0 {
                        // At the top right, under the minimap when it shows: the game opens
                        // its pop-ups at the right middle, where the tracker used to be.
                        let top = r.top + MARGIN + 24;
                        let y = if state.display == Display::Mini { top + MAP_PX + 12 } else { top };
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
                    for w in marker_windows.iter().flatten() {
                        w.keep_on_top(panel);
                    }
                }
            }
            _ => {
                map_window.hide();
                compass_window.hide();
                for w in marker_windows.iter_mut().flatten() {
                    w.hide();
                }
                if let Some(w) = tracker_window.as_mut() {
                    w.hide();
                }
                if let Some(w) = big_window.as_mut() {
                    w.hide();
                }
                big_at = NOT_SHOWN;
            }
        }
        if saved.elapsed() >= SAVE_EVERY {
            save(&mut state);
            saved = Instant::now();
        }
    }
    save(&mut shared.map.lock().unwrap());
}
