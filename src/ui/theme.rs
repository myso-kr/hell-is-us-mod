//! The panel's look in one place: a palette (the homepage, docs/, uses the same),
//! the corner radii, and egui's visuals set from them — so cards, the sidebar, the
//! chips and every egui widget agree (.spec/PANEL.md §3).
//!
//! Hell Is Us is grey and foggy, its UI cold and sparse; the mod keeps that — a dark
//! slate ground, one accent (Lymbic blue) for what is chosen or live, and the three
//! state colours (fine, waiting, wrong) used for nothing else.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// The window behind everything.
pub const GROUND: Color32 = Color32::from_rgb(0x0E, 0x12, 0x17);
/// The page and the sidebar.
pub const SURFACE: Color32 = Color32::from_rgb(0x13, 0x18, 0x1F);
/// A card.
pub const CARD: Color32 = Color32::from_rgb(0x1A, 0x20, 0x28);
/// A card's edge, and the dividers.
pub const EDGE: Color32 = Color32::from_rgb(0x2A, 0x32, 0x3D);
/// A control at rest, and a field's well.
pub const CONTROL: Color32 = Color32::from_rgb(0x22, 0x2A, 0x34);
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x2C, 0x36, 0x43);
/// Text, and text that steps back.
pub const TEXT: Color32 = Color32::from_rgb(0xD9, 0xE1, 0xEA);
pub const DIM: Color32 = Color32::from_rgb(0x8B, 0x96, 0xA3);
/// Titles.
pub const TITLE: Color32 = Color32::from_rgb(0xE6, 0xEE, 0xF7);
/// Chosen, live, on: Lymbic blue, and its deep tint behind a chosen item.
pub const ACCENT: Color32 = Color32::from_rgb(0x5A, 0x9C, 0xE6);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x22, 0x34, 0x4D);
/// States.
pub const OK: Color32 = Color32::from_rgb(0x8F, 0xD1, 0x7A);
pub const WAIT: Color32 = Color32::from_rgb(0xE8, 0xC0, 0x6A);
pub const BAD: Color32 = Color32::from_rgb(0xF0, 0x82, 0x78);

/// Spacing, on a 4 px scale — every gap in the panel is one of these:
/// `TIGHT` an icon and its text, `INLINE` controls in a row and rows in a card,
/// `BLOCK` cards from each other and a group from the next, `PAD` inside a card and
/// round the window.
pub const TIGHT: f32 = 4.0;
pub const INLINE: f32 = 8.0;
pub const BLOCK: f32 = 12.0;
pub const PAD: f32 = 16.0;

/// Corners: a card, a control, a chip.
pub const R_CARD: u8 = 8;
pub const R_CONTROL: u8 = 5;
pub const R_CHIP: u8 = 11;

/// egui's visuals from the palette, and the spacing and text sizes.
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(INLINE, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.window_margin = egui::Margin::same(BLOCK as i8);
        s.spacing.slider_width = 170.0;
        // Solid bars, shown whenever there is more to scroll: egui's default floating bars
        // are thin and appear only on hover, so a list cut off at the bottom looked complete.
        s.spacing.scroll = egui::style::ScrollStyle { bar_width: 6.0, ..egui::style::ScrollStyle::solid() };
        s.spacing.combo_width = 68.0;
        for (style, size) in [
            (egui::TextStyle::Body, 12.5),
            (egui::TextStyle::Button, 12.5),
            (egui::TextStyle::Small, 11.0),
            (egui::TextStyle::Monospace, 12.0),
            (egui::TextStyle::Heading, 15.0),
        ] {
            if let Some(f) = s.text_styles.get_mut(&style) {
                f.size = size;
            }
        }
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.override_text_color = Some(TEXT);
        v.panel_fill = SURFACE;
        v.window_fill = GROUND;
        v.window_stroke = Stroke::new(1.0, EDGE);
        v.window_corner_radius = CornerRadius::same(R_CARD);
        v.extreme_bg_color = GROUND;
        v.faint_bg_color = CARD;
        v.code_bg_color = CONTROL;
        v.hyperlink_color = ACCENT;
        v.warn_fg_color = WAIT;
        v.error_fg_color = BAD;
        v.selection.bg_fill = ACCENT_DEEP;
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.slider_trailing_fill = true;
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = CARD;
        w.noninteractive.weak_bg_fill = CARD;
        w.noninteractive.bg_stroke = Stroke::new(1.0, EDGE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        for (st, fill, edge) in [
            (&mut w.inactive, CONTROL, EDGE),
            (&mut w.hovered, CONTROL_HOVER, ACCENT.gamma_multiply(0.6)),
            (&mut w.active, ACCENT_DEEP, ACCENT),
            (&mut w.open, CONTROL_HOVER, ACCENT.gamma_multiply(0.6)),
        ] {
            st.bg_fill = fill;
            st.weak_bg_fill = fill;
            st.bg_stroke = Stroke::new(1.0, edge);
            st.corner_radius = CornerRadius::same(R_CONTROL);
            st.expansion = 0.0;
        }
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.fg_stroke = Stroke::new(1.0, TITLE);
        w.active.fg_stroke = Stroke::new(1.0, TITLE);
        w.noninteractive.corner_radius = CornerRadius::same(R_CONTROL);
    });
}
