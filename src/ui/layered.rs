//! A click-through, always-on-top window shown from a pixel buffer — the minimap and
//! the compass are each one. Never takes focus or clicks; the game keeps the mouse.
//!
//! Two ways to show the pixels: UpdateLayeredWindow (each present copies the bitmap through
//! the compositor — fine for small windows), or for the large ones (`new_composed`: the big
//! map, the game view's layer) a DirectComposition swap chain (composed.rs), falling back to
//! the first where it cannot be made.

use crate::raster::Canvas;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW, RegisterClassW, SetWindowPos,
    ShowWindow, TranslateMessage, UpdateLayeredWindow, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// Streamer mode: every overlay window kept out of screen capture (recording, streaming) —
/// the player sees them, the viewers see the game. Set by the overlay from the settings.
pub static HIDE_FROM_CAPTURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A window without a redirection bitmap: its content is a composition (composed.rs).
const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

pub struct Layered {
    hwnd: HWND,
    screen: HDC,
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u32,
    pub w: i32,
    pub h: i32,
    shown: bool,
    /// Where and how faded it was last presented: with the same pixels (the DIB still holds
    /// them), presenting again is skipped.
    last: Option<(i32, i32, u8)>,
    /// The swap chain its pixels go through, for a composed window.
    composed: Option<crate::ui::composed::Composed>,
    /// Whether it is kept out of capture now (`HIDE_FROM_CAPTURE`).
    hidden_from_capture: bool,
}

impl Layered {
    pub fn new(class: &str, title: &str, w: i32, h: i32) -> Option<Layered> {
        Layered::make(class, title, w, h, false)
    }

    /// A large window shown through DirectComposition; an ordinary one where that cannot be
    /// made (no Direct3D 11, an old Windows).
    pub fn new_composed(class: &str, title: &str, w: i32, h: i32) -> Option<Layered> {
        Layered::make(class, title, w, h, true).or_else(|| Layered::new(class, title, w, h))
    }

