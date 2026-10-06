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

/// `src` (sw × sh, premultiplied BGRA) into `dst` at dw × dh, bilinear.
fn resample(src: &[u32], sw: usize, sh: usize, dst: &mut Vec<u32>, dw: usize, dh: usize) {
    dst.resize(dw * dh, 0);
    let (fx, fy) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
    let at = |x: usize, y: usize| src[y.min(sh - 1) * sw + x.min(sw - 1)];
    for y in 0..dh {
        let sy = ((y as f32 + 0.5) * fy - 0.5).max(0.0);
        let (y0, ty) = (sy as usize, sy.fract());
        for x in 0..dw {
            let sx = ((x as f32 + 0.5) * fx - 0.5).max(0.0);
            let (x0, tx) = (sx as usize, sx.fract());
            let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
            let mut out = 0u32;
            for shift in [0, 8, 16, 24] {
                let ch = |p: u32| ((p >> shift) & 0xFF) as f32;
                let top = ch(a) + (ch(b) - ch(a)) * tx;
                let bottom = ch(c) + (ch(d) - ch(c)) * tx;
                out |= ((top + (bottom - top) * ty).round().clamp(0.0, 255.0) as u32) << shift;
            }
            dst[y * dw + x] = out;
        }
    }
}

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
    /// The size it is drawn at (its canvas); shown at `scale` times that (`pw` × `ph`).
    pub w: i32,
    pub h: i32,
    pw: i32,
    ph: i32,
    scale: f32,
    /// The canvas last scaled, and what it scaled to: a canvas unchanged is not scaled again.
    seen: Vec<u32>,
    scaled: Vec<u32>,
    /// The scaled canvas with sharp text over it (`present_over`).
    composed_px: Vec<u32>,
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
        Layered::make(class, title, w, h, 1.0, false)
    }

    /// Drawn at w × h, shown `scale` times as large (the overlay scale): the canvas is
    /// resampled as it is presented, so what draws it needs no change.
    pub fn new_scaled(class: &str, title: &str, w: i32, h: i32, scale: f32) -> Option<Layered> {
        Layered::make(class, title, w, h, scale, false)
    }

    /// A large window shown through DirectComposition; an ordinary one where that cannot be
    /// made (no Direct3D 11, an old Windows).
    pub fn new_composed(class: &str, title: &str, w: i32, h: i32) -> Option<Layered> {
        Layered::make(class, title, w, h, 1.0, true).or_else(|| Layered::new(class, title, w, h))
    }

    fn make(class: &str, title: &str, w: i32, h: i32, scale: f32, composed: bool) -> Option<Layered> {
        let scale = if (scale - 1.0).abs() < 0.01 { 1.0 } else { scale };
        let (lw, lh) = (w, h);
        let (w, h) = ((w as f32 * scale).round().max(1.0) as i32, (h as f32 * scale).round().max(1.0) as i32);
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
                w: lw,
                h: lh,
                pw: w,
                ph: h,
                scale,
                seen: Vec::new(),
                scaled: Vec::new(),
                composed_px: Vec::new(),
                shown: false,
                last: None,
                composed: comp,
                hidden_from_capture: false,
            })
        }
    }

    /// Show `cv` (exactly w × h, the drawing size) with its top-left at (x, y) on the screen.
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
        if self.scale == 1.0 {
            self.show(&cv.px, x, y, alpha);
            return;
        }
        if self.seen != cv.px {
            self.seen.clone_from(&cv.px);
            resample(&cv.px, self.w as usize, self.h as usize, &mut self.scaled, self.pw as usize, self.ph as usize);
        }
        let px = std::mem::take(&mut self.scaled);
        self.show(&px, x, y, alpha);
        self.scaled = px;
    }

    /// As `present`, with `text` (the window's shown size, premultiplied) laid over the
    /// scaled canvas: text drawn at the shown size stays sharp (pen.rs `hi`).
    pub fn present_over(&mut self, cv: &Canvas, text: &Canvas, x: i32, y: i32) {
        if self.scale == 1.0 || text.px.len() != (self.pw * self.ph) as usize {
            self.present_alpha(cv, x, y, 255);
            return;
        }
        if self.seen != cv.px {
            self.seen.clone_from(&cv.px);
            resample(&cv.px, self.w as usize, self.h as usize, &mut self.scaled, self.pw as usize, self.ph as usize);
        }
        let mut out = std::mem::take(&mut self.composed_px);
        out.clone_from(&self.scaled);
        for (d, &s) in out.iter_mut().zip(&text.px) {
            let a = s >> 24;
            if a == 0 {
                continue;
            }
            let keep = 255 - a;
            let ch = |shift: u32| {
                let v = ((s >> shift) & 0xFF) + (((*d >> shift) & 0xFF) * keep + 127) / 255;
                v.min(255) << shift
            };
            *d = ch(24) | ch(16) | ch(8) | ch(0);
        }
        self.show(&out, x, y, 255);
        self.composed_px = out;
    }

    /// `px` (exactly the window's pw × ph) shown at (x, y), faded to `alpha`.
    fn show(&mut self, px: &[u32], x: i32, y: i32, alpha: u8) {
        // Nothing changed: the same pixels, at the same place, as faded. Comparing is a
        // memcmp; presenting is a copy through the compositor (0.7 ms a window, measured).
        // SAFETY: `bits` is the DIB section of pw × ph pixels made with the window.
        let same = self.shown
            && self.last == Some((x, y, alpha))
            && unsafe { std::slice::from_raw_parts(self.bits, px.len()) } == px;
        if same {
            return;
        }
        let moved = self.last.is_none_or(|(lx, ly, _)| (lx, ly) != (x, y));
        self.last = Some((x, y, alpha));
        if let Some(c) = self.composed.as_mut() {
            // SAFETY: the DIB (kept as the last frame, for the comparison above) holds pw × ph
            // pixels; the window is this thread's.
            unsafe {
                std::ptr::copy_nonoverlapping(px.as_ptr(), self.bits, px.len());
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
                if let Err(e) = c.present(std::slice::from_raw_parts(self.bits, px.len()), alpha) {
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
            std::ptr::copy_nonoverlapping(px.as_ptr(), self.bits, px.len());
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: alpha,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let (pos, size, src) = (POINT { x, y }, SIZE { cx: self.pw, cy: self.ph }, POINT { x: 0, y: 0 });
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

#[cfg(test)]
mod scale_tests {
    use super::*;

    #[test]
    fn a_scaled_canvas_keeps_its_colours() {
        let src = vec![0xFF10_2030; 4 * 4];
        let mut dst = Vec::new();
        resample(&src, 4, 4, &mut dst, 6, 6);
        assert_eq!(dst.len(), 36);
        assert!(dst.iter().all(|&p| p == 0xFF10_2030));
    }
}
