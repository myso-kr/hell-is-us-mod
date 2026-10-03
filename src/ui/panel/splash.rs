//! The splash: shown in the panel's own window while the first reading comes in and the
//! game's data is checked, for at least `MIN` (a flash of it reads as a glitch), at most
//! `MAX` (a long survey is the sidebar's to report, not a reason to keep the panel shut).
//!
//! Drawn after the introduction video's closing card and the site's hero
//! (tools/video/scene.html, docs): the ground colour, faint contour lines drifting, the
//! compass strip with its glowing goal, the double diamond, the name and what it is.
//! Everything is a function of the time since it opened.

use super::super::theme::{ACCENT, DIM, EDGE, GROUND, TEXT, TITLE};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke};
use std::time::{Duration, Instant};

/// The primary screen's size (px): the splash is centred on it.
pub fn screen() -> (f32, f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    unsafe { (GetSystemMetrics(SM_CXSCREEN) as f32, GetSystemMetrics(SM_CYSCREEN) as f32) }
}

/// The splash window's size (points).
pub const SIZE: egui::Vec2 = vec2(560.0, 340.0);
/// Shown at least this long, and at most this long.
pub const MIN: Duration = Duration::from_millis(1800);
pub const MAX: Duration = Duration::from_secs(8);

/// One loading step: its name and colour (done, under way, or not applicable).
pub struct Step {
    pub name: String,
    pub colour: Color32,
}

/// Draw the splash over `ui`'s whole window, `since` it opened, with `steps` and how
/// far along it is (0–1).
pub fn draw(ui: &mut egui::Ui, since: Instant, steps: &[Step], progress: f32) {
    let rect = ui.max_rect();
    let t = since.elapsed().as_secs_f32();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, GROUND);
    contours(&p, rect, t);
    compass(&p, rect, t);

    let cx = rect.center().x;
    let fade = (t / 0.5).min(1.0);
    let alpha = |c: Color32| c.gamma_multiply(fade);
    mark(&p, pos2(cx, rect.top() + 104.0), 26.0, alpha(ACCENT));
    p.text(
        pos2(cx, rect.top() + 172.0),
        Align2::CENTER_CENTER,
        "hiumod",
        FontId::new(46.0, FontFamily::Name("display".into())),
        alpha(TITLE),
    );
    p.text(
        pos2(cx, rect.top() + 212.0),
        Align2::CENTER_CENTER,
        tr!("SPLASH_TAGLINE"),
        FontId::proportional(15.0),
        alpha(TEXT),
    );

    // The steps in a row, each a dot in its colour and its name.
    let font = FontId::proportional(12.0);
    let widths: Vec<f32> =
        steps.iter().map(|s| p.layout_no_wrap(s.name.clone(), font.clone(), TEXT).size().x + 14.0).collect();
    let gap = 22.0;
    let total = widths.iter().sum::<f32>() + gap * steps.len().saturating_sub(1) as f32;
    let mut x = cx - total / 2.0;
    let y = rect.bottom() - 62.0;
    for (s, w) in steps.iter().zip(&widths) {
        p.circle_filled(pos2(x + 4.0, y), 3.5, alpha(s.colour));
        p.text(pos2(x + 14.0, y), Align2::LEFT_CENTER, &s.name, font.clone(), alpha(DIM));
        x += w + gap;
    }

    // The bar along the bottom, and the site and version under it.
    let bar = Rect::from_min_size(pos2(rect.left() + 40.0, rect.bottom() - 38.0), vec2(rect.width() - 80.0, 2.0));
    p.rect_filled(bar, 1.0, EDGE);
    let done = Rect::from_min_size(bar.min, vec2(bar.width() * progress.clamp(0.0, 1.0), bar.height()));
    p.rect_filled(done, 1.0, ACCENT);
    let small = FontId::proportional(11.0);
    p.text(
        pos2(bar.left(), rect.bottom() - 20.0),
        Align2::LEFT_CENTER,
        "myso-kr.github.io/hell-is-us-mod",
        small.clone(),
        alpha(ACCENT.gamma_multiply(0.8)),
    );
    p.text(
        pos2(bar.right(), rect.bottom() - 20.0),
        Align2::RIGHT_CENTER,
        format!("v{}", env!("CARGO_PKG_VERSION")),
        small,
        alpha(DIM),
    );
}

