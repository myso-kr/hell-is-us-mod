//! A small grid system for the panel: columns that are exactly as wide as they are
//! given and clip what they hold, and form rows whose label column is fixed, so
//! nothing in one card can spill into the next (.spec/GUIDE.md §12).
//!
//! egui's own `columns` and `Grid` size to their content: a wide slider or a long
//! label pushes past the column and is drawn over its neighbour. Here every column,
//! card and field is handed its width, and its contents fit to it — sliders shrink,
//! rows of buttons and long notes wrap, labels that still do not fit are cut short.

use eframe::egui::{self, Color32, RichText};

/// The gap between columns, and the label column of a form row (px).
pub const GAP: f32 = 12.0;
pub const LABEL: f32 = 112.0;
/// Room a slider leaves for its value box.
const SLIDER_VALUE: f32 = 70.0;

/// `n` columns of equal width with `GAP` between them, each a child of exactly that
/// width that clips what it holds. Takes the height of the tallest.
pub fn grid<R>(ui: &mut egui::Ui, n: usize, add: impl FnOnce(&mut [egui::Ui]) -> R) -> R {
    let n = n.max(1);
    let top = ui.cursor().min;
    let width = ui.available_width();
    let col = ((width - GAP * (n - 1) as f32) / n as f32).floor();
    let mut cols: Vec<egui::Ui> = (0..n)
        .map(|i| {
            let x = top.x + i as f32 * (col + GAP);
            let rect = egui::Rect::from_min_size(egui::pos2(x, top.y), egui::vec2(col, 100_000.0));
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
            child.set_clip_rect(rect.intersect(ui.clip_rect()));
            child.set_width(col);
            child
        })
        .collect();
    let out = add(&mut cols);
    let height = cols.iter().map(|c| c.min_rect().height()).fold(0.0, f32::max);
    ui.allocate_rect(egui::Rect::from_min_size(top, egui::vec2(width, height)), egui::Sense::hover());
    out
}

/// A form row: a label in the fixed label column (cut short if it is too long), then
/// the control in what is left — a slider there shrinks to fit.
pub fn field<R>(ui: &mut egui::Ui, label: impl Into<RichText>, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(egui::vec2(LABEL, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(LABEL);
            ui.add(egui::Label::new(label.into()).truncate());
        });
        let rest = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2(rest, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_max_width(rest);
            ui.spacing_mut().slider_width = (rest - SLIDER_VALUE).max(60.0);
            body(ui)
        })
        .inner
    })
    .inner
}

/// A row of choices that wraps onto the next line rather than running past the edge.
pub fn choices<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        body(ui)
    })
    .inner
}

/// A small, dim note that wraps to the width it is given.
pub fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).color(Color32::from_gray(150)).small()).wrap());
}

/// A switch with its description beside it, the description wrapping.
pub fn switch(ui: &mut egui::Ui, on: &mut bool, text: impl Into<RichText>) -> egui::Response {
    ui.horizontal_top(|ui| {
        let r = super::panel::toggle(ui, on);
        ui.add(egui::Label::new(text.into()).wrap());
        r
    })
    .inner
}
