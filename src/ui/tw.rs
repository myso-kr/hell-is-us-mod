//! The panel's layout, as Tailwind writes it: CSS Flexbox and Grid, computed by taffy
//! (through egui_taffy) from what each widget measures (.spec/PANEL.md §1).
//!
//! Hand-computed widths kept running past a card's edge — a value box wider than
//! guessed, a label wrapping to two lines. Here every widget is a node that reports
//! its own size, and the containers are CSS: `flex`, `flex-wrap`, `gap`, `grow`,
//! `basis`, `min-w-0` — and masonry columns for the cards (`masonry`): each card in
//! the column that is shortest, by the heights the frame before measured.
//!
//! Containers take a `Tui`; the widgets inside are small egui leaves. Text that may
//! need to wrap goes in a `block`, which can shrink to nothing across and grows
//! downward instead.

use eframe::egui::{self, Color32, RichText};
use egui_taffy::taffy::prelude::{auto, fr, length, percent};
use egui_taffy::taffy::{self, AlignItems, Display, FlexDirection, FlexWrap, Size, Style};
use egui_taffy::{Tui, TuiBuilderLogic, TuiContainerResponse};

/// `gap-3`: the space between cards and between a card's rows (px).
pub const GAP: f32 = super::theme::BLOCK;
/// A card's width when the window is sized to its page (px).
pub const CARD: f32 = 372.0;

/// `w-full`: a root as wide as the room it is given. A root left `auto` is laid out
/// as CSS does — fit to its content — and its cards fold to one narrow column.
pub fn full(s: Style) -> Style {
    Style { size: Size { width: percent(1.0_f32), height: auto() }, ..s }
}

/// How wide a page of `cards` side-by-side cards is (px).
pub fn cards_width(cards: u32) -> f32 {
    cards as f32 * CARD + cards.saturating_sub(1) as f32 * GAP
}
/// A form row's label: a third of the row, between these (px).
const LABEL_MIN: f32 = 72.0;
const LABEL_MAX: f32 = 200.0;
/// The slider's track never gets narrower than this (px).
const TRACK_MIN: f32 = 48.0;

fn gap(px: f32) -> Size<taffy::LengthPercentage> {
    Size { width: length(px), height: length(px) }
}

/// `grid grid-cols-[minmax(0,1fr)] gap-{px}`: a column of rows, each as wide as the
/// column and as tall as its content.
///
/// A grid, not `flex-col`: a flex column hands its spare height to every child that
/// grows — and `block`s grow, so they fill what is left of a row — which showed as
/// uneven gaps between a card's rows. Grid rows are sized by their content alone.
pub fn col(px: f32) -> Style {
    Style {
        display: Display::Grid,
        grid_template_columns: vec![taffy::style_helpers::minmax(length(0.0_f32), fr(1.0_f32))],
        align_items: Some(AlignItems::Start),
        justify_items: Some(AlignItems::Stretch),
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
        flex_basis: length(0.0_f32),
        min_size: Size { width: length(0.0_f32), height: auto() },
        ..s
    }
}

/// Masonry: `n` cards in `columns` columns, each card put in the column that is
/// shortest so far — by the heights the cards had the frame before (kept in egui's
/// memory under `key`), so the columns come out about even however tall each card
/// grows. `card(tui, i)` draws card `i`. A card not measured yet counts as `GUESS` tall.
pub fn masonry(tui: &mut Tui, key: &str, columns: usize, n: usize, mut card: impl FnMut(&mut Tui, usize)) {
    const GUESS: f32 = 240.0;
    let columns = columns.clamp(1, n.max(1));
    let id = |i: usize| egui::Id::new(("masonry", key, i));
    let heights: Vec<f32> = {
        let ctx = tui.egui_ctx();
        (0..n).map(|i| ctx.data(|d| d.get_temp::<f32>(id(i))).unwrap_or(GUESS)).collect()
    };
    let lanes = place(&heights, columns);
    tui.style(Style { align_items: Some(AlignItems::Start), ..full(row(GAP)) }).add(|tui| {
        for lane in lanes {
            tui.style(grow(col(GAP))).add(|tui| {
                for i in lane {
                    tui.style(col(0.0)).add_with_background_ui(
                        |ui, container| {
                            let h = container.full_container().height();
                            ui.ctx().data_mut(|d| d.insert_temp(id(i), h));
                        },
                        |tui, _| card(tui, i),
                    );
                }
            });
        }
    });
}

/// Which card goes in which column: in order, each to the shortest column so far.
pub fn place(heights: &[f32], columns: usize) -> Vec<Vec<usize>> {
    let columns = columns.max(1);
    let mut lanes = vec![Vec::new(); columns];
    let mut tall = vec![0.0f32; columns];
    for (i, h) in heights.iter().enumerate() {
        let c = (0..columns).min_by(|&a, &b| tall[a].total_cmp(&tall[b])).unwrap();
        lanes[c].push(i);
        tall[c] += h + GAP;
    }
    lanes
}

/// `grid grid-cols-[{side}px_1fr] gap-{gap}`: a sidebar and the rest.
pub fn sidebar(side: f32, px: f32) -> Style {
    Style {
        display: Display::Grid,
        grid_template_columns: vec![length(side), fr(1.0_f32)],
        align_items: Some(AlignItems::Start),
        gap: gap(px),
        ..Default::default()
    }
}

