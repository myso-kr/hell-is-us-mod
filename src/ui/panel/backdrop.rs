//! The panel's backdrop: faint contour lines drifting over two hills and, every few
//! seconds, a route finding its way across them with the goal's diamond at its head,
//! after the introduction video (tools/video/scene.html). Low in contrast, it shows
//! between the cards and behind the sidebar; everything is a function of the time.

use super::super::theme::ACCENT;
use eframe::egui::{self, pos2, Color32, Pos2, Rect, Shape, Stroke};

/// How long one route takes, drawing and fading (s).
const CYCLE: f32 = 14.0;
/// Frames a second while it moves: enough for a slow drift.
pub const FPS: f32 = 15.0;

pub fn draw(p: &egui::Painter, rect: Rect, t: f32) {
    contours(p, rect, t);
    route(p, rect, t);
}

/// Wobbly rings round two hills, drifting slowly (the splash's, fainter).
fn contours(p: &egui::Painter, rect: Rect, t: f32) {
    let hills = [(0.78, 0.30, 0.0), (0.22, 0.82, 2.1)];
    for (hx, hy, seed) in hills {
        let centre = pos2(rect.left() + rect.width() * hx, rect.top() + rect.height() * hy);
        for k in 0..9 {
            let base = 40.0 + k as f32 * 38.0;
            let phase = seed + k as f32 * 0.7 + t * 0.03;
            let points: Vec<Pos2> = (0..120)
                .map(|i| {
                    let a = i as f32 / 120.0 * std::f32::consts::TAU;
                    let r = base * (1.0 + 0.12 * (3.0 * a + phase).sin() + 0.05 * (5.0 * a - phase * 1.3).sin());
                    pos2(centre.x + r * a.cos() * 1.3, centre.y + r * a.sin())
                })
                .collect();
            let alpha = (26.0 - k as f32 * 2.0).max(8.0) as u8;
            p.add(Shape::closed_line(points, Stroke::new(1.0, Color32::from_rgba_unmultiplied(139, 150, 163, alpha))));
        }
    }
}

/// A route across the panel: it draws itself as a dashed line over 70 % of the cycle,
/// the goal's diamond glowing at its head, holds, then fades; each cycle starts from
/// another side (the cycle's number picks it).
fn route(p: &egui::Painter, rect: Rect, t: f32) {
    let n = (t / CYCLE).floor();
    let u = (t / CYCLE).fract();
    let seed = n * 1.618;
    let at = |s: f32| {
        // From one side to the other, wandering as a path between hills does.
        let x = rect.left() + rect.width() * (0.08 + 0.84 * s);
        let wave = (s * 5.0 + seed).sin() * 0.16 + (s * 11.0 + seed * 2.0).sin() * 0.05;
        let y = rect.top() + rect.height() * (0.5 + 0.3 * (seed).sin() * (1.0 - 2.0 * s) + wave);
        if (n as i64).rem_euclid(2) == 0 {
            pos2(x, y)
        } else {
            pos2(rect.right() - (x - rect.left()), y)
        }
    };
    let drawn = (u / 0.7).min(1.0);
    let fade = if u > 0.85 { 1.0 - (u - 0.85) / 0.15 } else { 1.0 };
    let colour = |a: f32| Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), (a * fade) as u8);
    // Dashes: every other step of the path, up to where it has got.
    let steps = 160;
    let last = (steps as f32 * drawn) as usize;
    for i in (0..last).step_by(2) {
        let (a, b) = (at(i as f32 / steps as f32), at((i + 1) as f32 / steps as f32));
        p.line_segment([a, b], Stroke::new(1.6, colour(70.0)));
    }
    let head = at(drawn);
    let pulse = 0.6 + 0.4 * (t * 2.4).sin().abs();
    p.circle_filled(head, 10.0, colour(26.0 * pulse));
    let d = 4.0;
    p.add(Shape::convex_polygon(
        vec![pos2(head.x, head.y - d), pos2(head.x + d, head.y), pos2(head.x, head.y + d), pos2(head.x - d, head.y)],
        colour(150.0),
        Stroke::NONE,
    ));
}
