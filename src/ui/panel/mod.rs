//! What the panel draws, and the requests its controls send. Every tab and row is
//! read off the cheat table; the panel reads the worker's last snapshot and never
//! touches the game itself.


mod groups;
mod map;
mod guide;
mod quests;
mod collect;
mod saves;
mod debug;
mod deep;

use super::tw::{self, block, card, choices, field, note, switch, text, w};
use super::{hotkey, Request, Shared};
use crate::actors::{Kind as ThingKind, Sub};
use crate::cheats::{self, Active, Cheat, Effect, Group, Kind, CHEATS};
use crate::engine::Snapshot;
use crate::settings::{self, Settings};
use crate::verify;
use eframe::egui::{self, Color32, RichText};
use egui_taffy::{tui, Tui, TuiBuilderLogic};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

const OK: Color32 = Color32::from_rgb(0x8F, 0xD1, 0x7A);
const BAD: Color32 = Color32::from_rgb(0xF0, 0x82, 0x78);
const WAIT: Color32 = Color32::from_rgb(0xE8, 0xC0, 0x6A);
const DIM: Color32 = Color32::from_gray(150);

/// The window's width before its first page is measured (px).
pub const WIDTH: f32 = 960.0;
/// What a page's width is wrapped in: the window frame's margins and the page's
/// scrollbar (px).
const FRAME: f32 = 16.0;
const SCROLLBAR: f32 = 14.0;
/// The debug page's single card is this wide (px): its tables scroll sideways past it.
const DEBUG_PAGE: f32 = 620.0;
/// The sidebar's width, and the gap between it and the page with the divider in its
/// middle (px).
const NAV: f32 = 172.0;
const DIVIDER: f32 = 14.0;
/// The window is never taller than this share of the monitor; the page scrolls
/// inside it instead.
const MAX_SHARE: f32 = 0.85;
/// Room under the page for its footer, and for the title bar and margins (px).
const FOOTER: f32 = 92.0;
const CHROME: f32 = 56.0;

/// A compact console look: text a size smaller than before, gaps to match.
pub fn install_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(7.0, 5.0);
        s.spacing.button_padding = egui::vec2(7.0, 3.0);
        s.spacing.interact_size.y = 21.0;
        s.spacing.slider_width = 170.0;
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
    });
}

/// egui's own fonts have no Hangul, and neither they nor Malgun Gothic have arrows
/// and shapes like ▾ ▸ ↑ ↓. Malgun Gothic and Segoe UI Symbol ship with Windows, so
/// they are borrowed from the system rather than bundled — fallbacks, in that order;
/// without them the panel still works, with boxes for what is missing.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // The game's language's own font first (its kana or simplified Chinese — Malgun
    // Gothic has neither), only that one: a CJK font is tens of MB.
    let own = match crate::i18n::culture().as_str() {
        "ja" => Some(("own", r"C:\Windows\Fonts\YuGothR.ttc")),
        "zh-Hans" => Some(("own", r"C:\Windows\Fonts\msyh.ttc")),
        _ => None,
    };
    for (name, file) in own.into_iter().chain([("malgun", r"C:\Windows\Fonts\malgun.ttf"), ("symbol", r"C:\Windows\Fonts\seguisym.ttf")]) {
        let Ok(bytes) = std::fs::read(file) else { continue };
        fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.into());
        }
    }
    ctx.set_fonts(fonts);
}

/// The attribute a slider moves — whose live value is shown beside it.
fn chosen(c: &Cheat) -> Option<crate::attr::Attr> {
    match c.kind {
        Kind::Slider { effects, .. } => {
            effects.iter().find_map(|e| if let Effect::Chosen(a) = e { Some(*a) } else { None })
        }
        _ => None,
    }
}

/// A pin kind picker: its colour dot and name, the list of every kind. Whether it changed.
fn pin_picker(ui: &mut egui::Ui, id: &str, kind: &mut crate::minimap::PinKind) -> bool {
    let before = *kind;
    let dot = |k: crate::minimap::PinKind| {
        let [r, g, b] = k.rgb();
        RichText::new(format!("● {}", k.label())).color(Color32::from_rgb(r, g, b))
    };
    egui::ComboBox::from_id_salt(id).width(118.0).selected_text(dot(*kind)).show_ui(ui, |ui| {
        for k in crate::minimap::PinKind::ALL {
            ui.selectable_value(kind, k, dot(k));
        }
    });
    *kind != before
}

