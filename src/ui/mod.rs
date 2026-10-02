//! The overlay: a small always-on-top panel over the game, shown and hidden with ` (~).
//!
//! Three threads, one job each:
//!
//! ```text
//! worker   owns the Engine. Steps it ten times a second and runs what the panel asks
//! hotkey   watches ` (~) while the game or the panel has focus; shows and hides the window
//! minimap  the map window: F9 shows/hides it, F6 drops a marker (both changeable); reads snapshots only
//! ui       eframe. Draws the last snapshot and sends requests — never touches the game
//! ```
//!
//! It is also the launcher: started while the game is not running, it starts the
//! game, attaches once the game is up, and closes itself when the game exits.
//!
//! Closing the panel stops every toggle and puts the originals back. Killing the
//! process skips that, as it does for `hold`; `hiumod restore` covers it the same way.

mod console;
mod hotkey;
mod layered;
mod minimap;
mod panel;
mod pen;
mod tracker;
mod tw;

use crate::cheats::Active;
use crate::engine::{Engine, Snapshot};
use crate::game::{launch, locate, process::Game};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub enum Request {
    Set(&'static str, f32),
    Toggles(Vec<Active>),
    Restore,
    /// Remember where the hero stands, in a slot; go back to one.
    SavePosition(usize),
    LoadPosition(usize),
    Quit,
}

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
    /// run: this panel started the game and is waiting for it.
    pub launched: AtomicBool,
    /// minimap and panel: the map's trail, markers and settings.
    pub map: Mutex<crate::minimap::MapState>,
    /// overlay: the guide's route goes through an obstacle somewhere (no way in found).
    pub route_uncertain: Mutex<bool>,
    /// overlay: the menu signals last seen — (game cursor showing, game paused).
    pub menu: Mutex<(bool, bool)>,
    /// hotkey: where the panel is — the player's place for it, kept across runs.
    pub pos: Mutex<Option<(i32, i32)>>,
    pub quit: AtomicBool,
}

/// Back the saves up after each write the game makes (backup.rs), until the panel quits.
fn backups(shared: Arc<Shared>) {
    let mut watch = crate::backup::Watch::default();
    while !shared.quit.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_secs(2));
        match watch.poll() {
            Some(Ok(to)) => crate::journal::line(&format!("save backed up to {}", to.display())),
            Some(Err(e)) => crate::journal::line(&format!("save backup failed: {e}")),
            None => {}
        }
    }
}

/// Ten readings a second: the minimap turns with the camera from these, and at four
/// a second it visibly stepped.
const STEP: Duration = Duration::from_millis(100);

fn worker(shared: Arc<Shared>, rx: Receiver<Request>, ctx: eframe::egui::Context) {
    use crate::journal::line as log;
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
    loop {
        match rx.recv_timeout(STEP) {
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
                Ok(p) => reply(true, format!("위치 {} 저장 ({:.0}, {:.0}, {:.0})", i + 1, p[0], p[1], p[2])),
                Err(e) => reply(false, e),
            },
            Ok(Request::LoadPosition(i)) => match engine.load_position(i) {
                Ok(()) => reply(true, format!("위치 {} 로 이동", i + 1)),
                Err(e) => reply(false, e),
            },
            Ok(Request::Restore) => match engine.stop() {
                Ok(()) => reply(true, "originals restored".into()),
                Err(e) => reply(false, e),
            },
            Err(RecvTimeoutError::Timeout) => {}
        }
        let snap = engine.step();
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
        ctx.request_repaint();
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
    let windowed = drop_own_console();
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
    *shared.map.lock().unwrap() = minimap::load();
    if launch && Game::find()?.is_none() {
        locate::find(None)?;
        launch::launch()?;
        shared.launched.store(true, Ordering::SeqCst);
    }
    let (tx, rx) = mpsc::channel();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Hell Is Us Mod")
            .with_inner_size([panel::WIDTH, 560.0])
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
                panel::install_style(&cc.egui_ctx);
                cc.egui_ctx.set_visuals(eframe::egui::Visuals::dark());
                let (s, c) = (shared.clone(), cc.egui_ctx.clone());
                threads.push(std::thread::spawn(move || worker(s, rx, c)));
                let (s, c) = (shared.clone(), cc.egui_ctx.clone());
                threads.push(std::thread::spawn(move || hotkey::watch(s, c)));
                let s = shared.clone();
                threads.push(std::thread::spawn(move || minimap::run(s)));
                let s = shared.clone();
                threads.push(std::thread::spawn(move || backups(s)));
                Ok(Box::new(panel::Panel::new(shared, tx)))
            }),
        )
    };

    shared.quit.store(true, Ordering::SeqCst);
    let _ = tx.send(Request::Quit);
    for t in threads {
        let _ = t.join();
    }
    result.map_err(|e| format!("overlay: {e}"))
}
