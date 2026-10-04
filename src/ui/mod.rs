//! The overlay: a small always-on-top panel over the game, shown and hidden with ` (~).
//!
//! Three threads, one job each:
//!
//! ```text
//! worker   owns the Engine. Steps it ten times a second and runs what the panel asks
//! hotkey   watches ` (~) while the game or the panel has focus; shows and hides the window
//! minimap  the map window: F2 steps its display, F5 drops a marker (changeable); reads snapshots only
//! ui       eframe. Draws the last snapshot and sends requests — never touches the game
//! tray     the notification-area icon: click to show/hide, right-click for a menu
//! ```
//!
//! One panel at a time: started again, it shows the running panel and exits (tray.rs).
//!
//! It is also the launcher: started while the game is not running, it starts the
//! game, attaches once the game is up, and closes itself when the game exits.
//!
//! Closing the panel stops every toggle and puts the originals back. Killing the
//! process skips that, as it does for `hold`; `hiumod restore` covers it the same way.

mod app_icon;
mod console;
mod hotkey;
mod layered;
mod overlay;
mod panel;
mod pen;
mod svg;
mod theme;
mod tracker;
mod tray;
mod tw;
mod update;

use crate::cheats::Active;
use crate::engine::{Engine, Snapshot};
use crate::game::{launch, locate, process::Game};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub enum Request {
    Set(&'static str, f32),
    Toggles(Vec<Active>),
    Restore,
    /// Remember where the hero stands, in a slot; go back to one.
    SavePosition(usize),
    LoadPosition(usize),
    Quit,
}

/// A square frame the overlay made for the panel: (side, premultiplied pixels, counter).
pub type Square = Option<(usize, Vec<u32>, u64)>;
/// A frame of any shape: (width, height, premultiplied pixels, counter).
pub type Frame = Option<(usize, usize, Vec<u32>, u64)>;

/// What the threads share. Each field has one writer.
#[derive(Default)]
pub struct Shared {
    /// worker: the last step.
    pub snap: Mutex<Option<Snapshot>>,
    /// worker: what the last request came to — (succeeded, text).
    pub reply: Mutex<Option<(bool, String)>>,
    /// worker: the game's pid, 0 when not attached. The hotkey's focus test.
    pub game_pid: AtomicU32,
    /// ui: the panel's window, once it exists.
    pub hwnd: AtomicIsize,
    /// hotkey: whether the panel is showing.
    pub visible: AtomicBool,
    /// panel: the console is open (toggled on). Whether it shows is the hotkey thread's
    /// call (hotkey.rs `watch`): egui runs no frames while nothing happens in its windows.
    pub console_open: AtomicBool,
    /// hotkey: the panel is off the screen, not hidden, so its frames run for the console
    /// open without it (`hotkey::park`).
    pub parked: AtomicBool,
    /// panel: the consent's bits (`settings::Consent`), 0 until chosen — what the overlay
    /// may draw.
    pub consent: std::sync::atomic::AtomicU8,
    /// panel: the splash shows (panel/splash.rs); the hotkey thread leaves the window's
    /// place and size alone till it goes.
    pub splash: AtomicBool,
    /// run: this panel started the game and is waiting for it.
    pub launched: AtomicBool,
    /// minimap and panel: the map's trail, markers and settings.
    pub map: Mutex<crate::minimap::MapState>,
    /// overlay: the guide's route goes through an obstacle somewhere (no way in found).
    pub route_uncertain: Mutex<std::collections::HashSet<u64>>,
    /// overlay: the menu signals last seen — (game cursor showing, game paused).
    pub menu: Mutex<(bool, bool)>,
    /// overlay: a north-up map round the hero for the panel's "now" page, redrawn about
    /// once a second while the panel shows — (side in px, premultiplied 0xAARRGGBB
    /// pixels, a counter that moves on with each new one).
    pub hero: Mutex<Option<(usize, Vec<u32>, u64)>>,
    /// panel: the Map page is showing, so the overlay draws `preview`.
    pub preview_wanted: AtomicBool,
    /// overlay: the minimap as its settings draw it now, for the Map page's preview —
    /// (side, pixels, counter).
    pub preview: Mutex<Square>,
    /// overlay: the big map as it covers the game window now, made small for the same
    /// page — (width, height, pixels, counter).
    pub preview_big: Mutex<Frame>,
    /// hotkey: where the panel is — the player's place for it, kept across runs.
    pub pos: Mutex<Option<(i32, i32)>>,
    /// tray: its hidden window, once it exists; 0 after it is gone.
    pub tray: AtomicIsize,
    pub quit: AtomicBool,
}

/// Back the saves up after each write the game makes (backup.rs), follow the game's
/// language (i18n), and log this process's memory once a minute — where a growth
/// would show (memstat.rs), until the panel quits.
fn backups(shared: Arc<Shared>) {
    let mut watch = crate::backup::Watch::default();
    let mut logged = std::time::Instant::now();
    while !shared.quit.load(Ordering::SeqCst) {
        crate::i18n::follow_game();
        std::thread::sleep(Duration::from_secs(2));
        if logged.elapsed() >= Duration::from_secs(60) {
            logged = std::time::Instant::now();
            if let (Some((ws, private)), Some(peak)) = (crate::memstat::now(), crate::memstat::peak()) {
                use crate::memstat::mb;
                crate::logfile::line(&format!(
                    "memory: working {} · private {} · peak {}",
                    mb(ws),
                    mb(private),
                    mb(peak)
                ));
            }
        }
        match watch.poll() {
            Some(Ok(to)) => crate::logfile::line(&format!("save backed up to {}", to.display())),
            Some(Err(e)) => crate::logfile::line(&format!("save backup failed: {e}")),
            None => {}
        }
    }
}

/// Ten readings a second: the minimap turns with the camera from these, and at four
/// a second it visibly stepped.
const STEP: Duration = Duration::from_millis(100);

fn worker(shared: Arc<Shared>, rx: Receiver<Request>, ctx: eframe::egui::Context) {
    use crate::logfile::line as log;
    let reply = |ok: bool, text: String| {
        log(&format!("{} {text}", if ok { "ok  " } else { "FAIL" }));
        *shared.reply.lock().unwrap() = Some((ok, text));
    };
    log("panel started");
    // What the log last said about each of these, so it records changes, not ticks.
    let (mut was_game, mut was_gate, mut was_notice) = (String::new(), String::new(), String::new());
    // Once the game has been seen, its exit is the panel's cue to go too.
    let mut seen = false;
    let mut engine = match Engine::new() {
        Ok(e) => e,
        Err(e) => {
            reply(false, e);
            return;
        }
    };
    // When the next step is due: steps start STEP apart (a request answered in between
    // does not move it). Waiting a whole STEP after each step made them ~160 ms apart.
    let mut due = Instant::now();
    loop {
        match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
            Ok(Request::Quit) | Err(RecvTimeoutError::Disconnected) => {
                match engine.stop() {
                    Ok(()) => log("panel closed — originals restored"),
                    Err(e) => log(&format!("panel closed — {e}")),
                }
                return;
            }
            Ok(Request::Set(name, v)) => match engine.set(name, v) {
                Ok(()) => reply(true, format!("{name} = {v}")),
                Err(e) => reply(false, e),
            },
            Ok(Request::Toggles(t)) => {
                let on: Vec<String> = t.iter().map(|a| format!("{}={}", a.cheat, a.value)).collect();
                log(&format!("on: [{}]", on.join(", ")));
                if let Err(e) = engine.set_active(t) {
                    reply(false, e);
                }
            }
            Ok(Request::SavePosition(i)) => match engine.save_position(i) {
                Ok(p) => reply(true, trf!("POSITION_SAVED", slot = i + 1, x = p[0], y = p[1], z = p[2])),
                Err(e) => reply(false, e),
            },
            Ok(Request::LoadPosition(i)) => match engine.load_position(i) {
                Ok(()) => reply(true, trf!("MOVED_TO_POSITION", slot = i + 1)),
                Err(e) => reply(false, e),
            },
            Ok(Request::Restore) => match engine.stop() {
                Ok(()) => reply(true, tr!("ORIGINAL_VALUES_RESTORED").into()),
                Err(e) => reply(false, e),
            },
            Err(RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() < due {
            continue;
        }
        due = Instant::now() + STEP;
        let snap = {
            let _t = crate::prof::span("step");
            engine.step()
        };
        // Where the steps' time went, every half a minute (prof.rs).
        if let Some(l) = crate::prof::report("worker", Duration::from_secs(30)) {
            log(&l);
        }
        let game = match &snap.game {
            Ok((pid, v)) => format!("game attached — v{v}, pid {pid}"),
            Err(e) => format!("game not attached — {e}"),
        };
        let gate = match &snap.gate {
            Ok(()) => "hero gate open".to_string(),
            Err(e) => e.clone(),
        };
        let notice = snap.notice.clone().unwrap_or_default();
        for (now, was) in [(game, &mut was_game), (gate, &mut was_gate), (notice, &mut was_notice)] {
            if now != *was {
                if !now.is_empty() {
                    log(&now);
                }
                *was = now;
            }
        }
        shared.game_pid.store(snap.game.as_ref().map_or(0, |g| g.0), Ordering::SeqCst);
        if snap.game.is_ok() {
            seen = true;
            shared.launched.store(false, Ordering::SeqCst);
        } else if seen {
            // Not a viewport command: eframe runs no frames while the panel is hidden,
            // and a hidden panel must close too. WM_CLOSE reaches it either way.
            close(&shared);
        }
        *shared.snap.lock().unwrap() = Some(snap);
        // Only a shown panel draws the new snapshot; showing it asks for a frame.
        if shared.visible.load(Ordering::SeqCst) {
            ctx.request_repaint();
        }
    }
}

fn close(shared: &Shared) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    let hwnd = shared.hwnd.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe { PostMessageW(hwnd as _, WM_CLOSE, 0, 0) };
    }
}