/// Guide to a place the survey names: the live goal there if it is loaded, else the
/// place itself (the overlay adds it as a goal).
fn guide_to(state: &mut crate::minimap::MapState, goals: &[crate::goals::Goal], x: &crate::survey::Need) {
    let live = goals.iter().find(|g| (g.at[0] - x.at[0]).hypot(g.at[1] - x.at[1]) < 200.0).map(|g| g.id);
    match live {
        Some(id) => state.target = Some(id),
        None => {
            state.adhoc = Some((x.world.clone(), x.id, x.at, format!("{} {}", x.what, x.label).trim().to_string()));
            state.target = Some(x.id);
        }
    }
    state.chosen = true;
    state.route = true;
}

/// The tool pages in the sidebar, under the cheat groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Map,
    Guide,
    Quests,
    Collect,
    Saves,
    Debug,
}

impl Tool {
    const ALL: [Tool; 6] = [Tool::Map, Tool::Guide, Tool::Quests, Tool::Collect, Tool::Saves, Tool::Debug];

    /// Its name in settings.txt.
    fn id(self) -> &'static str {
        match self {
            Tool::Map => "map",
            Tool::Guide => "guide",
            Tool::Quests => "quests",
            Tool::Collect => "collect",
            Tool::Saves => "saves",
            Tool::Debug => "debug",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tool::Map => tr!("지도"),
            Tool::Guide => tr!("안내"),
            Tool::Quests => tr!("퀘스트"),
            Tool::Collect => tr!("수집"),
            Tool::Saves => tr!("세이브"),
            Tool::Debug => tr!("디버그"),
        }
    }
}

pub struct Panel {
    /// The culture the fonts were installed for.
    fonts_for: String,
    shared: Arc<Shared>,
    tx: Sender<Request>,
    tab: Group,
    /// A tool page instead of a group of cheats, when one is chosen.
    tool: Option<Tool>,
    /// Which kinds' finer sorts are unfolded in the map tab.
    unfolded: [bool; 6],
    marks: verify::Marks,
    on: HashMap<&'static str, bool>,
    value: HashMap<&'static str, f32>,
    /// Sliders send while dragging, at most this often each.
    slid: HashMap<&'static str, Instant>,
    /// After sending, the worker's next snapshot may predate it. Until it has caught
    /// up, the checkboxes are not overwritten from it.
    sent: Option<Instant>,
    reply: Option<(bool, String, Instant)>,
    /// The console at the foot of the window, and whether the window was showing on
    /// the last frame (to put the cursor in the console as it opens).
    console: super::console::Console,
    was_visible: bool,
    /// The collectible sort unfolded in the collect tab.
    unfolded_collect: Option<&'static str>,
    /// Puzzle answers and vault codes asked for, by id (not kept between runs).
    revealed: std::collections::HashSet<u64>,
    /// The puzzle list shows key doors and item placements too.
    show_placements: bool,
    /// Steam's achievements (game/achievements.rs), read again every 10 s.
    achievements: Option<(std::time::Instant, Vec<crate::game::achievements::Achievement>)>,
    /// The achievement card lists the unlocked ones too.
    show_unlocked: bool,
    /// The save backups, as last listed.
    backups: Vec<(String, std::path::PathBuf)>,
    backups_read: Option<Instant>,
    /// The size last asked of the window, so it is asked once per change.
    height: f32,
    width: f32,
    /// What the player turned on — not what is on right now. The game exiting or the
    /// gate closing switches cheats off; that must not become the saved choice, or
    /// nothing would come back next time.
    wanted: Vec<Active>,
    keep: bool,
    /// Cheats to turn back on from last time, once the gate first opens.
    resume: Option<Vec<Active>>,
    /// settings.txt as last written, and when.
    saved: (String, Instant),
}

