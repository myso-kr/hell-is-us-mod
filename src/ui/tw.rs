//! The panel's layout, as Tailwind writes it: CSS Flexbox and Grid, computed by taffy
//! (through egui_taffy) from what each widget measures (.spec/GUIDE.md §15).
//!
//! Hand-computed widths kept running past a card's edge — a value box wider than
//! guessed, a label wrapping to two lines. Here every widget is a node that reports
//! its own size, and the containers are CSS: `flex`, `flex-wrap`, `gap`, `grow`,
//! `basis`, `min-w-0`, and `grid` with `repeat(auto-fit, minmax(…, 1fr))` for cards
//! that sit side by side when there is room and stack when there is not.
//!
//! Containers take a `Tui`; the widgets inside are small egui leaves. Text that may
//! need to wrap goes in a `block`, which can shrink to nothing across and grows
//! downward instead.

use eframe::egui::{self, Color32, RichText};
use egui_taffy::taffy::prelude::{auto, fr, length, minmax, percent, repeat};
use egui_taffy::taffy::{self, AlignItems, Display, FlexDirection, FlexWrap, Size, Style, TrackSizingFunction};
use egui_taffy::{Tui, TuiBuilderLogic, TuiContainerResponse};

/// `gap-3`: the space between cards and between a card's rows (px).
pub const GAP: f32 = 12.0;
/// A card is at least this wide before the grid stacks the cards (px).
pub const CARD_MIN: f32 = 320.0;
/// A card's width when the window is sized to its page (px).
pub const CARD: f32 = 360.0;

/// `w-full`: a root as wide as the room it is given. A root left `auto` is laid out
/// as CSS does — fit to its content — and its cards fold to one narrow column.
pub fn full(s: Style) -> Style {
    Style { size: Size { width: percent(1.0), height: auto() }, ..s }
}

/// How wide a page of `cards` side-by-side cards is (px).
pub fn cards_width(cards: u32) -> f32 {
    cards as f32 * CARD + cards.saturating_sub(1) as f32 * GAP
}
/// A form row's label: a third of the row, between these (px).
const LABEL_MIN: f32 = 72.0;
const LABEL_MAX: f32 = 150.0;
/// The slider's track never gets narrower than this (px).
const TRACK_MIN: f32 = 48.0;

fn gap(px: f32) -> Size<taffy::LengthPercentage> {
    Size { width: length(px), height: length(px) }
}

/// `flex flex-col gap-{px} items-stretch`
pub fn col(px: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: Some(AlignItems::Stretch),
        gap: gap(px),
        ..Default::default()
    }
}

/// `flex flex-row gap-{px} items-center`
pub fn row(px: f32) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::Center),
        gap: gap(px),
        ..Default::default()
    }
}

/// `flex flex-row flex-wrap gap-{px} items-center`
pub fn wrap(px: f32) -> Style {
    Style { flex_wrap: FlexWrap::Wrap, ..row(px) }
}

/// `grow basis-0 min-w-0`: take what is left of the row, and give it back.
pub fn grow(s: Style) -> Style {
    Style {
        flex_grow: 1.0,
        flex_shrink: 1.0,
        flex_basis: length(0.0),
        min_size: Size { width: length(0.0), height: auto() },
        ..s
    }
}

/// `grid grid-cols-[repeat(auto-fit,minmax({min}px,1fr))] gap-3 items-start`: cards
/// side by side while each can be `min` wide, stacked when not.
pub fn cards(min: f32) -> Style {
    let track: TrackSizingFunction = minmax(length(min), fr(1.0));
    Style {
        display: Display::Grid,
        grid_template_columns: vec![repeat("auto-fit", vec![track])],
        align_items: Some(AlignItems::Start),
        gap: gap(GAP),
        ..Default::default()
    }
}

/// `grid grid-cols-[{side}px_1fr] gap-{gap}`: a sidebar and the rest.
pub fn sidebar(side: f32, px: f32) -> Style {
    Style {
        display: Display::Grid,
        grid_template_columns: vec![length(side), fr(1.0)],
        align_items: Some(AlignItems::Start),
        gap: gap(px),
        ..Default::default()
    }
}

/// A leaf that fills its row across and can shrink to nothing: what it holds is laid
/// out in the width it is given, and its height follows (wrapped text, lists).
pub fn block<T>(tui: &mut Tui, f: impl FnOnce(&mut egui::Ui) -> T) -> T {
    // `grow min-w-0`: stretched in a column; in a row it takes what is left, so a width
    // it was measured narrower at in the frame before cannot hold it there.
    tui.style(Style {
        min_size: Size { width: length(0.0), height: auto() },
        flex_grow: 1.0,
        flex_shrink: 1.0,
        ..Default::default()
    })
    .ui_manual(|ui, _| {
        let inner = f(ui);
        let used = ui.min_size();
        TuiContainerResponse {
            inner,
            min_size: egui::vec2(0.0, used.y),
            intrinsic_size: None,
            max_size: used,
            infinite: egui::Vec2b::FALSE,
        }
    })
}

/// A leaf exactly as big as the widget it holds.
///
/// Measured unwrapped (`shrink-0 whitespace-nowrap`): a button or a short label is
/// as wide as its text, never folded to the width a first frame happened to give it.
pub fn w<T>(tui: &mut Tui, f: impl FnOnce(&mut egui::Ui) -> T) -> T {
    tui.style(Style { flex_shrink: 0.0, ..Default::default() }).wrap_mode(egui::TextWrapMode::Extend).ui(f)
}

