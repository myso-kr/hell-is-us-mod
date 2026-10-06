//! Text for the overlays, in any script the game shows (Korean too): GDI draws it
//! white on black into a scratch bitmap, with greyscale anti-aliasing, and each
//! pixel's brightness becomes the coverage it is blended into a `Canvas` with.
//! The raster's own stroke font only knows compass letters and digits.

use crate::raster::{Canvas, Rgba};
use std::collections::HashMap;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush, SelectObject,
    SetBkMode, SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CALCRECT, DT_EDITCONTROL, DT_END_ELLIPSIS, DT_NOPREFIX,
    DT_WORDBREAK, HBITMAP, HDC, HFONT, OUT_DEFAULT_PRECIS, TRANSPARENT,
};

const FACE: &str = "Malgun Gothic";

pub struct Pen {
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u32,
    /// The block it writes, in drawing pixels; its bitmap is `scale` times that.
    w: i32,
    h: i32,
    /// The overlay scale: above or below 1, text is laid out at the drawing size but drawn
    /// at the shown size into `hi`, so it stays sharp when the window is scaled.
    scale: f32,
    /// The text at the shown size (`scale` × the block), over the scaled card (layered.rs
    /// `present_over`); empty at scale 1, where text goes onto the canvas itself.
    pub hi: Canvas,
    /// By (pixel height, bold).
    fonts: HashMap<(i32, bool), HFONT>,
}

impl Pen {
    /// A pen that writes blocks of text up to `w` × `h` pixels.
    pub fn new(w: i32, h: i32) -> Option<Pen> {
        Pen::new_scaled(w, h, 1.0)
    }

    /// As `new`, its text drawn `scale` times as large into `hi`.
    pub fn new_scaled(w: i32, h: i32, scale: f32) -> Option<Pen> {
        let scale = if (scale - 1.0).abs() < 0.01 { 1.0 } else { scale };
        let (lw, lh) = (w, h);
        let (w, h) = ((w as f32 * scale).ceil() as i32 + 2, (h as f32 * scale).ceil() as i32 + 2);
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            if dc.is_null() {
                return None;
            }
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
                return None;
            }
            SelectObject(dc, bitmap);
            SetBkMode(dc, TRANSPARENT as _);
            SetTextColor(dc, 0x00FF_FFFF);
            let hi = if scale == 1.0 {
                Canvas::new(0, 0)
            } else {
                Canvas::new((lw as f32 * scale).round() as usize, (lh as f32 * scale).round() as usize)
            };
            let _ = (w, h);
            Some(Pen { dc, bitmap, bits: bits.cast(), w: lw, h: lh, scale, hi, fonts: HashMap::new() })
        }
    }

    fn font(&mut self, px: i32, bold: bool) -> HFONT {
        *self.fonts.entry((px, bold)).or_insert_with(|| {
            let face: Vec<u16> = FACE.encode_utf16().chain([0]).collect();
            unsafe {
                CreateFontW(
                    -px,
                    0,
                    0,
                    0,
                    if bold { 700 } else { 400 },
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as _,
                    OUT_DEFAULT_PRECIS as _,
                    CLIP_DEFAULT_PRECIS as _,
                    ANTIALIASED_QUALITY as _,
                    DEFAULT_PITCH as _,
                    face.as_ptr(),
                )
            }
        })
    }

    /// Write `text` at (x, y) on `cv`, wrapped to `width`, at most `lines` lines (the
    /// last one cut with "…"), with a dark shadow so it reads over any scene. Returns
    /// the height it took.
    #[allow(clippy::too_many_arguments)]
    pub fn write(
        &mut self,
        cv: &mut Canvas,
        x: i32,
        y: i32,
        width: i32,
        text: &str,
        px: i32,
        bold: bool,
        c: Rgba,
        lines: i32,
    ) -> i32 {
        let width = width.min(self.w - 1);
        if text.is_empty() || width <= 0 {
            return 0;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let format = DT_WORDBREAK | DT_NOPREFIX | DT_EDITCONTROL;
        // Laid out at the drawing size, so the card around it is as tall either way.
        let h = unsafe {
            SelectObject(self.dc, self.font(px, bold));
            let mut r = RECT { left: 0, top: 0, right: width, bottom: 0 };
            DrawTextW(self.dc, wide.as_ptr(), wide.len() as i32, &mut r, format | DT_CALCRECT);
            let line = (px as f32 * 1.4).ceil() as i32;
            r.bottom.min(line * lines.max(1)).min(self.h - 1)
        };
        let k = self.scale;
        let s = |v: i32| (v as f32 * k).round() as i32;
        let (bw, bh, fpx) = (s(width), s(h), s(px).max(1));
        // The bitmap's row length, in pixels.
        let stride = (self.w as f32 * k).ceil() as i32 + 2;
        let shadow = Rgba(0, 0, 0, 200);
        let off = s(1).max(1);
        unsafe {
            SelectObject(self.dc, self.font(fpx, bold));
            // Clear the block, then draw into it.
            for row in 0..bh + 1 {
                std::slice::from_raw_parts_mut(self.bits.add((row * stride) as usize), (bw + 1) as usize).fill(0);
            }
            let mut r = RECT { left: 0, top: 0, right: bw, bottom: bh };
            DrawTextW(self.dc, wide.as_ptr(), wide.len() as i32, &mut r, format | DT_END_ELLIPSIS);
            GdiFlush();
            let target: &mut Canvas = if k == 1.0 { cv } else { &mut self.hi };
            let (ox, oy) = (s(x), s(y));
            for row in 0..bh {
                let src = std::slice::from_raw_parts(self.bits.add((row * stride) as usize), bw as usize);
                for (col, &p) in src.iter().enumerate() {
                    let cover = ((p >> 16) & 0xFF).max((p >> 8) & 0xFF).max(p & 0xFF);
                    if cover == 0 {
                        continue;
                    }
                    let a = cover as f32 / 255.0;
                    let (px_, py_) = (ox + col as i32, oy + row);
                    target.blend(px_ + off, py_ + off, shadow, a);
                    target.blend(px_, py_, c, a);
                }
            }
        }
        h
    }

    /// The sharp text cleared, as the canvas it goes over is.
    pub fn clear(&mut self) {
        self.hi.clear();
    }
}

impl Drop for Pen {
    fn drop(&mut self) {
        unsafe {
            for f in self.fonts.values() {
                DeleteObject(*f);
            }
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
