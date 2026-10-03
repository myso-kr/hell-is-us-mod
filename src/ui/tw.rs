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

/// `grid grid-cols-{n} gap-{px}`: equal columns, each as wide as the others, so tiles in
/// a row line up and none is left alone on a row of its own.
pub fn grid(n: usize, px: f32) -> Style {
    Style {
        display: Display::Grid,
        grid_template_columns: vec![taffy::style_helpers::minmax(length(0.0_f32), fr(1.0_f32)); n],
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
                    // GAP inside too: a card that draws several cards (a page's column)
                    // keeps the same space between them as between the slots.
                    tui.style(col(GAP)).add_with_background_ui(
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

/// As `block`, at least `width` across whatever it holds: the panel's page, which
/// is as wide as its columns of cards even before they are laid out (the cards are
/// laid out in the width it gets, so measured from them alone it came to nothing).
pub fn block_at_least<T>(tui: &mut Tui, width: f32, f: impl FnOnce(&mut egui::Ui) -> T) -> T {
    tui.style(Style {
        min_size: Size { width: length(width), height: auto() },
        flex_grow: 1.0,
        flex_shrink: 1.0,
        ..Default::default()
    })
    .ui_manual(|ui, _| {
        let inner = f(ui);
        let used = ui.min_size();
        TuiContainerResponse {
            inner,
            min_size: egui::vec2(width, used.y),
            intrinsic_size: None,
            max_size: egui::vec2(used.x.max(width), used.y),
            infinite: egui::Vec2b::FALSE,
        }
    })
}

/// `col-span-full`: `style` as a grid item across every column.
pub fn span_all(style: Style) -> Style {
    use taffy::prelude::line;
    Style { grid_column: taffy::Line { start: line(1), end: line(-1) }, ..style }
}

/// `flex-1 basis-0 min-w-0`: an even share of its row, whatever it holds. Its content is
/// laid out in that share and never widens it, so it cannot push the row, or the
/// window, wider from one frame to the next.
pub fn share<T>(tui: &mut Tui, f: impl FnOnce(&mut egui::Ui) -> T) -> T {
    tui.style(Style {
        min_size: Size { width: length(0.0_f32), height: auto() },
        flex_basis: length(0.0_f32),
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
            max_size: egui::vec2(0.0, used.y),
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

/// One line of a list, the panel's single pattern for "go there": an icon, the line, and
/// at most one button at its end (`end`, e.g. a reveal). `Some(on)` makes the line
/// pressable, marked while `on`; `None` draws it the same size and place, not pressable.
/// Returns whether the line was pressed.
pub fn line(
    tui: &mut Tui,
    on: Option<bool>,
    icon: impl FnOnce(&mut egui::Ui),
    text: impl Into<RichText>,
    end: impl FnOnce(&mut Tui),
) -> bool {
    let text = text.into();
    tui.style(row(super::theme::TIGHT)).add(|tui| {
        w(tui, icon);
        // Not pressable: the same selectable, disabled — the same padding and height, so
        // its text starts where a pressable line's does (a frameless button has none).
        let pressed = block(tui, |ui| {
            let button = egui::Button::selectable(on.unwrap_or(false), text).wrap_mode(egui::TextWrapMode::Wrap);
            ui.add_enabled(on.is_some(), button).clicked()
        });
        end(tui);
        pressed && on.is_some()
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
    tui.style(Style { padding: length(super::theme::PAD), ..col(super::theme::INLINE) })
        .add_with_background_ui(background, |tui, _| {
            // The header: the title over a hairline as wide as the card's content.
            block(tui, |ui| {
                ui.label(RichText::new(title).strong().size(13.5).color(super::theme::TITLE));
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), egui::Sense::hover());
                ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, super::theme::EDGE));
            });
            body(tui)
        })
        .main
}

/// A raised panel with rounded corners: a card's, a hero's. No accent rail: the accent
/// marks what is chosen or live, and a mark on every card would mean nothing.
fn background(ui: &mut egui::Ui, container: &egui_taffy::TaffyContainerUi) {
    let rect = container.full_container();
    ui.painter().rect(
        rect,
        super::theme::R_CARD,
        super::theme::CARD,
        egui::Stroke::new(1.0, super::theme::EDGE),
        egui::StrokeKind::Inside,
    );
}

/// A page's hero: one full-width panel above the cards, its content in a row that
/// wraps (an image, then the headline and its numbers).
pub fn hero<T>(tui: &mut Tui, body: impl FnOnce(&mut Tui) -> T) -> T {
    let style = Style {
        padding: length(super::theme::PAD),
        flex_wrap: FlexWrap::Wrap,
        align_items: Some(AlignItems::Center),
        ..row(super::theme::PAD)
    };
    tui.style(style).add_with_background_ui(background, |tui, _| body(tui)).main
}

/// What a chip says about a state: done or fine, waiting on something, wrong or about
/// to be lost, chosen or live, or just a label.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Wait,
    Bad,
    Accent,
    Quiet,
}

impl Tone {
    fn colour(self) -> Color32 {
        match self {
            Tone::Ok => super::theme::OK,
            Tone::Wait => super::theme::WAIT,
            Tone::Bad => super::theme::BAD,
            Tone::Accent => super::theme::ACCENT,
            Tone::Quiet => super::theme::DIM,
        }
    }
}

/// A state as a pill: its colour's text on a faint wash of it — the panel's one way to
/// show a state at a glance (can open now, covered, soon, opened…).
pub fn pill(ui: &mut egui::Ui, text: impl Into<String>, tone: Tone) -> egui::Response {
    let c = tone.colour();
    egui::Frame::new()
        .fill(c.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, c.gamma_multiply(0.35)))
        .corner_radius(super::theme::R_CHIP)
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| ui.label(RichText::new(text.into()).small().color(c)))
        .response
}

/// `pill` as a node of its own, in a row.
pub fn chip(tui: &mut Tui, text: impl Into<String>, tone: Tone) {
    let text = text.into();
    w(tui, |ui| pill(ui, text, tone));
}

/// A key as a keycap: `F10`, `` ` ``.
pub fn keycap(tui: &mut Tui, key: &str) {
    w(tui, |ui| {
        egui::Frame::new()
            .fill(super::theme::CONTROL)
            .stroke(egui::Stroke::new(1.0, super::theme::EDGE))
            .corner_radius(super::theme::R_CONTROL)
            .inner_margin(egui::Margin::symmetric(7, 1))
            .show(ui, |ui| ui.label(RichText::new(key).monospace().color(super::theme::TITLE)))
    });
}

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

/// A KPI tile: an icon and a label over a big number, the whole tile pressable (it
/// opens the page with the places). `lit` draws the number in the state colour it is
/// given — something that can be done now — else in the title colour; a zero is dim.
pub fn stat(tui: &mut Tui, icon: impl FnOnce(&mut egui::Ui), value: &str, label: &str, lit: Option<Color32>) -> bool {
    use super::theme::{CONTROL, CONTROL_HOVER, DIM, EDGE, R_CONTROL, TITLE};
    w(tui, |ui| {
        let id = ui.next_auto_id();
        let hovered = ui.ctx().data(|d| d.get_temp::<bool>(id).unwrap_or(false));
        let frame = egui::Frame::new()
            .fill(if hovered { CONTROL_HOVER } else { CONTROL })
            .stroke(egui::Stroke::new(1.0, EDGE))
            .corner_radius(R_CONTROL)
            .inner_margin(egui::Margin::symmetric(10, 7))
            .show(ui, |ui| {
                // As wide as its grid cell, less the frame's margins and edge.
                ui.set_min_width((ui.available_width() - 22.0).max(STAT_W));
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    icon(ui);
                    ui.label(RichText::new(label).small().color(DIM));
                });
                let colour = match lit {
                    _ if value == "0" => DIM,
                    Some(c) => c,
                    None => TITLE,
                };
                ui.label(RichText::new(value).size(20.0).strong().color(colour));
            });
        let r = ui.interact(frame.response.rect, id, egui::Sense::click());
        ui.ctx().data_mut(|d| d.insert_temp(id, r.hovered()));
        r.clicked()
    })
}

