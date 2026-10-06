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
pub fn masonry(tui: &mut Tui, key: &str, columns: usize, n: usize, card: impl FnMut(&mut Tui, usize)) {
    masonry_pinned(tui, key, columns, n, |_| None, card)
}

/// `masonry`, with cards that keep their column (`pin(i)`: `Some(column)`, the last one at
/// most): a card that grows and shrinks as it is used (what is followed, on the Guide page)
/// stays where the eye looks for it, and the others fill around it. And the cards are not
/// moved for every change in height: the columns of the frame before are kept while they
/// are within `SLACK` of the best.
pub fn masonry_pinned(
    tui: &mut Tui,
    key: &str,
    columns: usize,
    n: usize,
    pin: impl Fn(usize) -> Option<usize>,
    mut card: impl FnMut(&mut Tui, usize),
) {
    const GUESS: f32 = 240.0;
    const SLACK: f32 = 160.0;
    let columns = columns.clamp(1, n.max(1));
    let id = |i: usize| egui::Id::new(("masonry", key, i));
    let kept = egui::Id::new(("masonry-lanes", key));
    let pins: Vec<Option<usize>> = (0..n).map(|i| pin(i).map(|c| c.min(columns - 1))).collect();
    let (heights, before) = {
        let ctx = tui.egui_ctx();
        let heights: Vec<f32> = (0..n).map(|i| ctx.data(|d| d.get_temp::<f32>(id(i))).unwrap_or(GUESS)).collect();
        (heights, ctx.data(|d| d.get_temp::<Vec<Vec<usize>>>(kept)))
    };
    let best = place_pinned(&heights, columns, &pins);
    let lanes = match before.filter(|b| still_fits(b, columns, &pins)) {
        Some(b) if tallest(&b, &heights) <= tallest(&best, &heights) + SLACK => b,
        _ => best,
    };
    tui.egui_ctx().data_mut(|d| d.insert_temp(kept, lanes.clone()));
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
#[cfg(test)]
fn place(heights: &[f32], columns: usize) -> Vec<Vec<usize>> {
    place_pinned(heights, columns, &vec![None; heights.len()])
}

/// `place`, a card pinned going to its own column whatever the heights.
pub fn place_pinned(heights: &[f32], columns: usize, pins: &[Option<usize>]) -> Vec<Vec<usize>> {
    let columns = columns.max(1);
    let mut lanes = vec![Vec::new(); columns];
    let mut tall = vec![0.0f32; columns];
    for (i, h) in heights.iter().enumerate() {
        let c = match pins.get(i).copied().flatten() {
            Some(c) => c.min(columns - 1),
            None => (0..columns).min_by(|&a, &b| tall[a].total_cmp(&tall[b])).unwrap(),
        };
        lanes[c].push(i);
        tall[c] += h + GAP;
    }
    lanes
}

/// The tallest column's height, with `heights`.
fn tallest(lanes: &[Vec<usize>], heights: &[f32]) -> f32 {
    lanes
        .iter()
        .map(|l| l.iter().map(|&i| heights.get(i).copied().unwrap_or(0.0) + GAP).sum::<f32>())
        .fold(0.0, f32::max)
}

/// Whether columns kept from the frame before still hold every card once, in `columns`
/// columns, the pinned ones in theirs.
fn still_fits(lanes: &[Vec<usize>], columns: usize, pins: &[Option<usize>]) -> bool {
    let mut seen = vec![false; pins.len()];
    if lanes.len() != columns {
        return false;
    }
    for (c, lane) in lanes.iter().enumerate() {
        for &i in lane {
            if i >= seen.len() || seen[i] || pins[i].is_some_and(|p| p != c) {
                return false;
            }
            seen[i] = true;
        }
    }
    seen.iter().all(|s| *s)
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

/// A page's cards on a grid of `columns`, card `i` `spans[i]` columns wide (no wider than
/// the grid), the cards of a row stretched to one height: for a page whose long lists
/// want the width (a quest list, a vault table) beside short cards. `card(tui, i)` draws
/// card `i`.
pub fn spans(tui: &mut Tui, columns: usize, spans: &[u16], mut card: impl FnMut(&mut Tui, usize)) {
    let columns = columns.max(1);
    let grid = Style { align_items: Some(AlignItems::Stretch), ..grid(columns, GAP) };
    tui.style(grid).add(|tui| {
        for (i, n) in spans.iter().enumerate() {
            span(tui, (*n).min(columns as u16), |tui| card(tui, i));
        }
    });
}

/// `col-span-{n}`: what `body` holds as one grid item `n` columns wide.
pub fn span<T>(tui: &mut Tui, n: u16, body: impl FnOnce(&mut Tui) -> T) -> T {
    use taffy::prelude::{auto, fr, span as across};
    // Its one row fills the cell, so a card in it is as tall as the row (when the grid
    // stretches its items).
    tui.style(Style {
        grid_column: taffy::Line { start: auto(), end: across(n.max(1)) },
        grid_template_rows: vec![fr(1.0_f32)],
        align_items: Some(AlignItems::Stretch),
        ..col(0.0)
    })
    .add(body)
}

/// Cards stacked in one grid cell: each as tall as its content, the last one taking the
/// rest of the cell's height (so the stack ends level with its row's other cards, with no
/// gap opening between the cards). `rows` is how many cards `body` adds.
pub fn stack(tui: &mut Tui, rows: usize, body: impl FnOnce(&mut Tui)) {
    use taffy::prelude::{auto, fr};
    let mut template = vec![auto(); rows.saturating_sub(1)];
    template.push(fr(1.0_f32));
    tui.style(Style {
        grid_template_rows: template,
        align_items: Some(AlignItems::Stretch),
        align_content: Some(taffy::AlignContent::Start),
        ..col(GAP)
    })
    .add(body);
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

/// One line of a list that can be followed (guide/track.rs): an icon, the line itself
/// (pressing it does nothing: following is the button's, on every page), its own buttons
/// (`end`), and the Follow toggle at the end. `followed`: `None`, no toggle (done, or
/// elsewhere); `Some(None)`, "Follow"; `Some(Some(colour))`, "Following" in the track's
/// colour. Returns whether the toggle was pressed.
pub fn track_line(
    tui: &mut Tui,
    followed: Option<Option<[u8; 3]>>,
    icon: impl FnOnce(&mut egui::Ui),
    text: impl Into<RichText>,
    end: impl FnOnce(&mut Tui),
) -> bool {
    let text = text.into();
    tui.style(row(super::theme::TIGHT)).add(|tui| {
        w(tui, icon);
        tui.style(grow(row(super::theme::TIGHT))).add(|tui| {
            block(tui, |ui| {
                ui.add(egui::Label::new(text).wrap());
            });
        });
        end(tui);
        match followed {
            Some(c) => follow_toggle(tui, c),
            // Its room kept: the buttons before it stay in line with the lines that have one.
            None => {
                w(tui, |ui| {
                    ui.allocate_space(egui::vec2(TOGGLE_W, 1.0));
                });
                false
            }
        }
    })
}

/// How wide the Follow toggle is, the same on every line.
const TOGGLE_W: f32 = 78.0;

/// The Follow toggle, its two states plain at a glance: "Follow" on a control's ground
/// when not followed; "● Let go" on the track's colour, deepened, when followed (what a
/// press does next, as the button's word). Whether pressed.
pub fn follow_toggle(tui: &mut Tui, colour: Option<[u8; 3]>) -> bool {
    w(tui, |ui| {
        let size = [TOGGLE_W, ui.spacing().interact_size.y];
        let button = match colour {
            Some([r, g, b]) => {
                // The colour deepened, so white reads on every track's colour.
                let deep = |c: u8| (c as f32 * 0.55) as u8;
                egui::Button::new(
                    RichText::new(format!("● {}", tr!("TRACK_DROP"))).color(egui::Color32::WHITE).strong(),
                )
                .fill(egui::Color32::from_rgb(deep(r), deep(g), deep(b)))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(r, g, b)))
            }
            None => egui::Button::new(RichText::new(tr!("QUEST_FOLLOW")).color(super::theme::TEXT))
                .fill(super::theme::CONTROL)
                .stroke(egui::Stroke::new(1.0, super::theme::EDGE)),
        };
        let hint = if colour.is_some() { tr!("TRACK_FOLLOWING_HINT") } else { tr!("TRACK_TOGGLE_HINT") };
        ui.add_sized(size, button).on_hover_text(hint).clicked()
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
    card_with(tui, title, |_| {}, body)
}

/// A card whose header holds an action at its right end (a button that acts on the
/// whole card, as "let go of all"), level with the title.
pub fn card_with<T>(
    tui: &mut Tui,
    title: &str,
    action: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut Tui) -> T,
) -> T {
    // Its rows at the top: a card stretched to its row's height keeps its spare room
    // below them, not spread between them.
    tui.style(Style {
        padding: length(super::theme::PAD),
        align_content: Some(taffy::AlignContent::Start),
        ..col(super::theme::INLINE)
    })
    .add_with_background_ui(background, |tui, _| {
        // The header: the title over a hairline as wide as the card's content.
        block(tui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong().size(13.5).color(super::theme::TITLE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), action);
            });
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 5.0), egui::Sense::hover());
            ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, super::theme::EDGE));
        });
        body(tui)
    })
    .main
}

