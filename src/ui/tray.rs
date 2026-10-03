//! The notification-area icon, and the one-panel-at-a-time rule.
//!
//! The icon lives on its own thread with a hidden window of its own for the shell's
//! callbacks. A left click shows or hides the panel as ` (~) does; a right click opens
//! a small menu — show/hide, and quit. Quit posts the panel the same WM_CLOSE the
//! worker uses when the game exits, so it ends the way × does: eframe closes, and
//! `panel_and_launch` stops the worker, which puts the originals back.
//!
//! The window is a hidden top-level one rather than a message-only one: only top-level
//! windows hear `TaskbarCreated`, and the icon has to be added again when Explorer
//! restarts. A second `hiumod` finds the first through this window's class and asks
//! it to show the panel.

use super::Shared;
use eframe::egui;
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, GetWindowThreadProcessId, LoadIconW,
    PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu,
    TranslateMessage, HICON, IDI_APPLICATION, MF_SEPARATOR, MF_STRING, MSG, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
    WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW,
};

/// The tray window's class: how a second instance finds the first.
const CLASS: &str = "hiumod.tray";
/// The mutex only one panel can hold. `Local\`: one panel per signed-in session.
const MUTEX: &str = "Local\\hiumod.panel";
/// Sent by a second instance to the first's tray window: show the panel.
const SHOW_MESSAGE: &str = "hiumod.show";
const TIP: &str = "Hell Is Us Mod";
/// The shell's callback message for our icon.
const CALLBACK: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

const MENU_TOGGLE: usize = 1;
const MENU_QUIT: usize = 2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// What a click on the icon asks for, from the mouse message the shell passes on.
#[derive(Debug, PartialEq, Eq)]
enum Click {
    Toggle,
    Menu,
}

fn click(mouse: u32) -> Option<Click> {
    match mouse {
        WM_LBUTTONUP => Some(Click::Toggle),
        WM_RBUTTONUP | WM_CONTEXTMENU => Some(Click::Menu),
        _ => None,
    }
}

/// The tooltip, cut to what NOTIFYICONDATAW holds with its terminating zero.
fn tip_field(text: &str) -> [u16; 128] {
    let mut tip = [0u16; 128];
    for (slot, unit) in tip.iter_mut().zip(text.encode_utf16().take(127)) {
        *slot = unit;
    }
    tip
}

/// The menu's first item, by whether the panel shows now.
fn toggle_label(visible: bool) -> &'static str {
    if visible {
        tr!("TRAY_HIDE_PANEL")
    } else {
        tr!("TRAY_SHOW_PANEL")
    }
}

// ---- one panel at a time ----

/// Take the panel's mutex. `false` when another panel already holds it — then that
/// panel has been asked to show itself, and this one should go quietly. The handle is
/// never closed: it is held for the life of the process, and Windows lets go at exit.
pub fn claim() -> bool {
    let name = wide(MUTEX);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    let taken = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if handle.is_null() || !taken {
        // A mutex that could not be made at all must not keep the panel from opening.
        return true;
    }
    crate::logfile::line("panel already running — showing that one instead");
    show_other();
    false
}

/// Ask the running panel to show itself, through its tray window. Started by the
/// player, this process may hand it the foreground, so the panel comes up in front.
fn show_other() {
    let class = wide(CLASS);
    let hwnd = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if hwnd.is_null() {
        return;
    }
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            AllowSetForegroundWindow(pid);
        }
        PostMessageW(hwnd, show_message(), 0, 0);
    }
}

fn show_message() -> u32 {
    let name = wide(SHOW_MESSAGE);
    unsafe { RegisterWindowMessageW(name.as_ptr()) }
}

// ---- the icon ----

thread_local! {
    /// The tray thread's state, for its window procedure.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct State {
    shared: Arc<Shared>,
    ctx: egui::Context,
    icon: HICON,
    show_message: u32,
    taskbar_created: u32,
}