/// A leaf that fills its row across and can shrink to nothing: what it holds is laid
/// out in the width it is given, and its height follows (wrapped text, lists).
pub fn block<T>(tui: &mut Tui, f: impl FnOnce(&mut egui::Ui) -> T) -> T {
    // `grow min-w-0`: in a row it takes what is left, so a width it was measured
    // narrower at in the frame before cannot hold it there; in a column (a grid, see
    // `col`) it is stretched across and grow means nothing.
    tui.style(Style {
        min_size: Size { width: length(0.0_f32), height: auto() },
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

/// One entry of a list that runs over several lines — its head and the notes under
/// it held close (`gap-0.5`), so the card's own gap falls between entries, not inside one.
pub fn item<T>(tui: &mut Tui, body: impl FnOnce(&mut Tui) -> T) -> T {
    tui.style(col(2.0)).add(body)
}

/// One choice of a list: a selectable row that wraps to the width it is given.
pub fn pick(tui: &mut Tui, on: bool, text: impl Into<RichText>) -> bool {
    let text = text.into();
    block(tui, |ui| ui.add(egui::Button::selectable(on, text).wrap_mode(egui::TextWrapMode::Wrap)).clicked())
}

/// `pick` with an icon before it (ui/svg.rs draws one).
pub fn pick_with(tui: &mut Tui, on: bool, icon: impl FnOnce(&mut egui::Ui), text: impl Into<RichText>) -> bool {
    let text = text.into();
    tui.style(row(super::theme::TIGHT)).add(|tui| {
        w(tui, icon);
        pick(tui, on, text)
    })
}

/// A small, dim note that wraps to the width it is given.
pub fn note(tui: &mut Tui, text: impl Into<String>) {
    let text = text.into();
    block(tui, |ui| {
        ui.add(egui::Label::new(RichText::new(text).color(super::theme::DIM).small()).wrap());
    });
}

/// Wrapping text.
pub fn text(tui: &mut Tui, text: impl Into<RichText>) {
    let text = text.into();
    block(tui, |ui| {
        ui.add(egui::Label::new(text).wrap());
    });
}

/// A titled card: `flex flex-col gap-2 p-4 border rounded`, the accent bar beside its title.
pub fn card<T>(tui: &mut Tui, title: &str, body: impl FnOnce(&mut Tui) -> T) -> T {
    // A raised panel with rounded corners and an accent bar down its left edge.
    fn background(ui: &mut egui::Ui, container: &egui_taffy::TaffyContainerUi) {
        let rect = container.full_container();
        let p = ui.painter();
        p.rect(
            rect,
            super::theme::R_CARD,
            super::theme::CARD,
            egui::Stroke::new(1.0, super::theme::EDGE),
            egui::StrokeKind::Inside,
        );
        let bar = egui::Rect::from_min_size(rect.min + egui::vec2(1.0, super::theme::PAD - 1.0), egui::vec2(3.0, 18.0));
        p.rect_filled(bar, 1.5, ACCENT);
    }
    tui.style(Style { padding: length(super::theme::PAD), ..col(super::theme::INLINE) })
        .add_with_background_ui(background, |tui, _| {
            w(tui, |ui| ui.label(RichText::new(title).strong().size(13.5).color(super::theme::TITLE)));
            body(tui)
        })
        .main
}

/// The cards' accent bar: the theme's accent.
pub const ACCENT: Color32 = super::theme::ACCENT;

/// A form row: the label in 42 % of the row (72–200 px, wrapping), then the
/// controls in what is left, wrapping onto a second line rather than overflowing.
pub fn field<T>(tui: &mut Tui, label: impl Into<RichText>, body: impl FnOnce(&mut Tui) -> T) -> T {
    let label = label.into();
    tui.style(Style { align_items: Some(AlignItems::Center), ..row(super::theme::BLOCK) }).add(|tui| {
        tui.style(Style {
            flex_basis: percent(0.42_f32),
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
        tui.style(grow(wrap(super::theme::INLINE))).add(body)
    })
}

/// A switch with its description beside it, the description wrapping.
pub fn switch(tui: &mut Tui, on: &mut bool, label: impl Into<RichText>) -> egui::Response {
    let label = label.into();
    tui.style(Style { align_items: Some(AlignItems::Start), ..row(super::theme::INLINE) }).add(|tui| {
        let r = w(tui, |ui| super::panel::toggle(ui, on));
        text(tui, label);
        r
    })
}

/// Choices that wrap onto the next line.
pub fn choices<T>(tui: &mut Tui, body: impl FnOnce(&mut Tui) -> T) -> T {
    tui.style(wrap(super::theme::INLINE)).add(body)
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
    tui.style(grow(row(super::theme::INLINE))).add(|tui| {
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
    fn pages_are_as_wide_as_their_cards() {
        assert_eq!(cards_width(1), CARD);
        assert_eq!(cards_width(2), 2.0 * CARD + GAP);
    }

    #[test]
    fn masonry_puts_each_card_in_the_shortest_column() {
        // Tall first card: the next two go beside it, one under the other.
        assert_eq!(place(&[500.0, 100.0, 100.0, 100.0], 2), vec![vec![0], vec![1, 2, 3]]);
        assert_eq!(place(&[100.0, 100.0, 100.0], 3), vec![vec![0], vec![1], vec![2]]);
        assert_eq!(place(&[100.0; 4], 1), vec![vec![0, 1, 2, 3]]);
    }

    #[test]
    fn grow_gives_back_its_width() {
        let s = grow(row(6.0));
        assert_eq!(s.flex_grow, 1.0);
        assert_eq!(s.min_size.width, length(0.0_f32));
        assert_eq!(s.flex_direction, FlexDirection::Row);
    }
}
