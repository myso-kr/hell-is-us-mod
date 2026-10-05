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
pub(crate) mod context;
mod glide;
mod hud;
mod marker;
mod route;
mod screenroute;
mod trace;

use super::banner;
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
/// How far above the screen's middle the game's right-middle notices reach, on its
/// 1080-tall layout (`HUD_SecretStarted…`, `HUD_CombatItemPickUp…`: anchored at the
/// right edge's middle): the tracker ends above it.
const NOTICE_HALF: f32 = 200.0;
/// How long the banner shows, and a key item's note.
/// The goals the player agreed to be guided to, and how many were left out: (hidden
/// places, puzzles' answers). Without the guide, none.
fn by_consent(
    goals: &Arc<Vec<crate::goals::Goal>>,
    consent: crate::settings::Consent,
) -> (Arc<Vec<crate::goals::Goal>>, (usize, usize)) {
    use crate::goals::Reveal;
    use crate::settings::Consent;
    if !consent.has(Consent::GUIDE) && !consent.has(Consent::MAP) {
        return (Default::default(), (0, 0));
    }
    let ok = |r: Reveal| match r {
        Reveal::Nothing => true,
        Reveal::Places => consent.has(Consent::PLACES),
        Reveal::Answers => consent.has(Consent::ANSWERS),
    };
    if goals.iter().all(|g| ok(g.reveals)) {
        return (goals.clone(), (0, 0));
    }
    let places = goals.iter().filter(|g| g.reveals == Reveal::Places && !ok(g.reveals)).count();
    let answers = goals.iter().filter(|g| g.reveals == Reveal::Answers && !ok(g.reveals)).count();
    (Arc::new(goals.iter().filter(|g| ok(g.reveals)).cloned().collect()), (places, answers))
}

/// The barrier a route runs through: the first leg that goes through something, and a shut
/// barrier the graph knows within `BARRIER_NEAR` of it.
fn barrier_on<'a>(
    path: &crate::pathfind::Path,
    doors: &'a [crate::graph::DoorStep],
) -> Option<&'a crate::graph::DoorStep> {
    let k = path.through.iter().position(|&t| t)?;
    let (a, b) = (*path.points.get(k)?, *path.points.get(k + 1)?);
    let near = |p: [f32; 3]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx * dx + dy * dy;
        let t = if len > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len).clamp(0.0, 1.0) } else { 0.0 };
        (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy))
    };
    doors
        .iter()
        .map(|d| (near(d.at), d))
        .filter(|(m, _)| *m <= BARRIER_NEAR)
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, d)| d)
}

/// How near a route's blocked leg a barrier must stand to be what blocks it (cm).
const BARRIER_NEAR: f32 = 500.0;