/// A raised panel with rounded corners: a card's, a hero's. No accent rail: the accent
/// marks what is chosen or live, and a mark on every card would mean nothing.
/// A card's (and the hero's) background, as thin glass: the card's colour a little
/// see-through, so the panel's backdrop shows faintly behind; a soft sheen over its top
/// that fades out a third of the way down; a hairline of light along the top edge; the
/// usual edge. No blur (egui draws none) and nothing loud.
fn background(ui: &mut egui::Ui, container: &egui_taffy::TaffyContainerUi) {
    let rect = container.full_container();
    let p = ui.painter();
    let radius = super::theme::R_CARD as f32;
    let c = super::theme::CARD;
    p.rect_filled(rect, radius, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 206));
    // The sheen: white, faint at the top, gone by 40 % of the height; inset by the corner
    // radius at its sides so the rounded corners stay clean.
    let sheen = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, rect.top() + 1.0),
        egui::pos2(rect.right() - 1.0, rect.top() + rect.height().min(260.0) * 0.4),
    );
    let mut mesh = egui::Mesh::default();
    let (top, clear) = (Color32::from_white_alpha(9), Color32::from_white_alpha(0));
    mesh.colored_vertex(egui::pos2(sheen.left() + radius * 0.6, sheen.top()), top);
    mesh.colored_vertex(egui::pos2(sheen.right() - radius * 0.6, sheen.top()), top);
    mesh.colored_vertex(sheen.right_bottom(), clear);
    mesh.colored_vertex(sheen.left_bottom(), clear);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
    // The light catching the top edge.
    p.hline(
        (rect.left() + radius)..=(rect.right() - radius),
        rect.top() + 0.5,
        egui::Stroke::new(1.0, Color32::from_white_alpha(22)),
    );
    p.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, super::theme::EDGE.gamma_multiply(0.85)),
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
    w(tui, |ui| kbd(ui, key));
}