impl Panel {
    pub fn new(shared: Arc<Shared>, tx: Sender<Request>) -> Panel {
        let saved = settings::load();
        // A slider takes its saved value if it is still in range, else its default.
        let value = CHEATS
            .iter()
            .filter_map(|c| match c.kind {
                Kind::Slider { default, min, max, .. } => {
                    let v = saved.values.get(c.id).copied().filter(|v| (min..=max).contains(v));
                    Some((c.id, v.unwrap_or(default)))
                }
                _ => None,
            })
            .collect();
        // Through the same parser as the CLI, so a cheat since removed or a value
        // since out of range is dropped rather than sent.
        let resume: Vec<Active> = saved
            .on
            .iter()
            .filter_map(|(id, v)| Active::parse(&if *v == 0.0 { id.clone() } else { format!("{id}={v}") }).ok())
            .collect();
        let tab = saved.tab.as_deref();
        Panel {
            fonts_for: crate::i18n::culture(),
            shared,
            tx,
            tab: Group::ALL.into_iter().find(|g| Some(g.id()) == tab).unwrap_or(Group::Survival),
            tool: Tool::ALL.into_iter().find(|t| Some(t.id()) == tab),
            unfolded: [false; 6],
            wanted: resume.clone(),
            keep: saved.keep,
            resume: (saved.keep && !resume.is_empty()).then_some(resume),
            saved: (settings::render(&saved), Instant::now()),
            marks: verify::load(),
            on: HashMap::new(),
            value,
            slid: HashMap::new(),
            sent: None,
            reply: None,
            console: Default::default(),
            was_visible: false,
            unfolded_collect: None,
            revealed: Default::default(),
            show_placements: false,
            achievements: None,
            show_unlocked: false,
            backups: Vec::new(),
            backups_read: None,
            height: 0.0,
            width: 0.0,
        }
    }

    fn active(&self) -> Vec<Active> {
        CHEATS
            .iter()
            .filter(|c| {
                !matches!(c.kind, Kind::Set { .. } | Kind::SetStock { .. })
                    && self.on.get(c.id).copied().unwrap_or(false)
            })
            .map(|c| Active { cheat: c.id, value: self.value.get(c.id).copied().unwrap_or(0.0) })
            .collect()
    }

    /// Only ever called for something the player did — so what it sends is also
    /// what they want kept.
    fn send_active(&mut self) {
        self.wanted = self.active();
        let _ = self.tx.send(Request::Toggles(self.wanted.clone()));
        self.sent = Some(Instant::now());
    }

    fn settings(&self) -> Settings {
        Settings {
            keep: self.keep,
            tab: Some(
                self.tool.map_or(self.tab.id(), Tool::id).to_string(),
            ),
            pos: *self.shared.pos.lock().unwrap(),
            on: self.wanted.iter().map(|a| (a.cheat.to_string(), a.value)).collect(),
            values: self.value.iter().map(|(id, v)| (id.to_string(), *v)).collect(),
        }
    }

    /// Write settings.txt when something in it changed — at most twice a second
    /// while a slider is dragged, and always when `now` (the panel closing).
    fn persist(&mut self, now: bool) {
        let text = settings::render(&self.settings());
        if text != self.saved.0 && (now || self.saved.1.elapsed() >= Duration::from_millis(500)) {
            match settings::save(&self.settings()) {
                Ok(()) => self.saved = (text, Instant::now()),
                Err(e) => self.reply = Some((false, e, Instant::now())),
            }
        }
    }

    /// Turn last time's cheats back on — the first time the gate is open.
    fn resume(&mut self, snap: &Snapshot) {
        if snap.gate.is_err() {
            return;
        }
        let Some(resume) = self.resume.take() else { return };
        for a in &resume {
            self.on.insert(a.cheat, true);
            self.value.insert(a.cheat, a.value);
        }
        self.send_active();
        self.reply = Some((true, trf!("지난번에 켜 둔 치트 {a0}개를 다시 켰습니다", a0 = resume.len()), Instant::now()));
    }