/// How much nearer to a goal found blocked the hero comes (cm), or how long it is, before it
/// is tried again.
const BLOCKED_MOVED: f32 = 3000.0;
const BLOCKED_FOR: Duration = Duration::from_secs(180);
/// This many different goals found blocked within `ENCLOSED_WITHIN` from within
/// `ENCLOSED_NEAR` (cm) of one spot: the spot reads as closed, and blocked goals are not
/// believed for `DISTRUST_FOR`.
const ENCLOSED_GOALS: usize = 3;
const ENCLOSED_WITHIN: Duration = Duration::from_secs(5);
const ENCLOSED_NEAR: f32 = 1000.0;
const DISTRUST_FOR: Duration = Duration::from_secs(20);
const BANNER_FOR: Duration = Duration::from_secs(15);
const KEY_NOTE_FOR: Duration = Duration::from_secs(20);
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
/// The Guide page's map (`Shared::ops`): its side (px), and how often it is drawn.
const OPS_PX: usize = 400;
const OPS_EVERY: Duration = Duration::from_millis(500);
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
type Tracked = (Arc<Vec<Quest>>, Option<String>, bool, bool, String, Vec<(String, [u8; 3])>, Vec<context::Line>, i32);

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
    // The route in focus on the game's view (screenroute.rs), at most this often.
    let screen_route = screenroute::Painter::spawn();
    let mut tracker_cv = Canvas::new(tracker::W as usize, tracker::H as usize);
    let mut pen = Pen::new(tracker::W, tracker::H);
    let mut tracked: Option<Tracked> = None;
    // The banner under the compass (banner.rs): its window, canvas and pen, what it shows
    // and until when.
    let mut banner_window = Layered::new("hiumod-banner", "Hell Is Us Notice", banner::W, banner::H);
    let mut banner_cv = Canvas::new(banner::W as usize, banner::H as usize);
    let mut banner_pen = Pen::new(banner::W, banner::H);
    let mut banner_shown: Option<(String, String)> = None;
    let mut banner_until = Instant::now();
    let mut banner_used = 0;
    // What the deadlines' alert said last, so a new one shows again.
    let mut alerted = String::new();
    // Where the last session left off: shown once, on the first frame in play.
    let mut previously =
        shared.previous.lock().unwrap().clone().filter(|p| crate::session::now().saturating_sub(p.when) >= 30 * 60);
    // The context lines, worked out twice a second; the rods held, and a note on the
    // last one picked up, kept for a while.
    let mut context_lines: Vec<context::Line> = Vec::new();
    let mut context_at = Instant::now() - Duration::from_secs(1);
    let mut rods: Option<std::collections::BTreeSet<String>> = None;
    let mut key_note: Option<(String, Instant)> = None;
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
    let (mut hero_at, mut preview_at, mut ops_at) = (Instant::now(), Instant::now(), Instant::now());
    // The routes as last worked out, for the Guide page's map (drawn while the panel, not
    // the game, has the focus: the routes are worked out only in play).
    let mut last_drawn: Vec<crate::raster::Drawn> = Vec::new();
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
    // The goals a route found blocked, with where the hero was and when: kept after the
    // route is dropped. A route is kept only for what is followed, so without this a
    // blocked goal let go of looked open again the next frame, was picked again, found
    // blocked again — the guide flickering between two goals. Tried again once the hero
    // has come `BLOCKED_MOVED` nearer to it, or after `BLOCKED_FOR`: a door may have opened.
    // Nearer, not just elsewhere: with a speed cheat the hero went 60 m anywhere in seconds,
    // and the guide flickered all the same (seen in play).
    let mut blocked_seen: std::collections::HashMap<u64, ([f32; 3], Instant)> = Default::default();
    // What the guide works with, written out (trace.rs).
    let mut tracer = trace::Trace::default();
    // The goals found blocked lately, with where the hero was; and until when blocked
    // goals are not believed: when every way from where the hero stands goes through
    // something, it is the hero's spot (inside a building's rough hull: a porch, a
    // doorway) that reads as closed, not each goal — measured by Lake Cynon, where the
    // guide went through six goals in a second down to a key 25 m underground.
    let mut lately_blocked: std::collections::VecDeque<(Instant, u64, [f32; 3])> = Default::default();
    let mut distrust_until = Instant::now();
    // When a route was last found to run through a known barrier: then blocked goals are
    // that barrier's, not a closed spot's.
    let mut door_seen = Instant::now() - Duration::from_secs(60);
    // The step the guide took up for a barrier on its way, kept among the goals until done.
    let mut door_step: Option<crate::goals::Goal> = None;
    // The one-sided door last noted as seen from its locked side (once in the trace).
    let mut one_way_noted: Option<u64> = None;
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
        // Exclusive fullscreen, once a second: the shell knows (as Discord asks it), and only
        // then — borderless and windowed games are not reported.
        if tick % 20 == 1 {
            let mut state = 0;
            // SAFETY: writes the one value it is given.
            let full = unsafe { windows_sys::Win32::UI::Shell::SHQueryUserNotificationState(&mut state) } >= 0
                && state == windows_sys::Win32::UI::Shell::QUNS_RUNNING_D3D_FULL_SCREEN;
            shared.fullscreen.store(full && shared.game_pid.load(Ordering::SeqCst) != 0, Ordering::Relaxed);
        }

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
        ) = match crate::prof::timed("lock.snap", || shared.snap.lock().unwrap()).as_ref() {
            Some(s) => (
                s.pose_src,
                s.pose,
                s.world.clone(),
                shown.things(s, places),
                s.footprints.clone(),
                {
                    // What each goal gives away, against what the player agreed to; what is
                    // left out is counted for the guide's card (`Shared::withheld`).
                    let (kept, withheld) = by_consent(&s.goals, consent);
                    *shared.withheld.lock().unwrap() = withheld;
                    kept
                },
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
        let mut state = crate::prof::timed("lock.map", || shared.map.lock().unwrap());
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

        // The Guide page's map, while it shows: north up round the hero, as wide as the big
        // map's radius, with what is followed in its colours and their routes.
        if ops_at.elapsed() >= OPS_EVERY
            && shared.visible.load(Ordering::SeqCst)
            && shared.ops_wanted.load(Ordering::SeqCst)
        {
            ops_at = Instant::now();
            if let (Some((p, yaw)), Some(world)) = (here, world.as_deref()) {
                let view = View {
                    center: p,
                    yaw_deg: yaw,
                    heading_up: false,
                    scale: (OPS_PX as f32 / 2.0 - 14.0) / (state.big_radius_m * 100.0),
                    north_deg: state.north_yaw,
                    outline: false,
                    full: false,
                };
                let relief = baking.relief(&state, p, &obstacles);
                let goals = pinned.get(&goals, &state, world);
                let mut cv = Canvas::new(OPS_PX, OPS_PX);
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
                    &last_drawn,
                    relief.as_deref(),
                );
                state.dots = dots;
                let mut ops = shared.ops.lock().unwrap();
                let n = ops.as_ref().map_or(0, |o| o.2) + 1;
                *ops = Some((OPS_PX, cv.px, n, view));
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
                let mut goals = pinned.get(&goals, &state, world);
                // A barrier's step taken up: among the goals while its barrier still waits on
                // it; once done (its barrier opened, or its step no longer first), let go.
                let doors_now = shared.snap.lock().unwrap().as_ref().map(|s| s.doors.clone()).unwrap_or_default();
                if let Some(step) = &door_step {
                    let still = doors_now.iter().any(|d| d.goal.id == step.id);
                    if !still || state.auto != Some(step.id) {
                        if state.auto == Some(step.id) {
                            state.auto = None;
                            state.held = false;
                        }
                        door_step = None;
                    } else if !goals.iter().any(|g| g.id == step.id) {
                        let mut g = (*goals).clone();
                        g.push(step.clone());
                        goals = Arc::new(g);
                    }
                }

                // What can only be reached through something, by any route, remembered.
                for r in routes.values_mut() {
                    for (id, is) in r.verdicts() {
                        if is {
                            blocked_seen.insert(id, (p, Instant::now()));
                            lately_blocked.push_back((Instant::now(), id, p));
                        } else {
                            blocked_seen.remove(&id);
                        }
                    }
                }
                lately_blocked.retain(|(t, ..)| t.elapsed() < ENCLOSED_WITHIN);
                let here_blocked: std::collections::HashSet<u64> = lately_blocked
                    .iter()
                    .filter(|(_, _, at)| (at[0] - p[0]).hypot(at[1] - p[1]) < ENCLOSED_NEAR)
                    .map(|(_, id, _)| *id)
                    .collect();
                if here_blocked.len() >= ENCLOSED_GOALS && door_seen.elapsed() > Duration::from_secs(10) {
                    tracer.note_event(
                        &format!(
                            "every way from here goes through something ({} goals in {} s): the hero's spot \
                             reads as closed; blocked goals not believed for {} s",
                            here_blocked.len(),
                            ENCLOSED_WITHIN.as_secs(),
                            DISTRUST_FOR.as_secs()
                        ),
                        world,
                        p,
                    );
                    for id in &here_blocked {
                        blocked_seen.remove(id);
                    }
                    lately_blocked.clear();
                    distrust_until = Instant::now() + DISTRUST_FOR;
                    // What the cascade landed on is no choice: pick again, from the nearest
                    // wanted goal (one picked by hand is kept).
                    if !state.held {
                        state.auto = None;
                    }
                }
                if Instant::now() < distrust_until {
                    blocked_seen.clear();
                }
                blocked_seen.retain(|id, (at, when)| {
                    // how much nearer to the goal than where it was found blocked
                    let nearer = goals.iter().find(|g| g.id == *id).map_or(0.0, |g| {
                        (g.at[0] - at[0]).hypot(g.at[1] - at[1]) - (g.at[0] - p[0]).hypot(g.at[1] - p[1])
                    });
                    nearer < BLOCKED_MOVED && when.elapsed() < BLOCKED_FOR
                });
                let blocked: std::collections::HashSet<u64> = blocked_seen.keys().copied().collect();
                settle_target(&mut state, &goals, p, crate::quests::followed(&journal, None), &journal, &blocked);
                // Each quest followed to its own next goal (track.rs).
                state.resolve_quests(&goals, &journal, p, &blocked);
                // The cycle key: the focus through what is followed; with one or none, the
                // auto guide's next goal.
                if cycle_now && !state.cycle_focus() {
                    cycle(&mut state, &goals, p);
                }

                // A route to each thing followed, when due (the one in focus more often).
                // No guide asked for: nothing followed, no route, no ring.
                let followed = if consent.has(crate::settings::Consent::GUIDE) { state.followed() } else { Vec::new() };
                routes.retain(|id, _| followed.iter().any(|f| f.id == *id));
                let mut drawn: Vec<crate::raster::Drawn> = Vec::new();
                let mut uncertain = std::collections::HashSet::new();
                let mut notes: Vec<trace::RouteNote> = Vec::new();
                // The barrier the auto guide's route runs through, if one the graph knows.
                let doors = shared.snap.lock().unwrap().as_ref().map(|s| s.doors.clone()).unwrap_or_default();
                let mut through: Option<crate::graph::DoorStep> = None;
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
                        if f.focus {
                            *shared.route3d.lock().unwrap() = (r.drawn3d(p), f.colour.unwrap_or(g.tier.rgb()));
                        }
                        r.drawn(p)
                    } else {
                        Default::default()
                    };
                    if path.uncertain() {
                        uncertain.insert(f.id);
                        if f.track.is_none() && through.is_none() {
                            // Not when the step's own way is blocked too: then the two
                            // would take turns.
                            through = barrier_on(&path, &doors)
                                .filter(|d| d.goal.id != f.id && !blocked_seen.contains_key(&d.goal.id))
                                .filter(|d| {
                                    // A one-sided door from its locked side: not the way.
                                    let locked = d.opens_from.is_some_and(|o| {
                                        (p[0] - d.at[0]) * (o[0] - d.at[0]) + (p[1] - d.at[1]) * (o[1] - d.at[1]) < 0.0
                                    });
                                    // Blocked for real, not a closed spot: the rule for those
                                    // stays off while it is seen.
                                    if locked {
                                        door_seen = Instant::now();
                                    }
                                    if locked && one_way_noted != Some(d.goal.id) {
                                        one_way_noted = Some(d.goal.id);
                                        tracer.note_event(
                                            &format!(
                                                "{} opens from the other side only: not guided through it from here",
                                                d.label
                                            ),
                                            world,
                                            p,
                                        );
                                    }
                                    !locked
                                })
                                .cloned();
                        }
                    }
                    if let Some(end) = path.points.last() {
                        let short = (end[0] - g.at[0]).hypot(end[1] - g.at[1]) / 100.0;
                        notes.push((f.id, path.points.len(), path.uncertain(), (short * 10.0).round() / 10.0));
                    }
                    drawn.push(crate::raster::Drawn { id: f.id, path, colour: f.colour, focus: f.focus });
                }
                *shared.route_uncertain.lock().unwrap() = uncertain;
                last_drawn = drawn.clone();
                // Through a shut barrier: to what opens it first, held until it is done.
                if let Some(d) = through {
                    door_seen = Instant::now();
                    if state.auto != Some(d.goal.id) {
                        let was = state
                            .auto
                            .and_then(|a| goals.iter().find(|g| g.id == a))
                            .map(|g| g.label.clone())
                            .unwrap_or_default();
                        tracer.note_event(
                            &format!("the way to {was} runs through {}: first {}", d.label, d.chain),
                            world,
                            p,
                        );
                        state.auto = Some(d.goal.id);
                        state.held = true;
                        door_step = Some(d.goal.clone());
                    }
                }
                let story = crate::quests::followed(&journal, None);
                if let Some(t) = tracer.observe(&state, &goals, p, world, story, &journal, &blocked, &notes) {
                    *shared.trace.lock().unwrap() = t;
                }

                let relief = baking.relief(&state, p, &obstacles);

                // The maps are drawn from a copy (with this world's trail and pins), the lock let
                // go meanwhile: the panel waited on it up to 96 ms (measured) while the big map
                // drew.
                let mut drawing = state.for_drawing(world);
                drop(state);
                if drawing.display == Display::Big {
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
                            &drawing,
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
                        let alpha = (drawing.big_alpha as u32 * 255 / 100) as u8;
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
                    if drawing.display == Display::Mini {
                        let view = View {
                            center: p,
                            yaw_deg: yaw,
                            heading_up: drawing.heading_up,
                            scale: (MAP_PX as f32 / 2.0 - 14.0) / (drawing.radius_m * 100.0),
                            north_deg: drawing.north_yaw,
                            outline: drawing.mini_outline,
                            full: false,
                        };
                        // Dots are the big map's: the minimap is small, in a corner.
                        drawing.dots = false;
                        let t = crate::prof::span("mini.draw");
                        draw_map(
                            &mut map_cv,
                            &drawing,
                            world,
                            &view,
                            &things,
                            icons.as_ref(),
                            &footprints,
                            &goals,
                            &drawn,
                            relief.as_deref(),
                        );
                        drop(t);
                        let _t = crate::prof::span("mini.present");
                        crate::prof::timed("present.mini", || {
                            map_window.present(&map_cv, r.right - MAP_PX - MARGIN, r.top + MARGIN + 24)
                        });
                    } else {
                        map_window.hide();
                    }
                }

                state = crate::prof::timed("lock.map", || shared.map.lock().unwrap());

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
                            crate::prof::timed("present.marker", || {
                                w.present(&marker_cv, x - marker::CX, y - marker::CY)
                            });
                        }
                        None => w.hide(),
                    }
                }
                // The route in focus laid on the floor in the game's view, hidden where the
                // world hides it.
                // Drawn on a thread of its own from the camera as it is (screenroute::Painter):
                // handed here what to draw, at this loop's pace.
                {
                    let (route, route_colour) = shared.route3d.lock().unwrap().clone();
                    let route: Vec<[f32; 3]> =
                        if state.route && state.screen_route && consent.has(crate::settings::Consent::GUIDE) {
                            route
                        } else {
                            Vec::new()
                        };
                    // the maps' icons over what is near, as the minimap shows its kinds and sorts
                    let marks: Vec<crate::actors::Thing> =
                        if state.screen_marks && consent.has(crate::settings::Consent::HUD) {
                            things.iter().filter(|t| state.shows(t.sub)).copied().collect()
                        } else {
                            Vec::new()
                        };
                    // shown when the maps are (the map key's "off" hides it with them), while
                    // the camera can be read and the game is not paused
                    let pid = reader.as_ref().map(|r| r.pid);
                    let job = match (pose_src.filter(|_| cam.is_some()), pid, client) {
                        (Some(src), Some(pid), Some(c))
                            if !paused && state.display != Display::Off && (route.len() > 1 || !marks.is_empty()) =>
                        {
                            Some(screenroute::Job {
                                src,
                                pid,
                                client: c,
                                route,
                                scene: obstacles.clone(),
                                colour: route_colour,
                                hero: p,
                                things: marks,
                                icon_px: state.icon_px,
                            })
                        }
                        _ => None,
                    };
                    screen_route.set(job);
                }

                if state.compass {
                    let pins = hud::compass_pins(&goals, &state, world, p, &drawn);
                    draw_compass(&mut compass_cv, yaw - state.north_yaw, &pins);
                    let x = r.left + (r.right - r.left - COMPASS_W) / 2;
                    crate::prof::timed("present.compass", || compass_window.present(&compass_cv, x, r.top + 12));
                } else {
                    compass_window.hide();
                }
                if let (Some(w), Some(pen), true) =
                    (tracker_window.as_mut(), pen.as_mut(), state.tracker && consent.has(crate::settings::Consent::HUD))
                {
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
                    // What the place asks now (context.rs), twice a second; and a rod just
                    // picked up, said for a while.
                    if context_at.elapsed() >= Duration::from_millis(500) {
                        context_at = Instant::now();
                        let snap = shared.snap.lock().unwrap();
                        if let (Some(s), Some((h, _))) = (snap.as_ref(), here) {
                            let held = context::rods_held(s);
                            if let Some(before) = &rods {
                                if let Some(new) = held.difference(before).next() {
                                    key_note = context::key_note(s, new, h, world).map(|n| (n, Instant::now()));
                                }
                            }
                            rods = Some(held);
                            if key_note.as_ref().is_some_and(|(_, at)| at.elapsed() > KEY_NOTE_FOR) {
                                key_note = None;
                            }
                            let note = key_note.as_ref().map(|(n, _)| n.as_str());
                            context_lines = context::lines(s, &things, &goals, h, world, places, note);
                        }
                    }
                    // At the top right, under the minimap when it shows; no lower than the
                    // band where the game opens its notices (the right middle: secrets
                    // started, items picked up), on the game's 1080-tall layout scaled.
                    let top = r.top + MARGIN + 24;
                    let y = if state.display == Display::Mini { top + MAP_PX + 12 } else { top };
                    let scale = (r.bottom - r.top) as f32 / 1080.0;
                    let band = r.top + (r.bottom - r.top) / 2 - (NOTICE_HALF * scale) as i32;
                    let max_h = band - y;
                    let now = (
                        journal.clone(),
                        followed.map(|q| q.key.clone()),
                        near,
                        stuck,
                        line.clone(),
                        besides.clone(),
                        context_lines.clone(),
                        max_h,
                    );
                    if tracked.as_ref() != Some(&now) {
                        tracker_used = tracker::draw(
                            &mut tracker_cv,
                            pen,
                            &journal,
                            followed,
                            near,
                            stuck,
                            &line,
                            &besides,
                            &context_lines,
                            max_h,
                        );
                        tracked = Some(now);
                    }
                    if tracker_used > 0 {
                        crate::prof::timed("present.tracker", || {
                            w.present(&tracker_cv, r.right - tracker::W - MARGIN, y)
                        });
                    } else {
                        w.hide();
                    }
                } else if let Some(w) = tracker_window.as_mut() {
                    w.hide();
                }

                // The banner under the compass: a good deed the next story beat ends (when
                // the alert is new, for a while), else where the last session left off (on
                // the first frames in play).
                // A good deed about to be missed: only when asked for (Settings: missables).
                let alert = if consent.has(crate::settings::Consent::MISSABLES) {
                    crate::missables::alert(&deadlines).unwrap_or_default()
                } else {
                    String::new()
                };
                if alert != alerted {
                    alerted = alert.clone();
                    if !alert.is_empty() {
                        banner_shown = Some((tr!("BANNER_BEFORE_YOU_GO_ON").to_string(), alert));
                        banner_until = Instant::now() + BANNER_FOR;
                    }
                }
                if let (Some(p), Some(_)) = (previously.take(), here) {
                    if banner_shown.is_none() || Instant::now() >= banner_until {
                        let quest = p
                            .quest
                            .as_ref()
                            .and_then(|k| journal.iter().find(|q| q.key == *k))
                            .map(|q| q.name.clone())
                            .unwrap_or_default();
                        let ago = crate::session::now().saturating_sub(p.when);
                        let body = trf!(
                            "BANNER_PREVIOUSLY_BODY",
                            ago = super::panel::ago_text(ago),
                            place = crate::i18n::place(crate::survey::Survey::world_of(&p.world)),
                            quest = quest
                        );
                        banner_shown = Some((tr!("BANNER_PREVIOUSLY").to_string(), body));
                        banner_until = Instant::now() + BANNER_FOR;
                    } else {
                        previously = Some(p);
                    }
                }
                match (banner_window.as_mut(), banner_pen.as_mut(), &banner_shown) {
                    (Some(w), Some(bp), Some((title, body)))
                        if Instant::now() < banner_until && consent.has(crate::settings::Consent::HUD) =>
                    {
                        if banner_used == 0 {
                            let colour = if *title == tr!("BANNER_PREVIOUSLY") {
                                crate::raster::Rgba(0x5A, 0x9C, 0xE6, 255)
                            } else {
                                crate::raster::Rgba(0xE8, 0xC0, 0x6A, 255)
                            };
                            banner_used = banner::draw(&mut banner_cv, bp, title, body, colour);
                        }
                        let x = r.left + (r.right - r.left - banner::W) / 2;
                        let y = r.top + 12 + if state.compass { COMPASS_H + 6 } else { 0 };
                        w.present(&banner_cv, x, y);
                    }
                    (Some(w), ..) => {
                        w.hide();
                        banner_used = 0;
                    }
                    _ => {}
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
                    if let Some(w) = banner_window.as_ref() {
                        w.keep_on_top(panel);
                    }
                    if let Some(w) = big_window.as_ref() {
                        w.keep_on_top(panel);
                    }
                    for w in marker_windows.iter().flatten() {
                        w.keep_on_top(panel);
                    }
                    screen_route.keep_under(panel);
                }
            }
            _ => {
                map_window.hide();
                compass_window.hide();
                screen_route.set(None);
                for w in marker_windows.iter_mut().flatten() {
                    w.hide();
                }
                if let Some(w) = tracker_window.as_mut() {
                    w.hide();
                }
                if let Some(w) = banner_window.as_mut() {
                    w.hide();
                    banner_used = 0;
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
