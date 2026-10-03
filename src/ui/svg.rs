//! SVG in the panel: an SVG document rasterised by resvg (icons.rs `render`) into an
//! egui texture, kept by the document and size so each is drawn once.

use eframe::egui;
use std::hash::{Hash, Hasher};

/// The texture of `svg` at `px` pixels square — made once, then from the context's
/// memory.
pub fn texture(ctx: &egui::Context, svg: &str, px: u32) -> Option<egui::TextureHandle> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (svg, px).hash(&mut h);
    let id = egui::Id::new(("hiumod-svg", h.finish()));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let icon = crate::icons::render(svg, px as usize).ok()?;
    // The icon is premultiplied ARGB words; egui takes premultiplied RGBA bytes.
    let rgba: Vec<u8> = icon.px.iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]).collect();
    let image = egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], &rgba);
    let t = ctx.load_texture(format!("svg-{}", h.finish()), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

/// Show `svg` `size` points square (rasterised for the screen's scale).
pub fn image(ui: &mut egui::Ui, svg: &str, size: f32) -> egui::Response {
    let px = (size * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
    match texture(ui.ctx(), svg, px) {
        Some(t) => ui.add(egui::Image::new(&t).fit_to_exact_size(egui::vec2(size, size))),
        None => ui.label("?"),
    }
}

/// A vault symbol (map/symbols.rs) with its name on hover.
pub fn symbol(ui: &mut egui::Ui, n: u8, size: f32, colour: egui::Color32) -> egui::Response {
    let hex = format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b());
    match crate::symbols::svg(n, &hex) {
        Some(s) => image(ui, &s, size).on_hover_text(format!("{} ({n})", crate::symbols::label(n))),
        None => ui.label(n.to_string()),
    }
}