    /// The engine is the truth about what is on: the gate closing or the game exiting
    /// switches cheats off without the panel asking.
    fn follow(&mut self, snap: &Snapshot) {
        if self.sent.is_some_and(|t| t.elapsed() < Duration::from_millis(800)) {
            return;
        }
        for c in CHEATS {
            let a = snap.active.iter().find(|a| a.cheat == c.id);
            self.on.insert(c.id, a.is_some());
            if let (Some(a), Kind::Slider { .. }) = (a, c.kind) {
                self.value.insert(c.id, a.value);
            }
        }
    }

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let (rect, bar) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click_and_drag());
        if bar.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        // Laid out after the drag area, so the buttons sit on top of it and get their
        // own clicks rather than starting a drag.
        let row = egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center));
        ui.scope_builder(row, |ui| {
            ui.label(RichText::new("Hell Is Us Mod").strong());
            ui.label(RichText::new("`").color(DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(" × ").on_hover_text(tr!("닫기 — 원래 값으로 되돌리고 종료")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(" — ").on_hover_text(tr!("숨기기 (` 키로 다시 열기)")).clicked() {
                    hotkey::hide(&self.shared);
                }
                let label = if self.console.open { tr!("콘솔 ▾") } else { tr!("콘솔 ▸") };
                if ui.selectable_label(self.console.open, label).on_hover_text(tr!("CLI 명령 콘솔")).clicked() {
                    self.console.open = !self.console.open;
                    if self.console.open {
                        self.console.focus();
                    }
                }
            });
        });
        ui.separator();
    }

    /// The game and the hero gate, stacked for the sidebar.
    fn status(&self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let launching = self.shared.launched.load(Ordering::SeqCst);
        let line = |ui: &mut egui::Ui, title: &str, text: RichText| {
            ui.label(RichText::new(title).color(DIM).small());
            ui.add(egui::Label::new(text).wrap());
            ui.add_space(4.0);
        };
        match snap.map(|s| &s.game) {
            Some(Ok((pid, version))) => {
                line(ui, tr!("게임"), RichText::new(trf!("● 연결됨\nv{version} · PID {pid}", version = version, pid = pid)).color(OK));
                // A build the mod was not checked on: names may have moved.
                if version != crate::game::TESTED_BUILD {
                    let warn = trf!("⚠ 확인한 빌드는 {tested} — 이상하면 콘솔에서 `doctor` 를 실행하세요", tested = crate::game::TESTED_BUILD);
                    line(ui, "", RichText::new(warn).color(WAIT).small());
                }
            }
            _ if launching => line(ui, tr!("게임"), RichText::new(tr!("실행하는 중 — 켜지면 자동으로 연결")).color(WAIT)),
            Some(Err(e)) => line(ui, tr!("게임"), RichText::new(trf!("○ 연결 안 됨 — {e}", e = e)).color(DIM)),
            None => line(ui, tr!("게임"), RichText::new(tr!("시작하는 중…")).color(DIM)),
        }
        match snap.map(|s| &s.gate) {
            Some(Ok(())) => line(ui, tr!("주인공 게이트"), RichText::new(tr!("● 열림 — 조작 중")).color(OK)),
            Some(Err(e)) if snap.is_some_and(|s| s.game.is_ok()) => {
                line(ui, tr!("주인공 게이트"), RichText::new(trf!("○ 닫힘 — {e}", e = e)).color(WAIT))
            }
            _ => line(ui, tr!("주인공 게이트"), RichText::new(tr!("○ 닫힘")).color(DIM)),
        }
    }

    /// The sidebar's pages: each cheat group (with how many are on), then the map &
    /// guide, then debugging.
    fn nav(&mut self, ui: &mut egui::Ui) {
        let item = |ui: &mut egui::Ui, on: bool, text: String| {
            let label = RichText::new(text).size(13.0).color(if on { Color32::WHITE } else { Color32::from_gray(185) });
            let button = egui::Button::new(label)
                .fill(if on { Color32::from_rgb(0x2A, 0x3A, 0x52) } else { Color32::TRANSPARENT })
                .stroke(if on { egui::Stroke::new(1.0, Color32::from_rgb(0x4A, 0x7A, 0xB8)) } else { egui::Stroke::NONE })
                .corner_radius(6.0)
                .min_size(egui::vec2(ui.available_width(), 26.0));
            ui.add(button).clicked()
        };
        ui.label(RichText::new(tr!("치트")).color(DIM).small());
        for g in Group::ALL {
            let on = CHEATS.iter().filter(|c| c.group == g && self.on.get(c.id).copied().unwrap_or(false)).count();
            let text = if on > 0 { format!("{}  ({on})", g.label()) } else { g.label().to_string() };
            if item(ui, self.tool.is_none() && self.tab == g, text) {
                self.tab = g;
                self.tool = None;
            }
        }
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("도구")).color(DIM).small());
        for tool in Tool::ALL {
            if item(ui, self.tool == Some(tool), tool.label().to_string()) {
                self.tool = Some(tool);
            }
        }
    }

    /// The page chosen in the sidebar: cards on an auto-fit grid (tw.rs) — side by
    /// side while each can be `CARD_MIN` wide, stacked when not.
    fn page(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        match self.tab {
            _ if self.tool == Some(Tool::Debug) => block(t, |ui| self.debug_tab(ui, snap)),
            _ if self.tool == Some(Tool::Saves) => {
                t.style(tw::full(tw::cards(tw::CARD_MIN))).add(|t| {
                    t.style(tw::col(tw::GAP)).add(|t| self.backups_card(t));
                    t.style(tw::col(tw::GAP)).add(|t| self.slots_card(t));
                });
            }
            _ if self.tool.is_some() => self.map_tab(t, snap),
            g => t.style(tw::full(tw::cards(tw::CARD_MIN))).add(|t| {
                t.style(tw::col(tw::GAP)).add(|t| {
                    card(t, g.label(), |t| self.held(t, g, snap));
                    if g == Group::Movement {
                        self.positions(t, snap);
                    }
                });
                t.style(tw::col(tw::GAP)).add(|t| self.summary(t, snap));
            }),
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let pending = snap.map_or(0, |s| s.pending);
        ui.horizontal(|ui| {
            let restore = ui.add_enabled(pending > 0, egui::Button::new(trf!("모두 원래대로 ({pending})", pending = pending)));
            if restore.clicked() {
                let _ = self.tx.send(Request::Restore);
                self.on.clear();
                self.wanted.clear();
                self.resume = None;
                self.sent = Some(Instant::now());
            }
            ui.checkbox(&mut self.keep, tr!("다음에도 켜 둔 치트 유지"))
                .on_hover_text(tr!("다음 실행 때, 주인공을 조작할 수 있게 되면 지금 켜 둔 치트를 다시 켭니다."));
        });
        let attached = snap.is_some_and(|s| s.game.is_ok());
        if let Some(n) = snap.and_then(|s| s.notice.clone()) {
            ui.add(egui::Label::new(RichText::new(n).color(if attached { WAIT } else { DIM }).small()).wrap());
        }
        if let Some((ok, text, at)) = &self.reply {
            if at.elapsed() < Duration::from_secs(5) {
                ui.add(egui::Label::new(RichText::new(text).color(if *ok { OK } else { BAD }).small()).wrap());
            }
        }
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(
                RichText::new(tr!("주인공을 조작하는 동안만 값을 씁니다 · 업적은 차단되지 않습니다")).color(DIM).small(),
            )
            .wrap(),
        );
    }

    /// The window is exactly as tall as what is in it: nothing clipped, nothing empty.
    fn fit(&mut self, ui: &egui::Ui, width: f32, height: f32) {
        let (width, height) = (width.ceil(), height.ceil());
        if (height - self.height).abs() > 1.0 || (width - self.width).abs() > 1.0 {
            (self.width, self.height) = (width, height);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
        }
    }

    /// How wide the chosen page is: as many cards side by side as it has columns of
    /// them — two for the cheat groups and the map, one wide one for debugging.
    fn page_width(&self) -> f32 {
        if self.tool == Some(Tool::Debug) {
            DEBUG_PAGE
        } else {
            tw::cards_width(2)
        }
    }
}