/// Launched by double-click, the console that opened with us is ours alone — close
/// it. Launched from a terminal, leave the terminal be. Returns whether it closed,
/// because then an error has nowhere to be printed and must be shown instead.
fn drop_own_console() -> bool {
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
    let mut pids = [0u32; 2];
    unsafe { GetConsoleProcessList(pids.as_mut_ptr(), 2) == 1 && FreeConsole() != 0 }
}

/// An error box, for when there is no console to print to.
pub fn alert(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide("Hell Is Us Mod"));
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
}

/// `launch`: start the game if it is not already running.
pub fn run(launch: bool) -> Result<(), String> {
    // Started by an update: the panel it replaces exits first.
    update::after_update();
    // Another panel is running: it has been asked to show itself; this one goes quietly.
    if !tray::claim() {
        return Ok(());
    }
    let windowed = drop_own_console();
    crate::i18n::follow_game();
    let result = panel_and_launch(launch);
    if let (true, Err(e)) = (windowed, &result) {
        alert(e);
    }
    result
}

fn panel_and_launch(launch: bool) -> Result<(), String> {
    let shared = Arc::new(Shared::default());
    shared.visible.store(true, Ordering::SeqCst);
    *shared.pos.lock().unwrap() = crate::settings::load().pos;
    *shared.map.lock().unwrap() = overlay::load();
    if launch && Game::find()?.is_none() {
        locate::find(None)?;
        launch::launch()?;
        shared.launched.store(true, Ordering::SeqCst);
    }
    let (tx, rx) = mpsc::channel();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Hell Is Us Mod")
            .with_icon(app_icon::icon_data(64))
            .with_inner_size([panel::WIDTH, 560.0])
            // Off the screen while the splash shows (panel/splash.rs): the panel lays
            // itself out there, its frames running as a shown window's do, and comes to
            // its place only once it has settled.
            .with_position([-30000.0, -30000.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top(),
        ..Default::default()
    };

    let mut threads = Vec::new();
    let result = {
        let (shared, tx, threads) = (shared.clone(), tx.clone(), &mut threads);
        eframe::run_native(
            "hiumod",
            options,
            Box::new(move |cc| {
                panel::install_fonts(&cc.egui_ctx);
                // The theme sets both of egui's styles; set_visuals after it would put the
                // stock dark grey back.
                theme::install(&cc.egui_ctx);
                let (s, c) = (shared.clone(), cc.egui_ctx.clone());
                threads.push(std::thread::spawn(move || worker(s, rx, c)));
                let (s, c) = (shared.clone(), cc.egui_ctx.clone());
                threads.push(std::thread::spawn(move || hotkey::watch(s, c)));
                let s = shared.clone();
                threads.push(std::thread::spawn(move || overlay::run(s)));
                let s = shared.clone();
                threads.push(std::thread::spawn(move || backups(s)));
                let (s, c) = (shared.clone(), cc.egui_ctx.clone());
                threads.push(std::thread::spawn(move || tray::run(s, c)));
                Ok(Box::new(panel::Panel::new(shared, tx)))
            }),
        )
    };

    shared.quit.store(true, Ordering::SeqCst);
    tray::stop(&shared);
    let _ = tx.send(Request::Quit);
    for t in threads {
        let _ = t.join();
    }
    result.map_err(|e| format!("overlay: {e}"))
}