/// A KPI tile's least width (px), so a row of them lines up.
const STAT_W: f32 = 84.0;

/// A horizontal bar chart, one row per label: the part that can be done now in the
/// accent, the rest of what is left behind it, all to one scale (the longest row), the
/// counts on the right. Above it, a legend for the two parts.
pub fn bars(ui: &mut egui::Ui, rows: &[(String, usize, usize)], legend: (&str, &str)) {
    use super::theme::{ACCENT, CONTROL_HOVER, DIM, TEXT};
    const LABEL: f32 = 116.0;
    const COUNT: f32 = 64.0;
    const ROW: f32 = 18.0;
    let max = rows.iter().map(|r| r.2).max().unwrap_or(1).max(1) as f32;
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.horizontal(|ui| {
        for (text, colour) in [(legend.0, ACCENT), (legend.1, CONTROL_HOVER)] {
            let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 2.0, colour);
            ui.label(RichText::new(text).small().color(DIM));
            ui.add_space(6.0);
        }
    });
    for (label, now, left) in rows {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
        let p = ui.painter();
        let font = egui::FontId::proportional(12.0);
        let colour = if *now > 0 { TEXT } else { DIM };
        let text = p.layout(label.clone(), font.clone(), colour, LABEL - 6.0);
        p.galley(egui::pos2(rect.left(), rect.center().y - text.size().y / 2.0), text, colour);
        let track = egui::Rect::from_min_max(
            egui::pos2(rect.left() + LABEL, rect.center().y - 5.0),
            egui::pos2((rect.right() - COUNT).max(rect.left() + LABEL + 10.0), rect.center().y + 5.0),
        );
        let w = |n: usize| track.width() * n as f32 / max;
        let all = egui::Rect::from_min_size(track.min, egui::vec2(w(*left), track.height()));
        p.rect_filled(all, 3.0, CONTROL_HOVER);
        if *now > 0 {
            let doable = egui::Rect::from_min_size(track.min, egui::vec2(w(*now).max(3.0), track.height()));
            p.rect_filled(doable, 3.0, ACCENT);
        }
        p.text(
            egui::pos2(rect.right(), rect.center().y),
            egui::Align2::RIGHT_CENTER,
            format!("{now} / {left}"),
            egui::FontId::monospace(11.0),
            colour,
        );
    }
}