/// A key as a keycap in an egui row (HTML's `<kbd>`): the control fill, an edge, the
/// key in monospace. A chord is its keys joined by "+": `kbd(ui, "Ctrl")`, "+", `` ` ``.
pub fn kbd(ui: &mut egui::Ui, key: &str) -> egui::Response {
    egui::Frame::new()
        .fill(super::theme::CONTROL)
        .stroke(egui::Stroke::new(1.0, super::theme::EDGE))
        .corner_radius(super::theme::R_CONTROL)
        .inner_margin(egui::Margin::symmetric(6, 0))
        .show(ui, |ui| ui.label(RichText::new(key).monospace().color(super::theme::TITLE)))
        .response
}

/// A form row: the label in 42 % of the row (72–200 px, wrapping), then the
/// controls in what is left, wrapping onto a second line rather than overflowing.
/// Controls that cannot share a line with the label at all (a set of choices wider than
/// what is left) go under it, as wide as the row: the row wraps, so nothing in it runs
/// past the card's edge.
pub fn field<T>(tui: &mut Tui, label: impl Into<RichText>, body: impl FnOnce(&mut Tui) -> T) -> T {
    let label = label.into();
    let line = Style { align_items: Some(AlignItems::Center), flex_wrap: FlexWrap::Wrap, ..row(super::theme::BLOCK) };
    tui.style(line).add(|tui| {
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
        // As wide as its controls want (their one line), else what is left; never less
        // than its widest control, so a row too narrow for both wraps instead.
        tui.style(Style { flex_grow: 1.0, flex_shrink: 1.0, flex_basis: auto(), ..wrap(super::theme::INLINE) })
            .add(body)
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

/// A tab bar: the open tab lit (a tinted top, an accent underline, bright text), the others dim
/// text that brightens under the pointer, a rule under the row. Wraps when narrow. Returns whether
/// another tab was opened.
pub fn tabs(tui: &mut Tui, open: &mut u8, names: &[&str]) -> bool {
    use super::theme::{ACCENT, ACCENT_DEEP, DIM, TEXT};
    block(tui, |ui| {
        let mut changed = false;
        let top = ui.cursor().top();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 4.0);
            for (k, name) in names.iter().enumerate() {
                let on = *open == k as u8;
                let font = egui::FontId::proportional(13.5);
                let galley = ui.painter().layout_no_wrap(name.to_string(), font, TEXT);
                let size = galley.size() + egui::vec2(22.0, 14.0);
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                // a tab to screen readers and UI Automation, selected or not
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, *name));
                let hover = resp.hovered();
                let p = ui.painter();
                if on {
                    p.rect_filled(rect, egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, ACCENT_DEEP);
                    p.rect_filled(
                        egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.5), rect.right_bottom()),
                        1.0,
                        ACCENT,
                    );
                } else if hover {
                    p.rect_filled(
                        rect,
                        egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 },
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    );
                }
                let colour = if on {
                    TEXT
                } else if hover {
                    TEXT.gamma_multiply(0.9)
                } else {
                    DIM
                };
                p.galley(rect.center() - galley.size() / 2.0, galley, colour);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !on {
                    *open = k as u8;
                    changed = true;
                }
            }
        });
        // the rule under the whole row, the open tab's underline over it
        let y = ui.cursor().top().max(top) - 3.0;
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
        );
        ui.add_space(4.0);
        changed
    })
}