/// One choice of a list: a selectable row that wraps to the width it is given.
pub fn pick(tui: &mut Tui, on: bool, text: impl Into<RichText>) -> bool {
    let text = text.into();
    block(tui, |ui| ui.add(egui::Button::selectable(on, text).wrap_mode(egui::TextWrapMode::Wrap)).clicked())
}

/// A small, dim note that wraps to the width it is given.
pub fn note(tui: &mut Tui, text: impl Into<String>) {
    let text = text.into();
    block(tui, |ui| {
        ui.add(egui::Label::new(RichText::new(text).color(Color32::from_gray(150)).small()).wrap());
    });
}

/// Wrapping text.
pub fn text(tui: &mut Tui, text: impl Into<RichText>) {
    let text = text.into();
    block(tui, |ui| {
        ui.add(egui::Label::new(text).wrap());
    });
}

/// A titled card: `flex flex-col gap-1.5 p-2.5 border rounded`.
pub fn card<T>(tui: &mut Tui, title: &str, body: impl FnOnce(&mut Tui) -> T) -> T {
    tui.style(Style {
        padding: taffy::Rect { left: length(10.0), right: length(10.0), top: length(8.0), bottom: length(10.0) },
        ..col(6.0)
    })
    .add_with_border(|tui| {
        w(tui, |ui| ui.label(RichText::new(title).strong().size(15.0)));
        body(tui)
    })
}

/// A form row: the label in a third of the row (72–150 px, wrapping), then the
/// controls in what is left, wrapping onto a second line rather than overflowing.
pub fn field<T>(tui: &mut Tui, label: impl Into<RichText>, body: impl FnOnce(&mut Tui) -> T) -> T {
    let label = label.into();
    tui.style(Style { align_items: Some(AlignItems::Center), ..row(10.0) }).add(|tui| {
        tui.style(Style {
            flex_basis: percent(1.0 / 3.0),
            flex_shrink: 0.0,
            min_size: Size { width: length(LABEL_MIN), height: auto() },
            max_size: Size { width: length(LABEL_MAX), height: auto() },
            ..Default::default()
        })
        .ui_manual(|ui, _| {
            ui.add(egui::Label::new(label).wrap());
            let used = ui.min_size();
            TuiContainerResponse {
                inner: (),
                min_size: egui::vec2(LABEL_MIN, used.y),
                intrinsic_size: None,
                max_size: used,
                infinite: egui::Vec2b::FALSE,
            }
        });
        tui.style(grow(wrap(6.0))).add(body)
    })
}

/// A switch with its description beside it, the description wrapping.
pub fn switch(tui: &mut Tui, on: &mut bool, label: impl Into<RichText>) -> egui::Response {
    let label = label.into();
    tui.style(Style { align_items: Some(AlignItems::Start), ..row(8.0) }).add(|tui| {
        let r = w(tui, |ui| super::panel::toggle(ui, on));
        text(tui, label);
        r
    })
}

/// Choices that wrap onto the next line.
pub fn choices<T>(tui: &mut Tui, body: impl FnOnce(&mut Tui) -> T) -> T {
    tui.style(wrap(6.0)).add(body)
}

/// A slider whose track takes what is left of the row, and whose value box is its own
/// node — measured, never guessed.
pub fn slider<N: egui::emath::Numeric>(
    tui: &mut Tui,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    step: f64,
    suffix: &str,
) -> egui::Response {
    tui.style(grow(row(6.0))).add(|tui| {
        let track = tui.style(grow(Style::default())).ui_manual(|ui, _| {
            ui.spacing_mut().slider_width = ui.available_width().max(TRACK_MIN);
            let mut s = egui::Slider::new(value, range.clone()).show_value(false);
            if step > 0.0 {
                s = s.step_by(step);
            }
            let r = ui.add(s);
            let h = ui.min_size().y;
            TuiContainerResponse {
                inner: r,
                min_size: egui::vec2(TRACK_MIN, h),
                intrinsic_size: None,
                max_size: egui::vec2(TRACK_MIN, h),
                infinite: egui::Vec2b::FALSE,
            }
        });
        let speed = if step > 0.0 { step } else { 0.01 };
        let boxed = w(tui, |ui| {
            ui.add(egui::DragValue::new(value).range(range.clone()).speed(speed).suffix(suffix.to_string()))
        });
        track | boxed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_fit_as_many_as_their_minimum_allows() {
        let s = cards(CARD_MIN);
        assert_eq!(s.display, Display::Grid);
        assert_eq!(s.grid_template_columns.len(), 1, "one auto-fit repetition");
    }

    #[test]
    fn pages_are_as_wide_as_their_cards() {
        assert_eq!(cards_width(1), CARD);
        assert_eq!(cards_width(2), 2.0 * CARD + GAP);
    }

    #[test]
    fn grow_gives_back_its_width() {
        let s = grow(row(6.0));
        assert_eq!(s.flex_grow, 1.0);
        assert_eq!(s.min_size.width, length(0.0));
        assert_eq!(s.flex_direction, FlexDirection::Row);
    }
}