/// The mark: a diamond in a diamond, outlined (assets/app-icon.svg, the site's mark).
fn mark(p: &egui::Painter, c: Pos2, r: f32, colour: Color32) {
    let diamond = |r: f32| vec![pos2(c.x, c.y - r), pos2(c.x + r, c.y), pos2(c.x, c.y + r), pos2(c.x - r, c.y)];
    p.add(Shape::closed_line(diamond(r), Stroke::new(2.4, colour)));
    p.add(Shape::closed_line(diamond(r * 0.5), Stroke::new(2.4, colour)));
}

/// Faint contour lines round a hill off to the right, drifting slowly: the site's and the
/// video's ground. Wobbly rings, not noise: the same every run.
fn contours(p: &egui::Painter, rect: Rect, t: f32) {
    let centre = pos2(rect.left() + rect.width() * 0.74, rect.top() + rect.height() * 0.62);
    for k in 0..11 {
        let base = 26.0 + k as f32 * 30.0;
        let phase = k as f32 * 0.7 + t * 0.06;
        let points: Vec<Pos2> = (0..96)
            .map(|i| {
                let a = i as f32 / 96.0 * std::f32::consts::TAU;
                let r = base * (1.0 + 0.13 * (3.0 * a + phase).sin() + 0.06 * (5.0 * a - phase * 1.3).sin());
                pos2(centre.x + r * a.cos() * 1.25, centre.y + r * a.sin())
            })
            .collect();
        let a = (52.0 - k as f32 * 3.0).max(18.0) as u8;
        p.add(Shape::closed_line(points, Stroke::new(1.0, Color32::from_rgba_unmultiplied(139, 150, 163, a))));
    }
    // The copy's shade, as on the site: darker toward the left and the bottom.
    let shade = |x0: f32, x1: f32, a0: u8, a1: u8| {
        let mut mesh = egui::Mesh::default();
        let r = Rect::from_x_y_ranges(x0..=x1, rect.y_range());
        let c0 = Color32::from_rgba_unmultiplied(14, 18, 23, a0);
        let c1 = Color32::from_rgba_unmultiplied(14, 18, 23, a1);
        mesh.colored_vertex(r.left_top(), c0);
        mesh.colored_vertex(r.right_top(), c1);
        mesh.colored_vertex(r.right_bottom(), c1);
        mesh.colored_vertex(r.left_bottom(), c0);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        mesh
    };
    p.add(shade(rect.left(), rect.left() + rect.width() * 0.6, 200, 0));
}

/// The compass strip across the top: ticks sliding by, fading at the ends, and the goal
/// diamond glowing at the centre.
fn compass(p: &egui::Painter, rect: Rect, t: f32) {
    let (cx, y, half) = (rect.center().x, rect.top() + 34.0, 150.0);
    // Tick k sits at k × 20 + the distance slid; every fifth is long.
    let slid = t * 26.0;
    let first = ((cx - half - slid) / 20.0).floor() as i32;
    let last = ((cx + half - slid) / 20.0).ceil() as i32;
    for k in first..=last {
        let x = k as f32 * 20.0 + slid;
        let edge = 1.0 - ((x - cx).abs() / half).powf(2.0);
        if edge <= 0.0 {
            continue;
        }
        let major = k.rem_euclid(5) == 0;
        let h = if major { 9.0 } else { 5.0 };
        let c = Color32::from_rgba_unmultiplied(139, 150, 163, (edge * if major { 170.0 } else { 100.0 }) as u8);
        p.line_segment([pos2(x, y + 6.0 - h), pos2(x, y + 6.0)], Stroke::new(1.0, c));
    }
    let pulse = 0.6 + 0.4 * (t * 2.2).sin().abs();
    p.circle_filled(pos2(cx, y - 6.0), 9.0, ACCENT.gamma_multiply(0.18 * pulse));
    let d = 4.5;
    p.add(Shape::convex_polygon(
        vec![pos2(cx, y - 6.0 - d), pos2(cx + d, y - 6.0), pos2(cx, y - 6.0 + d), pos2(cx - d, y - 6.0)],
        ACCENT,
        Stroke::NONE,
    ));
}
