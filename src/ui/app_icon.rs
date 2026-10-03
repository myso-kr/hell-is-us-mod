//! The mod's mark (assets/app-icon.svg, the same drawing as the site's favicon) as the
//! panel window's icon and the tray's: the executable carries no icon resource, so
//! without this Windows shows its generic application icon.

const SVG: &str = include_str!("../../assets/app-icon.svg");

/// The mark at `size` px, straight (not premultiplied) RGBA rows.
fn rgba(size: usize) -> Vec<u8> {
    let Ok(icon) = crate::icons::render(SVG, size) else { return vec![0; size * size * 4] };
    icon.px
        .iter()
        .flat_map(|&p| {
            let a = p >> 24;
            let un = |c: u32| (c * 255 + a / 2).checked_div(a).map_or(0, |v| v.min(255) as u8);
            [un((p >> 16) & 0xFF), un((p >> 8) & 0xFF), un(p & 0xFF), a as u8]
        })
        .collect()
}

/// For eframe's window icon.
pub fn icon_data(size: usize) -> std::sync::Arc<eframe::egui::IconData> {
    std::sync::Arc::new(eframe::egui::IconData { rgba: rgba(size), width: size as u32, height: size as u32 })
}

/// For the tray: an icon handle made from the mark's pixels (32-bit colour, its alpha the
/// mask), or `None` when Windows refuses one.
pub fn hicon(size: usize) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};
    let px = rgba(size);
    unsafe {
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let dc = GetDC(std::ptr::null_mut());
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let colour = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        ReleaseDC(std::ptr::null_mut(), dc);
        if colour.is_null() || bits.is_null() {
            return None;
        }
        // BGRA, straight alpha: what a 32-bit icon's colour bitmap holds.
        let out = std::slice::from_raw_parts_mut(bits as *mut u8, size * size * 4);
        for (o, i) in out.chunks_exact_mut(4).zip(px.chunks_exact(4)) {
            o.copy_from_slice(&[i[2], i[1], i[0], i[3]]);
        }
        // With a 32-bit colour bitmap the alpha decides; the mask only has to exist.
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, std::ptr::null());
        let icon = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: colour };
        let h = CreateIconIndirect(&icon);
        DeleteObject(colour as _);
        DeleteObject(mask as _);
        (!h.is_null()).then_some(h)
    }
}
