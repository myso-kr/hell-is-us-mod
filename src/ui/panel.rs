//! What the panel draws, and the requests its controls send. Every tab and row is
//! read off the cheat table; the panel reads the worker's last snapshot and never
//! touches the game itself.

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

/// egui's own fonts have no Hangul. Malgun Gothic ships with Windows, so it is
/// borrowed from the system rather than bundled; without it the panel still works,
/// with boxes where the Korean would be.
/// Room to breathe: bigger hit targets and wider gaps than egui's compact default.
pub fn install_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size.y = 23.0;
        s.spacing.slider_width = 180.0;
        s.spacing.combo_width = 72.0;
        for (style, size) in
            [(egui::TextStyle::Body, 14.0), (egui::TextStyle::Button, 14.0), (egui::TextStyle::Small, 12.0)]
        {
            if let Some(f) = s.text_styles.get_mut(&style) {
                f.size = size;
            }
        }
    });
}

pub fn install_fonts(ctx: &egui::Context) {
    let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\malgun.ttf") else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("malgun".into(), Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("malgun".into());
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

pub struct Panel {
    shared: Arc<Shared>,
    tx: Sender<Request>,
    tab: Group,
    /// The debug tab, which is not a group of cheats.
    debug: bool,
    /// The map & guide page: minimap, big map, compass, north, where to go.
    map: bool,
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
            shared,
            tx,
            tab: Group::ALL.into_iter().find(|g| Some(g.id()) == tab).unwrap_or(Group::Survival),
            debug: tab == Some("debug"),
            map: tab == Some("map") || tab == Some("guide"),
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
                if self.debug {
                    "debug"
                } else if self.map {
                    "map"
                } else {
                    self.tab.id()
                }
                .to_string(),
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
        self.reply = Some((true, format!("지난번에 켜 둔 치트 {}개를 다시 켰습니다", resume.len()), Instant::now()));
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
            ui.label(RichText::new("F8").color(DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(" × ").on_hover_text("닫기 — 원래 값으로 되돌리고 종료").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(" — ").on_hover_text("숨기기 (F8로 다시 열기)").clicked() {
                    hotkey::hide(&self.shared);
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
                line(ui, "게임", RichText::new(format!("● 연결됨\nv{version} · PID {pid}")).color(OK))
            }
            _ if launching => line(ui, "게임", RichText::new("실행하는 중 — 켜지면 자동으로 연결").color(WAIT)),
            Some(Err(e)) => line(ui, "게임", RichText::new(format!("○ 연결 안 됨 — {e}")).color(DIM)),
            None => line(ui, "게임", RichText::new("시작하는 중…").color(DIM)),
        }
        match snap.map(|s| &s.gate) {
            Some(Ok(())) => line(ui, "주인공 게이트", RichText::new("● 열림 — 조작 중").color(OK)),
            Some(Err(e)) if snap.is_some_and(|s| s.game.is_ok()) => {
                line(ui, "주인공 게이트", RichText::new(format!("○ 닫힘 — {e}")).color(WAIT))
            }
            _ => line(ui, "주인공 게이트", RichText::new("○ 닫힘").color(DIM)),
        }
    }

    /// The sidebar's pages: each cheat group (with how many are on), then the map &
    /// guide, then debugging.
    fn nav(&mut self, ui: &mut egui::Ui) {
        let item = |ui: &mut egui::Ui, on: bool, text: String| {
            let label = RichText::new(text).size(14.5).color(if on { Color32::WHITE } else { Color32::from_gray(190) });
            let button = egui::Button::new(label)
                .fill(if on { Color32::from_rgb(0x2E, 0x3B, 0x4E) } else { Color32::TRANSPARENT })
                .stroke(egui::Stroke::NONE)
                .corner_radius(6.0)
                .min_size(egui::vec2(ui.available_width(), 30.0));
            ui.add(button).clicked()
        };
        ui.label(RichText::new("치트").color(DIM).small());
        for g in Group::ALL {
            let on = CHEATS.iter().filter(|c| c.group == g && self.on.get(c.id).copied().unwrap_or(false)).count();
            let text = if on > 0 { format!("{}  ({on})", g.label()) } else { g.label().to_string() };
            if item(ui, !self.debug && !self.map && self.tab == g, text) {
                self.tab = g;
                (self.debug, self.map) = (false, false);
            }
        }
        ui.add_space(6.0);
        ui.label(RichText::new("도구").color(DIM).small());
        if item(ui, self.map, "지도 · 안내".to_string()) {
            (self.debug, self.map) = (false, true);
        }
        if item(ui, self.debug, "디버그".to_string()) {
            (self.debug, self.map) = (true, false);
        }
    }

    /// The page chosen in the sidebar: cards on an auto-fit grid (tw.rs) — side by
    /// side while each can be `CARD_MIN` wide, stacked when not.
    fn page(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        match self.tab {
            _ if self.map => self.map_tab(t, snap),
            _ if self.debug => block(t, |ui| self.debug_tab(ui, snap)),
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

    /// Saved positions: save where the hero stands, go back to it.
    fn positions(&self, t: &mut Tui, snap: Option<&Snapshot>) {
        card(t, "위치 저장 · 이동", |t| {
            let world = snap.and_then(|s| s.world.clone());
            for i in 0..crate::engine::SLOTS {
                let slot = snap.and_then(|s| s.slots[i].clone());
                let here = slot.as_ref().is_some_and(|(w, _)| Some(w) == world.as_ref());
                field(t, format!("슬롯 {}", i + 1), |t| {
                    if w(t, |ui| ui.button("저장")).clicked() {
                        let _ = self.tx.send(Request::SavePosition(i));
                    }
                    if w(t, |ui| ui.add_enabled(here, egui::Button::new("이동"))).clicked() {
                        let _ = self.tx.send(Request::LoadPosition(i));
                    }
                    let place = match &slot {
                        Some((_, p)) if here => format!("{:.0}, {:.0}, {:.0}", p[0], p[1], p[2]),
                        Some((w, _)) => format!("다른 지역 ({w})"),
                        None => "비어 있음".into(),
                    };
                    note(t, place);
                });
            }
            note(t, "저장한 지역 안에서만 이동합니다");
        });
    }

    /// Every cheat that is on, across the groups, with its value.
    fn summary(&self, t: &mut Tui, snap: Option<&Snapshot>) {
        let on: Vec<&Cheat> = CHEATS.iter().filter(|c| self.on.get(c.id).copied().unwrap_or(false)).collect();
        card(t, &format!("켜진 치트 ({})", on.len()), |t| {
            if on.is_empty() {
                text(t, RichText::new("켜진 치트가 없습니다").color(DIM));
            }
            for c in on {
                field(t, c.label, |t| {
                    let value = match c.kind {
                        Kind::Slider { .. } => {
                            let v = self.value.get(c.id).copied().unwrap_or(0.0);
                            match chosen(c).and_then(|a| snap.and_then(|s| s.value(a))) {
                                Some(n) => format!("{v:.2} (게임 {n:.2})"),
                                None => format!("{v:.2}"),
                            }
                        }
                        _ => "켜짐".to_string(),
                    };
                    w(t, |ui| ui.label(RichText::new(value).color(OK)));
                    w(t, |ui| ui.label(RichText::new(c.group.label()).color(DIM).small()));
                });
            }
        });
    }

    /// The map & guide page. Left column: the minimap, the land, the big map, what is
    /// shown, this area. Right: compass and north, the guide and where to go, every
    /// key, the game's menus.
    fn map_tab(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let shared = self.shared.clone();
        let mut guard = shared.map.lock().unwrap();
        let before = guard.clone();
        t.style(tw::full(tw::cards(tw::CARD_MIN))).add(|t| {
            t.style(tw::col(tw::GAP)).add(|t| self.map_column(t, &mut guard, snap));
            t.style(tw::col(tw::GAP)).add(|t| self.guide_column(t, &mut guard, snap));
        });
        if *guard != before {
            guard.dirty = true;
        }
    }

    fn map_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let world = snap.and_then(|s| s.world.clone());
        let near = snap.map(|s| s.things.clone()).unwrap_or_default();
        card(t, "미니맵", |t| {
            field(t, "표시 방식", |t| {
                for d in crate::minimap::Display::ALL {
                    w(t, |ui| ui.selectable_value(&mut state.display, d, d.label()));
                }
            });
            field(t, "위쪽", |t| {
                w(t, |ui| ui.selectable_value(&mut state.heading_up, false, "북쪽(N)"));
                w(t, |ui| ui.selectable_value(&mut state.heading_up, true, "카메라 방향"));
            });
            field(t, "반경", |t| tw::slider(t, &mut state.radius_m, 20.0..=300.0, 10.0, " m"));
            field(t, "스타일", |t| {
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, true, "윤곽선"));
                w(t, |ui| ui.selectable_value(&mut state.mini_outline, false, "채움"));
            });
            field(t, "아이콘 크기", |t| tw::slider(t, &mut state.icon_px, crate::minimap::ICON_PX, 1.0, " px"));
            switch(t, &mut state.hide_in_menus, "인벤토리·메뉴가 열리면 모든 오버레이 숨기기");
        });

        card(t, "지형", |t| {
            field(t, "지형 표시", |t| {
                for m in crate::minimap::ReliefMode::ALL {
                    w(t, |ui| ui.selectable_value(&mut state.relief, m, m.label()));
                }
            });
            field(t, "벽·바닥 윤곽", |t| w(t, |ui| toggle(ui, &mut state.terrain)));
            if state.terrain {
                choices(t, |t| {
                    for b in crate::raster::Band::ALL {
                        w(t, |ui| {
                            ui.horizontal(|ui| {
                                let (fill, edge) = b.colours();
                                let swatch = Color32::from_rgb(
                                    fill.0.max(edge.0 / 2),
                                    fill.1.max(edge.1 / 2),
                                    fill.2.max(edge.2 / 2),
                                );
                                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                ui.painter().rect_filled(r, 2.0, swatch);
                                ui.painter().rect_stroke(
                                    r,
                                    2.0,
                                    egui::Stroke::new(1.0, Color32::from_rgb(edge.0, edge.1, edge.2)),
                                    egui::StrokeKind::Inside,
                                );
                                ui.label(RichText::new(b.label()).small());
                            });
                        });
                    }
                });
            }
        });

        card(t, "큰 지도", |t| {
            field(t, "반경", |t| tw::slider(t, &mut state.big_radius_m, 50.0..=1000.0, 25.0, " m"));
            field(t, "스타일", |t| {
                w(t, |ui| ui.selectable_value(&mut state.big_outline, true, "윤곽선"));
                w(t, |ui| ui.selectable_value(&mut state.big_outline, false, "채움"));
            });
            field(t, "불투명도", |t| tw::slider(t, &mut state.big_alpha, 20..=100, 1.0, " %"));
        });

        card(t, "표시할 것", |t| {
            for (i, k) in ThingKind::ALL.into_iter().enumerate() {
                let [r, g, b] = k.rgb();
                let colour = Color32::from_rgb(r, g, b);
                let total = near.iter().filter(|x| x.kind() == k).count();
                let subs = Sub::ALL.iter().filter(|x| x.kind() == k).count();
                let mut on = state.layers & k.bit() != 0;
                t.style(tw::row(8.0)).add(|t| {
                    if w(t, |ui| toggle(ui, &mut on)).changed() {
                        state.layers ^= k.bit();
                    }
                    w(t, |ui| {
                        let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            dot.center(),
                            6.0,
                            if on { colour } else { colour.gamma_multiply(0.3) },
                        );
                    });
                    t.style(tw::grow(tw::row(6.0))).add(|t| {
                        w(t, |ui| {
                            ui.label(RichText::new(k.label()).strong().color(if on { Color32::WHITE } else { DIM }))
                        });
                        w(t, |ui| ui.label(RichText::new(format!("({total})")).color(DIM)));
                    });
                    if subs > 1 {
                        let hidden = Sub::ALL.iter().filter(|x| x.kind() == k && state.hidden.contains(x)).count();
                        let label = match (self.unfolded[i], hidden) {
                            (true, _) => "세부 ▲".to_string(),
                            (false, 0) => "세부 ▼".to_string(),
                            (false, h) => format!("세부 ▼ ({h}개 숨김)"),
                        };
                        if w(t, |ui| ui.add(egui::Button::new(RichText::new(label).small()).frame(false))).clicked() {
                            self.unfolded[i] = !self.unfolded[i];
                        }
                    }
                });
                if self.unfolded[i] && subs > 1 {
                    choices(t, |t| {
                        for sub in Sub::ALL.into_iter().filter(|x| x.kind() == k) {
                            let n = near.iter().filter(|x| x.sub == sub).count();
                            let shown = !state.hidden.contains(&sub);
                            if w(t, |ui| ui.add_enabled(on, chip(&format!("{} ({n})", sub.label()), shown, colour)))
                                .clicked()
                            {
                                if shown {
                                    state.hidden.insert(sub);
                                } else {
                                    state.hidden.remove(&sub);
                                }
                            }
                        }
                    });
                }
            }
        });

        card(t, "지나온 길 · 마커", |t| match &world {
            Some(wd) => {
                let trail = state.trails.get(wd).map_or(0, |x| x.iter().flatten().count());
                let markers = state.markers.get(wd).map_or(0, Vec::len);
                field(t, "이 지역", |t| text(t, format!("길 {trail}점 · 마커 {markers}개")));
                choices(t, |t| {
                    if w(t, |ui| ui.button("경로 지우기")).clicked() {
                        state.clear_trail(wd);
                    }
                    if w(t, |ui| ui.button("마커 지우기")).clicked() {
                        state.clear_markers(wd);
                    }
                });
            }
            _ => text(t, RichText::new("주인공을 조작할 수 있을 때 표시됩니다").color(DIM)),
        });
    }

    /// The quest journal: which quest the guide and the tracker follow.
    fn quests_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        use crate::quests::{Kind, Status};
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        card(t, "퀘스트", |t| {
            switch(t, &mut state.tracker, "퀘스트 추적기 — 화면 오른쪽 가운데");
            if journal.is_empty() {
                note(t, "퀘스트를 읽는 중입니다 — 게임을 불러오고 몇 초 뒤에 나옵니다");
                return;
            }
            let followed = crate::quests::followed(&journal, state.quest.as_deref()).map(|q| q.key.clone());
            let mut pick: Option<Option<String>> = None;
            let auto = match journal.iter().find(|q| Some(&q.key) == followed.as_ref()) {
                Some(q) if state.quest.is_none() => format!("메인 스토리 자동 — {}", q.name),
                _ => "메인 스토리 자동".to_string(),
            };
            if tw::pick(t, state.quest.is_none(), auto) {
                pick = Some(None);
            }
            for q in journal.iter().filter(|q| q.active()) {
                let tag = match q.kind {
                    Kind::Main(n) => format!("메인 {n}"),
                    Kind::GoodDeed => "선행".into(),
                };
                let mut label = format!("[{tag}] {}", q.name);
                if let Some((got, all)) = q.progress.filter(|(_, all)| *all > 0) {
                    label += &format!(" · 단서 {got}/{all}");
                }
                let on = state.quest.as_deref() == Some(q.key.as_str());
                if tw::pick(t, on, label) {
                    pick = Some(Some(q.key.clone()));
                }
            }
            let done = journal.iter().filter(|q| q.status == Status::Completed).count();
            let failed = journal.iter().filter(|q| q.status == Status::Failed).count();
            note(t, format!("완료 {done}개 · 실패 {failed}개"));
            note(t, "영어로 나오는 선행 이름은 게임이 아직 보여 주지 않은 것 — 데이터패드의 탐험 → 선행에서 보면 한국어로 바뀝니다");
            if let Some(p) = pick {
                state.quest = p;
                // Guide anew, to the newly followed quest.
                state.target = None;
                state.chosen = false;
                state.guide_auto = true;
                state.route = true;
                state.dirty = true;
            }
        });
    }

    fn guide_column(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let dist = |g: &crate::goals::Goal| {
            here.map_or(f32::MAX, |h| ((g.at[0] - h[0]).powi(2) + (g.at[1] - h[1]).powi(2)).sqrt() / 100.0)
        };

        card(t, "나침반 · 방향", |t| {
            field(t, format!("나침반 (F{})", state.compass_key), |t| {
                w(t, |ui| toggle(ui, &mut state.compass));
                w(t, |ui| ui.label(RichText::new("화면 위 가운데").color(DIM).small()));
            });
        });

        card(t, "안내", |t| {
            switch(t, &mut state.guide_auto, "자동 — 고른 곳이 없으면 따라가는 퀘스트의 가장 가까운 목표로");
            switch(
                t,
                &mut state.route,
                "실제 이동 경로 (A*) — 지형·물·벽을 돌아가는 길, 나침반이 다음 꺾이는 곳을 가리킴",
            );
            choices(t, |t| {
                for tier in crate::goals::Tier::ALL {
                    let [r, g, b] = tier.rgb();
                    let n = goals.iter().filter(|x| x.tier == tier).count();
                    let on = state.goal_tiers & (1 << tier as u8) != 0;
                    if w(t, |ui| ui.add(chip(&format!("{} ({n})", tier.label()), on, Color32::from_rgb(r, g, b))))
                        .clicked()
                    {
                        state.goal_tiers ^= 1 << tier as u8;
                    }
                }
            });
            match state.target.and_then(|id| goals.iter().find(|g| g.id == id)) {
                Some(g) => {
                    let [r, gg, b] = g.tier.rgb();
                    t.style(tw::row(8.0)).add(|t| {
                        w(t, |ui| ui.label(RichText::new("◆").color(Color32::from_rgb(r, gg, b))));
                        text(t, RichText::new(&g.label).strong());
                        w(t, |ui| ui.label(RichText::new(crate::raster::distance(dist(g))).color(DIM)));
                        if w(t, |ui| ui.button("안내 끄기")).clicked() {
                            state.target = None;
                            state.guide_auto = false;
                        }
                    });
                    note(t, g.detail.clone());
                    if state.route && *self.shared.route_uncertain.lock().unwrap() {
                        text(
                            t,
                            RichText::new(
                                "⚠ 걸어서 닿는 길이 없습니다 — 닫힌 문·퍼즐·열쇠 너머일 수 있음. 닿는 곳까지 안내하고 나머지는 노란 점선",
                            )
                            .color(WAIT)
                            .small(),
                        );
                    }
                }
                None => text(t, RichText::new("안내 중인 곳이 없습니다").color(DIM)),
            }
        });

        self.quests_card(t, state, snap);

        let mut list: Vec<&crate::goals::Goal> =
            goals.iter().filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0).collect();
        list.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
        card(t, &format!("갈 곳 ({})", list.len()), |t| {
            block(t, |ui| {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for g in list {
                        let [r, gg, b] = g.tier.rgb();
                        let chosen = state.target == Some(g.id);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("◆").color(Color32::from_rgb(r, gg, b)));
                            let label = format!("{}  ({})", g.label, crate::raster::distance(dist(g)));
                            let button = egui::Button::selectable(chosen, label).truncate();
                            if ui.add(button).on_hover_text(&g.detail).clicked() {
                                state.target = Some(g.id);
                                state.chosen = true;
                            }
                        });
                    }
                });
            });
        });

        card(t, "단축키", |t| {
            for (label, id) in [
                ("지도 표시 방식 전환", "toggle_key"),
                ("마커 찍기/지우기", "marker_key"),
                ("나침반 표시/숨김", "compass_key"),
                ("다음 목표로 안내", "cycle_key"),
            ] {
                let mine = match id {
                    "toggle_key" => state.toggle_key,
                    "marker_key" => state.marker_key,
                    "compass_key" => state.compass_key,
                    _ => state.cycle_key,
                };
                let taken = others(state, mine);
                let key = match id {
                    "toggle_key" => &mut state.toggle_key,
                    "marker_key" => &mut state.marker_key,
                    "compass_key" => &mut state.compass_key,
                    _ => &mut state.cycle_key,
                };
                field(t, label, |t| w(t, |ui| key_picker(ui, id, key, &taken)));
            }
        });
    }

    /// For each cheat that is on: every attribute it writes — what it was before,
    /// what is being written, what the game holds now. A row in red is a cheat the
    /// game is overriding. Then a place to record what each cheat did in play.
    fn debug_tab(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let fmt = |v: Option<f32>| v.map_or("—".to_string(), |v| format!("{v:.2}"));
        ui.label(RichText::new("켜진 치트").strong());
        ui.label(RichText::new("원래 값 → 넣는 값, 그리고 지금 게임에 들어 있는 값").color(DIM).small());
        let active = snap.map(|s| s.active.clone()).unwrap_or_default();
        if active.is_empty() {
            ui.label(RichText::new("켜진 치트가 없습니다").color(DIM));
        } else if let Some(snap) = snap {
            egui::ScrollArea::horizontal().id_salt("debug-scroll").show(ui, |ui| {
                egui::Grid::new("debug").num_columns(4).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for a in &active {
                        let Some(c) = cheats::find(a.cheat) else { continue };
                        for e in c.effects() {
                            let (target, want) = match *e {
                                Effect::Fixed(t, v) => (t, Some(v)),
                                Effect::Chosen(t) => (t, Some(a.value)),
                                Effect::Fill(t, max) => (t, snap.value(max)),
                                // Past the hero: many targets, not one value to compare.
                                Effect::EnemyTime
                                | Effect::EnemyFrail
                                | Effect::Stock(_)
                                | Effect::WeaponXp
                                | Effect::Ghost
                                | Effect::Untouchable => {
                                    ui.label(c.label);
                                    ui.label(RichText::new("적·인벤토리 대상").color(DIM).small());
                                    ui.label("");
                                    ui.label(RichText::new("매 틱 적용").color(OK));
                                    ui.end_row();
                                    continue;
                                }
                            };
                            let now = snap.value(target);
                            let was = snap.originals.iter().find(|(x, _)| *x == target).map(|(_, (_, c))| *c);
                            ui.label(c.label);
                            ui.label(RichText::new(target.name).color(DIM).small());
                            ui.label(format!("{} → {}", fmt(was), fmt(want)));
                            match (want, now) {
                                (Some(w), Some(n)) if (w - n).abs() <= 0.01 + w.abs() * 0.01 => {
                                    ui.label(RichText::new(format!("적용 중 {n:.2}")).color(OK))
                                }
                                (Some(_), Some(n)) => {
                                    ui.label(RichText::new(format!("게임이 바꿈 {n:.2}")).color(BAD)).on_hover_text(
                                        "넣은 값과 다릅니다. 게임이 매 순간 다시 계산하는 값일 수 있습니다.",
                                    )
                                }
                                _ => ui.label(RichText::new("읽을 수 없음").color(BAD)),
                            };
                            ui.end_row();
                        }
                    }
                });
            });
        }

        ui.separator();
        ui.label(RichText::new("테스트 결과 기록").strong());
        ui.label(RichText::new("게임에서 해 보고 눌러 주세요. verify.txt에 저장됩니다.").color(DIM).small());
        let mut changed = false;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt("marks-scroll").show(ui, |ui| {
                egui::Grid::new("marks").num_columns(3).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for c in CHEATS {
                        ui.label(c.label);
                        let mark = self.marks.get(c.id).copied();
                        ui.horizontal(|ui| {
                            if ui.selectable_label(mark == Some(true), "됨 ✓").clicked() {
                                self.marks.insert(c.id.to_string(), true);
                                changed = true;
                            }
                            if ui.selectable_label(mark == Some(false), "안 됨 ✗").clicked() {
                                self.marks.insert(c.id.to_string(), false);
                                changed = true;
                            }
                        });
                        ui.label(
                            RichText::new(if c.verified { "확인됨" } else { "미검증" })
                                .color(if c.verified { OK } else { WAIT })
                                .small(),
                        );
                        ui.end_row();
                    }
                });
            });
        });
        if changed {
            if let Err(e) = verify::save(&self.marks) {
                self.reply = Some((false, e, Instant::now()));
            }
        }

        ui.separator();
        if ui.button("로그·기록 폴더 열기").on_hover_text("hiumod.log, verify.txt, originals.txt").clicked() {
            if let Some(dir) = verify::path().parent() {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::process::Command::new("explorer.exe")
                    .arg(dir)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
        }
    }

    fn badge(ui: &mut egui::Ui, c: &Cheat) {
        if !c.verified {
            ui.label(RichText::new("미검증").color(WAIT).small())
                .on_hover_text("아직 게임에서 확인하지 않은 치트입니다. 동작하는지 알려주세요.");
        }
    }

    /// A group's cheats as form rows: the name on the left, wrapping if it must; the
    /// switch, slider (track and value box measured apart), button and badge on the
    /// right, wrapping onto a second line rather than overflowing.
    fn held(&mut self, t: &mut Tui, group: Group, snap: Option<&Snapshot>) {
        let mut changed = false;
        for c in CHEATS.iter().filter(|c| c.group == group) {
            let mut on = self.on.get(c.id).copied().unwrap_or(false);
            match c.kind {
                Kind::Toggle(_) => field(t, c.label, |t| {
                    changed |= w(t, |ui| toggle(ui, &mut on)).changed();
                    w(t, |ui| Self::badge(ui, c));
                }),
                Kind::Slider { min, max, .. } => field(t, c.label, |t| {
                    changed |= w(t, |ui| toggle(ui, &mut on)).changed();
                    let step = if max - min > 50.0 { 10.0 } else { 0.05 };
                    let v = self.value.entry(c.id).or_insert(min);
                    let mut slider = tw::slider(t, v, min..=max, step, "");
                    if let Some(now) = chosen(c).and_then(|a| snap.and_then(|s| s.value(a))) {
                        slider = slider.on_hover_text(format!("게임의 현재 값: {now:.2}"));
                    }
                    // Live while dragging, at most every 150 ms — and always on release.
                    let due = self.slid.get(c.id).is_none_or(|x| x.elapsed() >= Duration::from_millis(150));
                    if on && ((slider.changed() && due) || slider.drag_stopped()) {
                        self.slid.insert(c.id, Instant::now());
                        changed = true;
                    }
                    w(t, |ui| Self::badge(ui, c));
                }),
                Kind::SetStock { max, default, .. } => {
                    // Written once, on the button — not held.
                    let tx = self.tx.clone();
                    field(t, c.label, |t| {
                        let v = self.value.entry(c.id).or_insert(default);
                        tw::slider(t, v, 1.0..=max, 1.0, "");
                        let v = *v;
                        if w(t, |ui| ui.button("적용")).clicked() {
                            let _ = tx.send(Request::Set(c.id, v));
                        }
                        w(t, |ui| Self::badge(ui, c));
                    });
                    continue;
                }
                Kind::Set { .. } => continue,
            }
            self.on.insert(c.id, on);
        }
        if changed {
            self.send_active();
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let pending = snap.map_or(0, |s| s.pending);
        ui.horizontal(|ui| {
            let restore = ui.add_enabled(pending > 0, egui::Button::new(format!("모두 원래대로 ({pending})")));
            if restore.clicked() {
                let _ = self.tx.send(Request::Restore);
                self.on.clear();
                self.wanted.clear();
                self.resume = None;
                self.sent = Some(Instant::now());
            }
            ui.checkbox(&mut self.keep, "다음에도 켜 둔 치트 유지")
                .on_hover_text("다음 실행 때, 주인공을 조작할 수 있게 되면 지금 켜 둔 치트를 다시 켭니다.");
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
                RichText::new("주인공을 조작하는 동안만 값을 씁니다 · 업적은 차단되지 않습니다").color(DIM).small(),
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
        if self.debug {
            DEBUG_PAGE
        } else {
            tw::cards_width(2)
        }
    }
}

impl eframe::App for Panel {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
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
                ui.painter().vline(x, top..=ui.min_rect().bottom(), stroke);
            })
            .response
            .rect;
        self.fit(ui, width, used.height().min(max_height));
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

/// A function key, F1–F12 — not F8 (the panel's) and none the other pickers hold.
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