/// A progress ring: the track, the part done from twelve o'clock clockwise (the accent,
/// the OK colour once complete), the count in the middle and a label under it.
pub fn ring(ui: &mut egui::Ui, side: f32, done: usize, all: usize, label: &str) -> egui::Response {
    use super::theme::{ACCENT, CONTROL_HOVER, DIM, OK, TITLE};
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side + 16.0), egui::Sense::hover());
    let p = ui.painter();
    let centre = egui::pos2(rect.center().x, rect.top() + side / 2.0);
    let width = (side * 0.09).max(4.0);
    let r = side / 2.0 - width / 2.0 - 1.0;
    p.circle_stroke(centre, r, egui::Stroke::new(width, CONTROL_HOVER));
    let frac = if all == 0 { 0.0 } else { (done as f32 / all as f32).clamp(0.0, 1.0) };
    if frac > 0.0 {
        let steps = (64.0 * frac).ceil().max(2.0) as usize;
        let points: Vec<egui::Pos2> = (0..=steps)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * frac * i as f32 / steps as f32;
                centre + egui::vec2(a.cos(), a.sin()) * r
            })
            .collect();
        let colour = if done >= all { OK } else { ACCENT };
        p.add(egui::Shape::line(points, egui::Stroke::new(width, colour)));
    }
    p.text(centre, egui::Align2::CENTER_CENTER, done.to_string(), egui::FontId::proportional(side * 0.26), TITLE);
    p.text(
        egui::pos2(centre.x, centre.y + side * 0.2),
        egui::Align2::CENTER_CENTER,
        format!("/ {all}"),
        egui::FontId::proportional(side * 0.13),
        DIM,
    );
    p.text(
        egui::pos2(rect.center().x, rect.bottom() - 6.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        DIM,
    );
    response
}