impl Panel {
    /// The console, dropped down from the top of the game window across its width and
    /// see-through, as in Half-Life: its own window (an egui viewport), shown while the
    /// panel is and the console is open.
    fn console_window(&mut self, ctx: &egui::Context) {
        let game = self.shared.game_pid.load(std::sync::atomic::Ordering::SeqCst);
        let Some((_, r)) = (self.console.open && game != 0).then(|| hotkey::game_window(game)).flatten() else { return };
        let ppp = ctx.pixels_per_point();
        let (w, h) = ((r.right - r.left) as f32 / ppp, (r.bottom - r.top) as f32 / ppp);
        let size = egui::vec2(w, (h * super::console::SHARE).max(180.0));
        let builder = egui::ViewportBuilder::default()
            .with_title("Hell Is Us Mod — console")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_position(egui::pos2(r.left as f32 / ppp, r.top as f32 / ppp))
            .with_inner_size(size);
        let console = &mut self.console;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("hiumod-console"), builder, |ctx, _| {
            let frame = egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(8, 10, 14, 200))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(120, 160, 200, 120)));
            egui::CentralPanel::default().frame(frame).show(ctx, |ui| console.ui(ui));
            if console.wants_focus() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        });
        // Under the panel, always — taking the keyboard raised it over the panel.
        hotkey::console_under(self.shared.hwnd.load(std::sync::atomic::Ordering::SeqCst) as _);
    }
}

impl eframe::App for Panel {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // The game's language changed: its font.
        let culture = crate::i18n::culture();
        if self.fonts_for != culture {
            install_fonts(ui.ctx());
            self.fonts_for = culture;
        }
        if self.shared.hwnd.load(Ordering::SeqCst) == 0 {
            if let Ok(RawWindowHandle::Win32(h)) = frame.window_handle().map(|h| h.as_raw()) {
                self.shared.hwnd.store(h.hwnd.get(), Ordering::SeqCst);
            }
        }
        if let Some((ok, text)) = self.shared.reply.lock().unwrap().take() {
            self.reply = Some((ok, text, Instant::now()));
        }
        let snap = self.shared.snap.lock().unwrap().clone();
        if let Some(s) = &snap {
            self.resume(s);
            self.follow(s);
        }
        let open = snap.as_ref().is_some_and(|s| s.gate.is_ok());

