//! A click-through, always-on-top window shown from a pixel buffer — the minimap and
//! the compass are each one. Never takes focus or clicks; the game keeps the mouse.

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
}

impl Layered {
    pub fn new(class: &str, title: &str, w: i32, h: i32) -> Option<Layered> {
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
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
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
            Some(Layered { hwnd, screen, dc, bitmap, bits: bits.cast(), w, h, shown: false })
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

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    pub fn hide(&mut self) {
        if self.shown {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.shown = false;
        }
    }

    /// A borderless game re-asserts its z-order when it takes focus; stay above it.
    pub fn keep_on_top(&self) {
        if self.shown {
            unsafe { SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
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
