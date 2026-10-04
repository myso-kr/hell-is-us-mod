//! The guide's target marked in the game's own view: its place projected through the game's
//! camera (`player::Camera`) onto the game window, a ring there with the distance under it.
//! The maps and the compass say which way and how far; at a wall of grooves 2 m apart they
//! cannot say which groove, and this can.

use crate::player::Camera;
use crate::raster::Canvas;

/// The marker's window: a ring and the distance under it.
pub const W: i32 = 96;
pub const H: i32 = 112;
/// Where the ring's centre is in it (the point marked).
pub const CX: i32 = W / 2;
pub const CY: i32 = 44;

/// Marked only this near (cm): farther, the compass and maps say it better, and a ring on
/// a far hillside hides behind what is in between, which the camera does not know.
pub const NEAR: f32 = 6_000.0;

const GOLD: crate::map::canvas::Rgba = crate::map::canvas::Rgba(255, 205, 90, 255);
const SHADE: crate::map::canvas::Rgba = crate::map::canvas::Rgba(0, 0, 0, 170);

/// The game window's drawn area on the screen: (left, top, width, height). The camera's
/// view fills it, not the window's frame (a windowed game has a title bar).
pub fn client(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
    let mut r: RECT = unsafe { std::mem::zeroed() };
    let mut o = POINT { x: 0, y: 0 };
    let ok = unsafe { GetClientRect(hwnd, &mut r) != 0 && ClientToScreen(hwnd, &mut o) != 0 };
    (ok && r.right > 0 && r.bottom > 0).then_some((o.x, o.y, r.right, r.bottom))
}

/// Where `p` (cm) is in a `w` × `h` view from `cam`, in pixels from the top left, and how
/// far from the camera; `None` behind it. Unreal's camera looks along its X, Y right, Z up,
/// and its field of view is the horizontal one.
pub fn project(cam: &Camera, p: [f32; 3], w: f32, h: f32) -> Option<(f32, f32, f32)> {
    let (pitch, yaw, roll) = (cam.rotation[0].to_radians(), cam.rotation[1].to_radians(), cam.rotation[2].to_radians());
    let (sp, cp, sy, cy, sr, cr) = (pitch.sin(), pitch.cos(), yaw.sin(), yaw.cos(), roll.sin(), roll.cos());
    // FRotationMatrix's axes.
    let forward = [cp * cy, cp * sy, sp];
    let right = [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp];
    let up = [-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp];
    let d = [p[0] as f64 - cam.at[0], p[1] as f64 - cam.at[1], p[2] as f64 - cam.at[2]];
    let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
    let (x, y, z) = (dot(forward), dot(right), dot(up));
    if x < 10.0 {
        return None;
    }
    let focal = (w as f64 / 2.0) / (cam.fov as f64 / 2.0).to_radians().tan();
    let sx = w as f64 / 2.0 + y / x * focal;
    let sy = h as f64 / 2.0 - z / x * focal;
    let far = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    Some((sx as f32, sy as f32, far as f32))
}

/// The marker for a target `far` cm away: a ring, smaller the farther, and the distance.
pub fn draw(cv: &mut Canvas, far: f32) {
    cv.px.fill(0);
    let r = (22.0 - far / 400.0).clamp(10.0, 22.0);
    let (cx, cy) = (CX as f32, CY as f32);
    cv.ring(cx, cy, r + 1.5, 4.5, SHADE);
    cv.ring(cx, cy, r, 2.5, GOLD);
    cv.disc(cx, cy, 2.5, GOLD);
    let m = far / 100.0;
    let label = if m < 10.0 { format!("{m:.1}m") } else { format!("{m:.0}m") };
    let ty = cy + r + 14.0;
    cv.text(cx + 1.0, ty + 1.0, 13.0, &label, SHADE);
    cv.text(cx, ty, 13.0, &label, GOLD);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(yaw: f64, pitch: f64) -> Camera {
        Camera { at: [0.0, 0.0, 0.0], rotation: [pitch, yaw, 0.0], fov: 90.0 }
    }

    #[test]
    fn straight_ahead_is_the_middle_and_behind_is_nowhere() {
        let (x, y, far) = project(&cam(0.0, 0.0), [1000.0, 0.0, 0.0], 1920.0, 1080.0).unwrap();
        assert!((x - 960.0).abs() < 0.5 && (y - 540.0).abs() < 0.5, "{x} {y}");
        assert!((far - 1000.0).abs() < 0.5);
        assert!(project(&cam(0.0, 0.0), [-1000.0, 0.0, 0.0], 1920.0, 1080.0).is_none());
    }

    #[test]
    fn right_is_right_and_up_is_up() {
        // 45° to the right at a 90° field of view: the right edge.
        let (x, _, _) = project(&cam(0.0, 0.0), [1000.0, 1000.0, 0.0], 1920.0, 1080.0).unwrap();
        assert!((x - 1920.0).abs() < 0.5, "{x}");
        let (_, y, _) = project(&cam(0.0, 0.0), [1000.0, 0.0, 200.0], 1920.0, 1080.0).unwrap();
        assert!(y < 540.0, "above the middle: {y}");
        // Facing east (yaw 90), a point to the east is ahead.
        let (x, y, _) = project(&cam(90.0, 0.0), [0.0, 1000.0, 0.0], 1920.0, 1080.0).unwrap();
        assert!((x - 960.0).abs() < 0.5 && (y - 540.0).abs() < 0.5);
        // Looking down 30°, a point ahead at eye level is above the middle.
        let (_, y, _) = project(&cam(0.0, -30.0), [1000.0, 0.0, 0.0], 1920.0, 1080.0).unwrap();
        assert!(y < 540.0, "{y}");
    }
}