        // As tall as what it shows, up to a share of the monitor; past that the page
        // scrolls and the sidebar and footer stay put.
        let monitor = ui.ctx().input(|i| i.viewport().monitor_size).map_or(1080.0, |m| m.y);
        let max_height = (monitor * MAX_SHARE).max(480.0);
        let page_height = (max_height - CHROME - FOOTER).max(240.0);
        // Opening the window puts the cursor in the console.
        let visible = self.shared.visible.load(std::sync::atomic::Ordering::SeqCst);
        if visible && !self.was_visible && self.console.open {
            self.console.focus();
        }
        self.was_visible = visible;
        // As wide as the page's cards side by side, plus the sidebar — never wider than
        // the monitor.
        let monitor_w = ui.ctx().input(|i| i.viewport().monitor_size).map_or(1920.0, |m| m.x);
        let width = (FRAME + NAV + DIVIDER + self.page_width() + SCROLLBAR).min(monitor_w * 0.9);
        let used = egui::Frame::central_panel(ui.style())
            .show(ui, |ui| {
                ui.set_width(width - FRAME);
                ui.set_max_width(width - FRAME);
                self.title_bar(ui);
                // `grid grid-cols-[172px_1fr]`: the sidebar, then the page — which
                // scrolls inside the window past `page_height`, over its own grid of
                // cards, with the footer under it.
                let left = ui.cursor().min.x;
                let top = ui.cursor().min.y;
                let shell = tw::full(tw::sidebar(NAV, DIVIDER));
                tui(ui, ui.id().with("shell")).reserve_available_width().style(shell).show(|t| {
                    block(t, |ui| {
                        self.status(ui, snap.as_ref());
                        ui.separator();
                        self.nav(ui);
                    });
                    block(t, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("page")
                            .max_height(page_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.add_enabled_ui(open, |ui| {
                                    tui(ui, ui.id().with("page"))
                                        .reserve_available_width()
                                        .style(tw::full(tw::col(tw::GAP)))
                                        .show(|t| self.page(t, snap.as_ref()));
                                });
                            });
                        ui.separator();
                        self.footer(ui, snap.as_ref());
                    });
                });
                let x = left + NAV + DIVIDER / 2.0;
                let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                let bottom = ui.min_rect().bottom();
                ui.painter().vline(x, top..=bottom, stroke);

            })
            .response
            .rect;
        self.fit(ui, width, used.height().min(max_height));
        self.console_window(ui.ctx());
        self.persist(false);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist(true);
    }
}

/// The keys the map uses other than `mine`.
fn others(state: &crate::minimap::MapState, mine: u8) -> Vec<u8> {
    state.keys().into_iter().filter(|&k| k != mine).collect()
}

/// A function key, F1–F12 — none the other pickers hold.
fn key_picker(ui: &mut egui::Ui, id: &str, key: &mut u8, taken: &[u8]) {
    egui::ComboBox::from_id_salt(id).width(72.0).selected_text(format!("F{key}")).show_ui(ui, |ui| {
        for k in (1..=12u8).filter(|&k| crate::minimap::usable_key(k) && !taken.contains(&k)) {
            ui.selectable_value(key, k, format!("F{k}"));
        }
    });
}

/// An on/off switch — clearer at a glance than a checkbox.
pub(super) fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = ui.spacing().interact_size.y * egui::vec2(1.8, 0.9);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let radius = rect.height() / 2.0;
        let track = if *on { OK.gamma_multiply(0.85) } else { Color32::from_gray(70) };
        ui.painter().rect_filled(rect, radius, track);
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), radius - 3.0, Color32::from_gray(235));
    }
    response
}

/// A pill that is filled with the kind's colour while on.
fn chip(text: &str, on: bool, colour: Color32) -> egui::Button<'static> {
    let label = RichText::new(text.to_string()).color(if on { Color32::from_gray(15) } else { DIM });
    egui::Button::new(label)
        .fill(if on { colour } else { Color32::TRANSPARENT })
        .stroke(egui::Stroke::new(1.0, if on { colour } else { colour.gamma_multiply(0.5) }))
        .corner_radius(12.0)
}
