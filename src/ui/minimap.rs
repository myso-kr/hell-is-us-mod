//! The minimap window: a click-through, always-on-top layered window in the game
//! window's top-right corner, drawn by `raster::draw_map` on its own thread.
//!
//! Not an eframe viewport: eframe stops running frames while the panel is hidden
//! (F8), and the map has to keep drawing then. A layered window is one Win32 call
//! per frame (`UpdateLayeredWindow`) from a pixel buffer, and never takes focus or
//! clicks — the game keeps the mouse.
//!
//! Keys, polled like F8 and only while the game or the panel has focus, chosen in
//! the panel (F9 and F6 unless changed — F7 is the game's photo mode): one shows and
//! hides the map, the other drops a marker where the hero stands (or removes the one
//! it stands beside). The map only reads the worker's snapshot — it never
//! touches the game's memory.

use super::hotkey::{game_window, pid_of};
use super::Shared;
use crate::minimap::{MapState, View};
use crate::raster::{draw_map, Canvas};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F1};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow, PeekMessageW,
    RegisterClassW, SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindow, HWND_TOPMOST, MSG, PM_REMOVE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// The window is this many pixels square.
const SIZE_PX: i32 = 240;
/// Gap from the game window's top-right corner.
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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// A 32-bit top-down DIB the canvas is copied into each frame.
struct Surface {
    screen: HDC,
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u32,
}

impl Surface {
    fn new() -> Option<Surface> {
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIZE_PX,
                biHeight: -SIZE_PX,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(dc);
                ReleaseDC(std::ptr::null_mut(), screen);
                return None;
            }
            SelectObject(dc, bitmap);
            Some(Surface { screen, dc, bitmap, bits: bits.cast() })
        }
    }

    fn present(&self, hwnd: HWND, cv: &Canvas, x: i32, y: i32) {
        unsafe {
            std::ptr::copy_nonoverlapping(cv.px.as_ptr(), self.bits, cv.px.len());
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let (pos, size, src) = (POINT { x, y }, SIZE { cx: SIZE_PX, cy: SIZE_PX }, POINT { x: 0, y: 0 });
            UpdateLayeredWindow(hwnd, self.screen, &pos, &size, self.dc, &src, 0, &blend, ULW_ALPHA);
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
            ReleaseDC(std::ptr::null_mut(), self.screen);
        }
    }
}

fn create_window() -> HWND {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = wide("hiumod-minimap");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let title = wide("Hell Is Us Minimap");
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            SIZE_PX,
            SIZE_PX,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    }
}

fn pump() {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn pressed(key: u16, was: &mut bool) -> bool {
    let down = unsafe { GetAsyncKeyState(key as i32) } as u16 & 0x8000 != 0;
    let edge = down && !*was;
    *was = down;
    edge
}

pub fn run(shared: Arc<Shared>) {
    let hwnd = create_window();
    let Some(surface) = (!hwnd.is_null()).then(Surface::new).flatten() else {
        crate::journal::line("minimap: could not create its window");
        return;
    };
    let mut cv = Canvas::new(SIZE_PX as usize, SIZE_PX as usize);
    let (mut marker_was, mut toggle_was) = (false, false);
    let mut shown = false;
    let mut saved = Instant::now();
    let mut tick = 0u32;
    while !shared.quit.load(Ordering::SeqCst) {
        pump();
        std::thread::sleep(FRAME);
        tick = tick.wrapping_add(1);

        let game = shared.game_pid.load(Ordering::SeqCst);
        let focus = pid_of(unsafe { GetForegroundWindow() });
        let focused = game != 0 && (focus == game || focus == std::process::id());
        let (pose, world) = match shared.snap.lock().unwrap().as_ref() {
            Some(s) => (s.pose, s.world.clone()),
            None => (None, None),
        };
        let here = pose.map(|(p, yaw)| ([p[0] as f32, p[1] as f32, p[2] as f32], yaw as f32));

        let mut state = shared.map.lock().unwrap();
        let fkey = |n: u8| VK_F1 + n as u16 - 1;
        let (toggle_now, marker_now) =
            (pressed(fkey(state.toggle_key), &mut toggle_was), pressed(fkey(state.marker_key), &mut marker_was));
        if focused && toggle_now {
            state.show = !state.show;
            state.dirty = true;
        }
        if let (Some((p, yaw)), Some(world)) = (here, world.as_deref()) {
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
            let window = game_window(game).filter(|_| focused && state.show);
            if let Some((_, r)) = window {
                let view = View {
                    center: p,
                    yaw_deg: yaw,
                    heading_up: state.heading_up,
                    scale: (SIZE_PX as f32 / 2.0 - 14.0) / (state.radius_m * 100.0),
                };
                draw_map(&mut cv, &state, world, &view);
                surface.present(hwnd, &cv, r.right - SIZE_PX - MARGIN, r.top + MARGIN + 24);
                if !shown {
                    unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
                    shown = true;
                }
                // A borderless game re-asserts its z-order on focus; stay above it.
                if tick % 20 == 0 {
                    unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
                }
            }
            if window.is_none() && shown {
                unsafe { ShowWindow(hwnd, SW_HIDE) };
                shown = false;
            }
        } else if shown {
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            shown = false;
        }
        if saved.elapsed() >= SAVE_EVERY {
            save(&mut state);
            saved = Instant::now();
        }
    }
    save(&mut shared.map.lock().unwrap());
    drop(surface);
    unsafe { DestroyWindow(hwnd) };
}
