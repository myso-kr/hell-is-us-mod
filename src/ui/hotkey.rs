//! ` (~, VK_OEM_3): show or hide the panel — the key games open their console with.
//!
//! Polled rather than registered. `RegisterHotKey` would take the key from every
//! other program for as long as the panel runs; polling only acts while the game or
//! the panel has focus, and leaves it alone everywhere else.
//!
//! Showing and hiding go straight to the window with Win32 calls. eframe stops
//! running its frame loop while its window is hidden, so a request routed through
//! it could wait for a frame that never comes.

use super::Shared;
use eframe::egui;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_OEM_3};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, FindWindowW, SW_SHOWNOACTIVATE, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE,
};

pub(super) fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

/// The game's main window: its largest visible top-level window.
pub(super) fn game_window(pid: u32) -> Option<(HWND, RECT)> {
    struct Search {
        pid: u32,
        best: Option<(HWND, RECT)>,
    }
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let s = &mut *(lp as *mut Search);
        let mut r: RECT = std::mem::zeroed();
        if IsWindowVisible(hwnd) != 0 && pid_of(hwnd) == s.pid && GetWindowRect(hwnd, &mut r) != 0 {
            let area = |r: &RECT| (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
            if s.best.is_none_or(|(_, b)| area(&r) > area(&b)) {
                s.best = Some((hwnd, r));
            }
        }
        1
    }
    let mut s = Search { pid, best: None };
    unsafe { EnumWindows(Some(each), &mut s as *mut Search as LPARAM) };
    s.best
}

/// The console's own window (panel.rs `console_window`), found by its title. It is an
/// egui viewport, so it goes and comes back with the panel here: eframe draws no frames
/// while the panel is hidden, and would leave it up.
fn console_window() -> HWND {
    let title: Vec<u16> = "Hell Is Us Mod — console".encode_utf16().chain([0]).collect();
    unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }
}

/// Put the console right under the panel. Both are topmost; left alone, the console
/// came up over the panel whenever it took the keyboard, and the panel went back over it
/// at the next re-assertion — the panel blinked behind and in front.
pub(super) fn console_under(panel: HWND) {
    let console = console_window();
    if !console.is_null() && !panel.is_null() {
        unsafe { SetWindowPos(console, panel, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
    }
}

pub fn hide(shared: &Shared) {
    let hwnd = shared.hwnd.load(Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
        return;
    }
    unsafe { ShowWindow(hwnd, SW_HIDE) };
    let console = console_window();
    if !console.is_null() {
        unsafe { ShowWindow(console, SW_HIDE) };
    }
    shared.visible.store(false, Ordering::SeqCst);
    // Hand the keyboard back, so the next keypress goes to the game.
    if let Some((game, _)) = game_window(shared.game_pid.load(Ordering::SeqCst)) {
        unsafe { SetForegroundWindow(game) };
    }
}

/// Top-left of the game window, clear of its title bar if it has one. `None` until
/// the game has a real window — the launch splash is a small one first.
fn over_game(pid: u32) -> Option<(i32, i32)> {
    let (_, r) = game_window(pid)?;
    (r.right - r.left >= 640).then_some((r.left + 24, r.top + 48))
}

/// Take focus from the game. Windows refuses `SetForegroundWindow` to a process that
/// did not receive the last input — and the game did, the panel's key included. Sharing the
/// game's input state for the length of the call lifts that; without it the panel
/// would show but the game would keep the mouse, so nothing on it could be clicked.
fn take_focus(hwnd: HWND) {
    unsafe {
        let them = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let me = GetCurrentThreadId();
        let joined = them != 0 && them != me && AttachThreadInput(me, them, 1) != 0;
        SetForegroundWindow(hwnd);
        BringWindowToTop(hwnd);
        if joined {
            AttachThreadInput(me, them, 0);
        }
    }
}

fn show(shared: &Shared, ctx: &egui::Context) {
    let hwnd = shared.hwnd.load(Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
        return;
    }
    let saved = *shared.pos.lock().unwrap();
    let (x, y) = saved.or_else(|| over_game(shared.game_pid.load(Ordering::SeqCst))).unwrap_or((40, 40));
    unsafe { SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_SHOWWINDOW) };
    let console = console_window();
    if !console.is_null() {
        unsafe { ShowWindow(console, SW_SHOWNOACTIVATE) };
    }
    take_focus(hwnd);
    shared.visible.store(true, Ordering::SeqCst);
    ctx.request_repaint();
}

/// Every second while showing: back on top. A borderless game re-asserts its own
/// z-order when it takes focus, and would otherwise end up over the panel.
const ON_TOP_EVERY: u32 = 33;

pub fn watch(shared: Arc<Shared>, ctx: egui::Context) {
    let mut was_down = false;
    // The game the panel was last moved over; a new pid moves it again. PLAYER: it
    // is where the player left it last time, and stays there.
    const PLAYER: u32 = u32::MAX;
    let mut placed = 0;
    let mut tick = 0u32;
    while !shared.quit.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(30));
        tick = tick.wrapping_add(1);
        let hwnd = shared.hwnd.load(Ordering::SeqCst) as HWND;
        let game = shared.game_pid.load(Ordering::SeqCst);
        let visible = shared.visible.load(Ordering::SeqCst);
        if !hwnd.is_null() && visible {
            let saved = *shared.pos.lock().unwrap();
            if placed == 0 {
                if let Some((x, y)) = saved {
                    unsafe { SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE) };
                    placed = PLAYER;
                }
            }
            if game != 0 && game != placed && placed != PLAYER {
                if let Some((x, y)) = over_game(game) {
                    unsafe { SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE) };
                    placed = game;
                }
            } else if tick % ON_TOP_EVERY == 0 {
                unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
                console_under(hwnd);
                // Once placed, wherever the panel is now is where the player wants it.
                let mut r: RECT = unsafe { std::mem::zeroed() };
                if placed != 0 && unsafe { GetWindowRect(hwnd, &mut r) } != 0 && saved != Some((r.left, r.top)) {
                    *shared.pos.lock().unwrap() = Some((r.left, r.top));
                }
            }
        }

        let down = unsafe { GetAsyncKeyState(VK_OEM_3 as i32) } as u16 & 0x8000 != 0;
        if down && !was_down {
            let focus = pid_of(unsafe { GetForegroundWindow() });
            if focus == std::process::id() || (game != 0 && focus == game) {
                if visible {
                    hide(&shared);
                } else {
                    show(&shared, &ctx);
                }
            }
        }
        was_down = down;
    }
}