    fn make(class: &str, title: &str, w: i32, h: i32, composed: bool) -> Option<Layered> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = wide(class);
            let wc = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&wc);
            let title = wide(title);
            let style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
            let hwnd = CreateWindowExW(
                if composed { style | WS_EX_NOREDIRECTIONBITMAP } else { style },
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                w,
                h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return None;
            }
            let comp = if composed {
                // Layered and transparent for clicks to go through; opaque as a layer, the
                // composition carries the alpha.
                windows_sys::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
                    hwnd,
                    0,
                    255,
                    windows_sys::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
                );
                match crate::ui::composed::Composed::new(hwnd, w, h) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        crate::logfile::line(&format!("composition unavailable ({e}): layered window instead"));
                        DestroyWindow(hwnd);
                        return None;
                    }
                }
            } else {
                None
            };
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
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
                DestroyWindow(hwnd);
                return None;
            }
            SelectObject(dc, bitmap);
            Some(Layered {
                hwnd,
                screen,
                dc,
                bitmap,
                bits: bits.cast(),
                w,
                h,
                shown: false,
                last: None,
                composed: comp,
                hidden_from_capture: false,
            })
        }
    }

    /// Show `cv` (exactly w × h) with its top-left at (x, y) on the screen.
    pub fn present(&mut self, cv: &Canvas, x: i32, y: i32) {
        self.present_alpha(cv, x, y, 255);
    }

    /// As `present`, the whole window faded to `alpha` (0–255) as it is composed —
    /// free: Windows applies it, nothing is redrawn.
    pub fn present_alpha(&mut self, cv: &Canvas, x: i32, y: i32, alpha: u8) {
        debug_assert_eq!(cv.px.len(), (self.w * self.h) as usize);
        let hide = HIDE_FROM_CAPTURE.load(std::sync::atomic::Ordering::Relaxed);
        if hide != self.hidden_from_capture {
            // WDA_EXCLUDEFROMCAPTURE (Windows 10 2004 on); WDA_NONE back
            // SAFETY: the window is this thread's.
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SetWindowDisplayAffinity(
                    self.hwnd,
                    if hide { 0x11 } else { 0 },
                );
            }
            self.hidden_from_capture = hide;
        }
        // Nothing changed: the same pixels, at the same place, as faded. Comparing is a
        // memcmp; presenting is a copy through the compositor (0.7 ms a window, measured).
        // SAFETY: `bits` is the DIB section of w × h pixels made with the window.
        let same = self.shown
            && self.last == Some((x, y, alpha))
            && unsafe { std::slice::from_raw_parts(self.bits, cv.px.len()) } == &cv.px[..];
        if same {
            return;
        }
        let moved = self.last.is_none_or(|(lx, ly, _)| (lx, ly) != (x, y));
        self.last = Some((x, y, alpha));
        if let Some(c) = self.composed.as_mut() {
            // SAFETY: the DIB (kept as the last frame, for the comparison above) holds w × h
            // pixels; the window is this thread's.
            unsafe {
                std::ptr::copy_nonoverlapping(cv.px.as_ptr(), self.bits, cv.px.len());
                if moved {
                    SetWindowPos(
                        self.hwnd,
                        std::ptr::null_mut(),
                        x,
                        y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOACTIVATE | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
                    );
                }
                if let Err(e) = c.present(std::slice::from_raw_parts(self.bits, cv.px.len()), alpha) {
                    crate::logfile::line(&format!("composition present failed: {e}"));
                }
                if !self.shown {
                    ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                    self.shown = true;
                }
            }
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(cv.px.as_ptr(), self.bits, cv.px.len());
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: alpha,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let (pos, size, src) = (POINT { x, y }, SIZE { cx: self.w, cy: self.h }, POINT { x: 0, y: 0 });
            UpdateLayeredWindow(self.hwnd, self.screen, &pos, &size, self.dc, &src, 0, &blend, ULW_ALPHA);
            if !self.shown {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.shown = true;
            }
        }
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.shown = false;
        }
    }

    /// A borderless game re-asserts its z-order when it takes focus; stay above it —
    /// but under `below` when given (the panel, while it shows). Both are topmost
    /// windows: each putting itself first in turn swapped their order every second,
    /// and the panel flashed as it was uncovered and redrawn.
    pub fn keep_on_top(&self, below: Option<HWND>) {
        if self.shown {
            let after = below.filter(|h| !h.is_null()).unwrap_or(HWND_TOPMOST);
            unsafe { SetWindowPos(self.hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
        }
    }
}

impl Drop for Layered {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
            ReleaseDC(std::ptr::null_mut(), self.screen);
            DestroyWindow(self.hwnd);
        }
    }
}

/// Handle the thread's window messages.
pub fn pump() {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test --release layered -- --ignored --nocapture`: what showing a frame costs
    /// (the copy into the window's bitmap, and Windows taking it), by the window's size;
    /// shown off the screen.
    #[test]
    #[ignore]
    fn present_cost() {
        for (w, h) in [(2560, 1440), (1440, 1440), (1920, 1080), (1080, 1080)] {
            for composed in [false, true] {
                let mut win = if composed {
                    Layered::new_composed("hiumod-bench2", "bench", w, h).unwrap()
                } else {
                    Layered::new("hiumod-bench", "bench", w, h).unwrap()
                };
                let cv = Canvas { w: w as usize, h: h as usize, px: vec![0x8000_0000; (w * h) as usize] };
                win.present(&cv, -30000, -30000);
                let started = std::time::Instant::now();
                for _ in 0..60 {
                    win.present(&cv, -30000, -30000);
                }
                println!(
                    "{w}x{h} composed={composed}: {:.2} ms a frame",
                    started.elapsed().as_secs_f64() * 1000.0 / 60.0
                );
            }
        }
    }
}