/// A thin progress bar `width` px wide (the rest of the row when `None`): `done` of
/// `all` in the accent, OK once complete.
pub fn meter(ui: &mut egui::Ui, width: Option<f32>, done: usize, all: usize) {
    use super::theme::{ACCENT, CONTROL_HOVER, OK};
    let w = width.unwrap_or_else(|| ui.available_width());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 6.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, CONTROL_HOVER);
    if all > 0 && done > 0 {
        let frac = (done as f32 / all as f32).clamp(0.0, 1.0);
        let part = egui::Rect::from_min_size(rect.min, egui::vec2((rect.width() * frac).max(3.0), rect.height()));
        p.rect_filled(part, 3.0, if done >= all { OK } else { ACCENT });
    }
}

/// A vertical scroll area that shows it scrolls: a solid bar (theme.rs) and, at an edge
/// that hides content, a fade into `ground` (the colour behind it) — a list cut off at the
/// bottom otherwise looks complete. `min` keeps the height while laid out at the frame
/// before's size (a scroll area alone never asks for more than it was given).
pub fn scroll<R>(
    ui: &mut egui::Ui,
    id: &str,
    max: f32,
    min: f32,
    ground: Color32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let out = egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(max)
        .min_scrolled_height(min)
        // As wide as it is given, as tall as its content up to `max`.
        .auto_shrink([false, true])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(ui, body);
    let view = out.inner_rect;
    let hidden_above = out.state.offset.y > 1.0;
    let hidden_below = out.state.offset.y + view.height() < out.content_size.y - 1.0;
    // Leave the bar's lane clear of the fade.
    let bar = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_outer_margin + 2.0;
    let lane = egui::Rect::from_min_max(view.min, egui::pos2(view.right() - bar, view.bottom()));
    if hidden_above {
        fade(ui, egui::Rect::from_min_size(lane.min, egui::vec2(lane.width(), FADE)), ground, true);
    }
    if hidden_below {
        let r = egui::Rect::from_min_max(egui::pos2(lane.left(), lane.bottom() - FADE), lane.max);
        fade(ui, r, ground, false);
    }
    out.inner
}

/// How tall a scroll area's edge fade is (px).
const FADE: f32 = 22.0;

/// A vertical gradient over `rect`: `ground` at the edge (`top` or bottom), clear inward.
fn fade(ui: &egui::Ui, rect: egui::Rect, ground: Color32, top: bool) {
    let clear = Color32::from_rgba_premultiplied(0, 0, 0, 0);
    let (upper, lower) = if top { (ground, clear) } else { (clear, ground) };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), upper);
    mesh.colored_vertex(rect.right_top(), upper);
    mesh.colored_vertex(rect.right_bottom(), lower);
    mesh.colored_vertex(rect.left_bottom(), lower);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// Other regions with a count each, as a dim label and quiet chips, most first — the
/// pages' "elsewhere" lines, which read as one long sentence of names.
pub fn regions(tui: &mut Tui, label: &str, mut counts: Vec<(String, usize)>) {
    if counts.is_empty() {
        return;
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let label = label.to_string();
    tui.style(wrap(super::theme::TIGHT)).add(|tui| {
        w(tui, |ui| ui.label(RichText::new(label).small().color(super::theme::DIM)));
        for (place, n) in counts {
            chip(tui, format!("{place} {n}"), Tone::Quiet);
        }
    });
}

/// A long list's order, as a small segmented switch: `first` (by distance, as given)
/// or `grouped` (by kind, with headings). Returns whether it changed.
pub fn order(tui: &mut Tui, grouped: &mut bool, first: &str, by_kind: &str) -> bool {
    let (first, by_kind) = (first.to_string(), by_kind.to_string());
    tui.style(row(super::theme::TIGHT)).add(|tui| {
        let mut changed = false;
        for (on, label) in [(false, first), (true, by_kind)] {
            let text = RichText::new(label).small();
            if w(tui, |ui| ui.selectable_label(*grouped == on, text)).clicked() && *grouped != on {
                *grouped = on;
                changed = true;
            }
        }
        changed
    })
}

/// A group's heading inside a list: its name and count, small and dim, over a hairline.
pub fn group_heading(ui: &mut egui::Ui, name: &str, count: usize) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).small().strong().color(super::theme::DIM));
        ui.label(RichText::new(count.to_string()).small().monospace().color(super::theme::DIM));
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), egui::Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, super::theme::EDGE));
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