/// Choices that wrap onto the next line.
pub fn choices<T>(tui: &mut Tui, body: impl FnOnce(&mut Tui) -> T) -> T {
    tui.style(wrap(super::theme::INLINE)).add(body)
}

/// A distance at the end of a line, small and dim (nothing when it is unknown): a column of its
/// own, so the line before it wraps on its words, not round the number.
pub fn distance(tui: &mut Tui, span: String) {
    if !span.is_empty() {
        w(tui, |ui| ui.label(RichText::new(span).monospace().size(11.5).color(super::theme::DIM)));
    }
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
            // the rail apart from a well's fill (both were CONTROL): the whole range shows
            ui.visuals_mut().widgets.inactive.bg_fill = super::theme::EDGE;
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

/// A scroll area inside a card, as tall as its content up to `max` (px), over a taffy
/// column of its own. Its room is given outright: a ScrollArea is no taller than its
/// parent's room, and a taffy leaf's room is last frame's height, which starts at 0.
pub fn scroll_list(tui: &mut Tui, id: &str, max: f32, body: impl FnOnce(&mut Tui)) {
    block(tui, |ui| {
        let room = egui::vec2(ui.available_width(), max);
        ui.allocate_ui_with_layout(room, egui::Layout::top_down(egui::Align::Min), |ui| {
            scroll(ui, id, max, 0.0, super::theme::CARD, |ui| {
                egui_taffy::tui(ui, ui.id().with(id))
                    .reserve_available_width()
                    .style(full(col(super::theme::INLINE)))
                    .show(body);
            });
        });
    });
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
        // Pinned: the first to the first column, the second to the last, whatever the heights.
        let pins = [Some(0), Some(9), None, None];
        assert_eq!(place_pinned(&[50.0, 900.0, 100.0, 100.0], 2, &pins), vec![vec![0, 2, 3], vec![1]]);
        // As masonry_pinned has them: each within the columns.
        let pins = [Some(0), Some(1), None, None];
        assert!(still_fits(&[vec![0, 2], vec![1, 3]], 2, &pins));
        assert!(!still_fits(&[vec![1, 2], vec![0, 3]], 2, &pins), "a pinned card out of its column");
        assert!(!still_fits(&[vec![0, 2], vec![1]], 2, &pins), "a card missing");
    }

    #[test]
    fn grow_gives_back_its_width() {
        let s = grow(row(6.0));
        assert_eq!(s.flex_grow, 1.0);
        assert_eq!(s.min_size.width, length(0.0_f32));
        assert_eq!(s.flex_direction, FlexDirection::Row);
    }
}