/// The exe's own icon (resource 1) if it has one, else the stock application icon.
/// The mod's mark (app_icon.rs) at the tray's size; Windows' generic icon if that fails.
fn app_icon() -> HICON {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as usize;
    if let Some(icon) = crate::ui::app_icon::hicon(size) {
        return icon;
    }
    unsafe {
        let own = LoadIconW(GetModuleHandleW(std::ptr::null()), 1 as _);
        if !own.is_null() {
            return own;
        }
        LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid
}

fn add_icon(hwnd: HWND, icon: HICON) {
    let mut nid = icon_data(hwnd);
    nid.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE;
    nid.uCallbackMessage = CALLBACK;
    nid.hIcon = icon;
    nid.szTip = tip_field(TIP);
    if unsafe { Shell_NotifyIconW(NIM_ADD, &nid) } == 0 {
        crate::logfile::line("tray: could not add the icon");
    }
}

fn remove_icon(hwnd: HWND) {
    let nid = icon_data(hwnd);
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

fn toggle(state: &State) {
    if state.shared.visible.load(Ordering::SeqCst) {
        super::hotkey::hide(&state.shared);
    } else {
        super::hotkey::show(&state.shared, &state.ctx);
    }
}

fn menu(hwnd: HWND, state: &State) {
    let toggle_text = wide(toggle_label(state.shared.visible.load(Ordering::SeqCst)));
    let quit_text = wide(tr!("TRAY_QUIT"));
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_STRING, MENU_TOGGLE, toggle_text.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_QUIT, quit_text.as_ptr());
        let mut at = POINT { x: 0, y: 0 };
        GetCursorPos(&mut at);
        // The menu closes on a click elsewhere only while its owner is in front; the
        // WM_NULL after it is the documented fix for its second opening.
        SetForegroundWindow(hwnd);
        let chosen =
            TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, at.x, at.y, 0, hwnd, std::ptr::null()) as usize;
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        match chosen {
            MENU_TOGGLE => toggle(state),
            MENU_QUIT => {
                crate::logfile::line("tray: quit");
                super::close(&state.shared);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // These need no state, and WM_DESTROY arrives inside DestroyWindow — while the
    // state may be taken out below.
    match msg {
        WM_CLOSE => {
            DestroyWindow(hwnd);
            return 0;
        }
        WM_DESTROY => {
            remove_icon(hwnd);
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    // Taken out for the call, so a re-entrant message (the menu runs its own loop) finds
    // nothing rather than a borrowed RefCell.
    let Some(state) = STATE.with(|s| s.borrow_mut().take()) else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    let mut result = 0;
    match msg {
        CALLBACK => match click(lp as u32 & 0xFFFF) {
            Some(Click::Toggle) => toggle(&state),
            Some(Click::Menu) => menu(hwnd, &state),
            None => {}
        },
        m if m != 0 && m == state.show_message => super::hotkey::show(&state.shared, &state.ctx),
        m if m != 0 && m == state.taskbar_created => add_icon(hwnd, state.icon),
        _ => result = DefWindowProcW(hwnd, msg, wp, lp),
    }
    STATE.with(|s| *s.borrow_mut() = Some(state));
    result
}

/// The tray thread: the icon until the panel quits (`stop`).
pub fn run(shared: Arc<Shared>, ctx: egui::Context) {
    let class = wide(CLASS);
    let taskbar = wide("TaskbarCreated");
    let icon = app_icon();
    let state = State {
        shared: shared.clone(),
        ctx,
        icon,
        show_message: show_message(),
        taskbar_created: unsafe { RegisterWindowMessageW(taskbar.as_ptr()) },
    };
    STATE.with(|s| *s.borrow_mut() = Some(state));
    let hwnd = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(procedure);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        // Never shown; WS_EX_TOOLWINDOW keeps it off the taskbar and Alt+Tab regardless.
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        crate::logfile::line("tray: could not create its window");
        return;
    }
    shared.tray.store(hwnd as isize, Ordering::SeqCst);
    add_icon(hwnd, icon);
    // `stop` may have run before the window was stored: it found nothing to close.
    if shared.quit.load(Ordering::SeqCst) {
        unsafe { DestroyWindow(hwnd) };
    }
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    shared.tray.store(0, Ordering::SeqCst);
    STATE.with(|s| *s.borrow_mut() = None);
}

/// Take the icon down and end the tray thread. Called once `shared.quit` is set.
pub fn stop(shared: &Shared) {
    let hwnd = shared.tray.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe { PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_left_click_toggles_and_a_right_click_opens_the_menu() {
        assert_eq!(click(WM_LBUTTONUP), Some(Click::Toggle));
        assert_eq!(click(WM_RBUTTONUP), Some(Click::Menu));
        assert_eq!(click(WM_CONTEXTMENU), Some(Click::Menu));
        // Moves and presses pass by: acting on the release only.
        assert_eq!(click(0x0200), None);
        assert_eq!(click(0x0201), None);
    }

    #[test]
    fn the_tooltip_is_cut_to_fit_with_its_terminating_zero() {
        let tip = tip_field(TIP);
        assert_eq!(String::from_utf16_lossy(&tip[..TIP.len()]), TIP);
        assert_eq!(tip[TIP.len()], 0);
        let long = tip_field(&"x".repeat(300));
        assert_eq!(long[126], u16::from(b'x'));
        assert_eq!(long[127], 0);
    }
}
