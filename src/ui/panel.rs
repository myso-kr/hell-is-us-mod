//! What the panel draws, and the requests its controls send. Every tab and row is
//! read off the cheat table; the panel reads the worker's last snapshot and never
//! touches the game itself.

use super::{hotkey, Request, Shared};
use crate::actors::{Kind as ThingKind, Sub};
use crate::cheats::{self, Active, Cheat, Effect, Group, Kind, CHEATS};
use crate::engine::Snapshot;
use crate::settings::{self, Settings};
use crate::verify;
use eframe::egui::{self, Color32, RichText};
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

pub const WIDTH: f32 = 470.0;

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
    /// The minimap tab.
    map: bool,
    /// The guide tab: compass and where to go.
    guide: bool,
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
    /// The height last asked of the window, so it is asked once per change.
    height: f32,
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
            map: tab == Some("map"),
            guide: tab == Some("guide"),
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
        }
    }

    fn active(&self) -> Vec<Active> {
        CHEATS
            .iter()
            .filter(|c| !matches!(c.kind, Kind::Set { .. }) && self.on.get(c.id).copied().unwrap_or(false))
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
                } else if self.guide {
                    "guide"
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
                if ui.button(" ✕ ").on_hover_text("닫기 — 원래 값으로 되돌리고 종료").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(" — ").on_hover_text("숨기기 (F8로 다시 열기)").clicked() {
                    hotkey::hide(&self.shared);
                }
            });
        });
        ui.separator();
    }

    fn status(&self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let launching = self.shared.launched.load(Ordering::SeqCst);
        egui::Grid::new("status").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            ui.label("게임");
            match snap.map(|s| &s.game) {
                Some(Ok((pid, version))) => {
                    ui.label(RichText::new(format!("연결됨 · v{version} · PID {pid}")).color(OK))
                }
                _ if launching => ui.label(RichText::new("실행하는 중 — 켜지면 자동으로 연결합니다").color(WAIT)),
                Some(Err(e)) => ui.label(RichText::new(format!("연결 안 됨 — {e}")).color(DIM)),
                None => ui.label(RichText::new("시작하는 중…").color(DIM)),
            };
            ui.end_row();
            ui.label("주인공 게이트");
            match snap.map(|s| &s.gate) {
                Some(Ok(())) => ui.label(RichText::new("열림 — 주인공 조작 중").color(OK)),
                Some(Err(e)) if snap.is_some_and(|s| s.game.is_ok()) => {
                    ui.label(RichText::new(format!("닫힘 — {e}")).color(WAIT))
                }
                _ => ui.label(RichText::new("닫힘").color(DIM)),
            };
            ui.end_row();
        });
    }

    fn tabs(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        ui.horizontal(|ui| {
            for g in Group::ALL {
                let on = CHEATS.iter().filter(|c| c.group == g && self.on.get(c.id).copied().unwrap_or(false)).count();
                let text = if on > 0 { format!("{} ({on})", g.label()) } else { g.label().to_string() };
                if ui.selectable_label(!self.debug && !self.map && !self.guide && self.tab == g, text).clicked() {
                    self.tab = g;
                    (self.debug, self.map, self.guide) = (false, false, false);
                }
            }
            ui.separator();
            if ui.selectable_label(self.guide, "안내").clicked() {
                (self.debug, self.map, self.guide) = (false, false, true);
            }
            if ui.selectable_label(self.map, "지도").clicked() {
                (self.debug, self.map, self.guide) = (false, true, false);
            }
            if ui.selectable_label(self.debug, "디버그").clicked() {
                (self.debug, self.map, self.guide) = (true, false, false);
            }
        });
        ui.add_space(4.0);
        match self.tab {
            _ if self.guide => self.guide_tab(ui, snap),
            _ if self.map => self.map_tab(ui, snap),
            _ if self.debug => self.debug_tab(ui, snap),
            g => self.held(ui, g, snap),
        }
    }

    /// The minimap's settings, and what it knows about where the hero is — in four
    /// boxes: the map, its keys, what it shows, and this area.
    fn map_tab(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let world = snap.and_then(|s| s.world.clone());
        let near = snap.map(|s| s.things.clone()).unwrap_or_default();
        let shared = self.shared.clone();
        let mut state = shared.map.lock().unwrap();
        let before = state.clone();

        section(ui, "미니맵", |ui| {
            egui::Grid::new("map-basic").num_columns(2).spacing([14.0, 7.0]).show(ui, |ui| {
                ui.label(format!("표시 (F{})", state.toggle_key));
                toggle(ui, &mut state.show);
                ui.end_row();
                ui.label("진행 방향을 위로");
                ui.horizontal(|ui| {
                    toggle(ui, &mut state.heading_up);
                    ui.label(
                        RichText::new(if state.heading_up { "카메라 방향이 위" } else { "북쪽(N)이 위" })
                            .color(DIM)
                            .small(),
                    );
                });
                ui.end_row();
                ui.label("반경");
                ui.add(egui::Slider::new(&mut state.radius_m, 20.0..=300.0).step_by(10.0).suffix(" m"));
                ui.end_row();
                ui.label("아이콘 크기");
                ui.add(egui::Slider::new(&mut state.icon_px, crate::minimap::ICON_PX).suffix(" px"));
                ui.end_row();
                ui.label("벽·바닥 윤곽");
                ui.horizontal(|ui| {
                    toggle(ui, &mut state.terrain);
                    let n = snap.map_or(0, |s| s.footprints.len());
                    ui.label(RichText::new(format!("지금 불러온 구조물 ({n})")).color(DIM).small());
                });
                ui.end_row();
                if state.terrain {
                    ui.label("");
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for b in crate::raster::Band::ALL {
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
                        }
                    });
                    ui.end_row();
                }
            });
        });

        let (cursor, paused) = *self.shared.menu.lock().unwrap();
        section(ui, "게임 메뉴", |ui| {
            ui.horizontal(|ui| {
                toggle(ui, &mut state.hide_in_menus);
                ui.label("인벤토리·메뉴가 열리면 모든 오버레이 숨기기");
            });
            let sig =
                |on: bool| if on { RichText::new("켜짐").color(WAIT) } else { RichText::new("꺼짐").color(DIM) };
            ui.horizontal(|ui| {
                ui.label(RichText::new("감지 — 게임 커서").color(DIM).small());
                ui.label(sig(cursor).small());
                ui.label(RichText::new("· 일시정지").color(DIM).small());
                ui.label(sig(paused).small());
            });
        });

        section(ui, "단축키", |ui| {
            egui::Grid::new("map-keys").num_columns(2).spacing([14.0, 7.0]).show(ui, |ui| {
                ui.label("미니맵 표시/숨김");
                let taken = others(&state, state.toggle_key);
                key_picker(ui, "toggle_key", &mut state.toggle_key, &taken);
                ui.end_row();
                ui.label("마커 찍기/지우기");
                let taken = others(&state, state.marker_key);
                key_picker(ui, "marker_key", &mut state.marker_key, &taken);
                ui.end_row();
                ui.label("큰 지도 (화면 가운데)");
                let taken = others(&state, state.big_key);
                key_picker(ui, "big_key", &mut state.big_key, &taken);
                ui.end_row();
                ui.label("큰 지도 반경");
                ui.add(egui::Slider::new(&mut state.big_radius_m, 50.0..=1000.0).step_by(25.0).suffix(" m"));
                ui.end_row();
            });
            ui.label(RichText::new("마커 옆(5 m 안)에서 마커 키를 누르면 그 마커를 지웁니다").color(DIM).small());
        });

        section(ui, "표시할 것", |ui| {
            ui.label(RichText::new("괄호 안 숫자는 지금 불러온 범위에 남아 있는 개수입니다").color(DIM).small());
            ui.add_space(2.0);
            for (i, k) in ThingKind::ALL.into_iter().enumerate() {
                let [r, g, b] = k.rgb();
                let colour = Color32::from_rgb(r, g, b);
                let total = near.iter().filter(|t| t.kind() == k).count();
                let mut on = state.layers & k.bit() != 0;
                ui.horizontal(|ui| {
                    if toggle(ui, &mut on).changed() {
                        state.layers ^= k.bit();
                    }
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 6.0, if on { colour } else { colour.gamma_multiply(0.3) });
                    let name = RichText::new(k.label()).strong().color(if on { Color32::WHITE } else { DIM });
                    ui.label(name);
                    ui.label(RichText::new(format!("({total})")).color(DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let subs = Sub::ALL.iter().filter(|x| x.kind() == k).count();
                        if subs > 1 {
                            let hidden = Sub::ALL.iter().filter(|x| x.kind() == k && state.hidden.contains(x)).count();
                            let text = match (self.unfolded[i], hidden) {
                                (true, _) => "세부 ▲".to_string(),
                                (false, 0) => "세부 ▼".to_string(),
                                (false, h) => format!("세부 ▼ ({h}개 숨김)"),
                            };
                            if ui.add(egui::Button::new(RichText::new(text).small()).frame(false)).clicked() {
                                self.unfolded[i] = !self.unfolded[i];
                            }
                        }
                    });
                });
                if self.unfolded[i] && Sub::ALL.iter().filter(|x| x.kind() == k).count() > 1 {
                    ui.indent(("subs", i), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                            for sub in Sub::ALL.into_iter().filter(|x| x.kind() == k) {
                                let n = near.iter().filter(|t| t.sub == sub).count();
                                let shown = !state.hidden.contains(&sub);
                                if ui.add_enabled(on, chip(&format!("{} ({n})", sub.label()), shown, colour)).clicked()
                                {
                                    if shown {
                                        state.hidden.insert(sub);
                                    } else {
                                        state.hidden.remove(&sub);
                                    }
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });

        section(ui, "이 지역", |ui| match (&world, snap.and_then(|s| s.pose)) {
            (Some(w), Some((p, yaw))) => {
                let trail = state.trails.get(w).map_or(0, |t| t.iter().flatten().count());
                let markers = state.markers.get(w).map_or(0, Vec::len);
                egui::Grid::new("map-area").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                    ui.label("지역");
                    ui.label(RichText::new(w).color(DIM));
                    ui.end_row();
                    ui.label("위치");
                    ui.label(
                        RichText::new(format!("{:.0}, {:.0}, {:.0} · 방향 {yaw:.0}°", p[0], p[1], p[2])).color(DIM),
                    );
                    ui.end_row();
                    ui.label("기록");
                    ui.label(format!("지나온 길 ({trail}점) · 마커 ({markers}개)"));
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.button("경로 지우기").clicked() {
                        state.clear_trail(w);
                    }
                    if ui.button("마커 지우기").clicked() {
                        state.clear_markers(w);
                    }
                });
            }
            _ => {
                ui.label(RichText::new("주인공을 조작할 수 있을 때 표시됩니다").color(DIM));
            }
        });

        let changed = (state.show, state.heading_up, state.radius_m, state.toggle_key, state.marker_key)
            != (before.show, before.heading_up, before.radius_m, before.toggle_key, before.marker_key)
            || (state.layers, state.icon_px, state.terrain) != (before.layers, before.icon_px, before.terrain)
            || state.hidden != before.hidden
            || (state.big_key, state.big_radius_m, state.hide_in_menus)
                != (before.big_key, before.big_radius_m, before.hide_in_menus);
        if changed {
            state.dirty = true;
        }
    }

    /// The compass, and the guide: what to point at, chosen here or by the cycle key.
    fn guide_tab(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let dist = |g: &crate::goals::Goal| {
            here.map_or(f32::MAX, |h| ((g.at[0] - h[0]).powi(2) + (g.at[1] - h[1]).powi(2)).sqrt() / 100.0)
        };
        let shared = self.shared.clone();
        let mut state = shared.map.lock().unwrap();
        let before = state.clone();

        section(ui, "나침반", |ui| {
            egui::Grid::new("compass").num_columns(2).spacing([14.0, 7.0]).show(ui, |ui| {
                ui.label(format!("화면 위 가운데 (F{})", state.compass_key));
                toggle(ui, &mut state.compass);
                ui.end_row();
                ui.label("나침반 표시/숨김");
                let taken = others(&state, state.compass_key);
                key_picker(ui, "compass_key", &mut state.compass_key, &taken);
                ui.end_row();
                ui.label("다음 목표로 안내");
                let taken = others(&state, state.cycle_key);
                key_picker(ui, "cycle_key", &mut state.cycle_key, &taken);
                ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.label("북쪽 보정");
                for (deg, name) in [(270.0, "기본 (−Y)"), (0.0, "+X"), (90.0, "+Y"), (180.0, "−X")] {
                    ui.selectable_value(&mut state.north_yaw, deg, name);
                }
            });
            ui.label(
                RichText::new("게임 나침반 아이템의 북쪽과 다르면 바꾸세요 (미니맵·큰 지도·나침반 모두에 적용)")
                    .color(DIM)
                    .small(),
            );
        });

        section(ui, "안내", |ui| {
            ui.horizontal(|ui| {
                toggle(ui, &mut state.guide_auto);
                ui.label("자동 — 고른 곳이 없으면 가장 가까운 퀘스트 목표로");
            });
            ui.horizontal(|ui| {
                toggle(ui, &mut state.route);
                ui.label("실제 이동 경로 (A*) — 벽을 돌아가는 길을 그리고 나침반이 다음 꺾이는 곳을 가리킴");
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for t in crate::goals::Tier::ALL {
                    let [r, g, b] = t.rgb();
                    let n = goals.iter().filter(|x| x.tier == t).count();
                    let on = state.goal_tiers & (1 << t as u8) != 0;
                    if ui.add(chip(&format!("{} ({n})", t.label()), on, Color32::from_rgb(r, g, b))).clicked() {
                        state.goal_tiers ^= 1 << t as u8;
                    }
                }
            });
            match state.target.and_then(|t| goals.iter().find(|g| g.id == t)) {
                Some(g) => {
                    let [r, gg, b] = g.tier.rgb();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("◆").color(Color32::from_rgb(r, gg, b)));
                        ui.label(RichText::new(&g.label).strong());
                        ui.label(RichText::new(format!("({})", crate::raster::distance(dist(g)))).color(DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("안내 끄기").clicked() {
                                state.target = None;
                                state.guide_auto = false;
                            }
                        });
                    });
                    ui.label(RichText::new(&g.detail).color(DIM).small());
                    if state.route && *self.shared.route_uncertain.lock().unwrap() {
                        ui.label(
                            RichText::new(
                                "⚠ 들어가는 길을 찾지 못해 장애물을 넘는 추정 구간이 있습니다 (지도에 노란 점선)",
                            )
                            .color(WAIT)
                            .small(),
                        );
                    }
                }
                None => {
                    ui.label(RichText::new("안내 중인 곳이 없습니다").color(DIM));
                }
            }
        });

        let quests = snap.map(|s| s.quests.clone()).unwrap_or_default();
        section(ui, "진행 중인 조사", |ui| {
            if quests.is_empty() {
                ui.label(RichText::new("없음 (또는 아직 읽지 못함)").color(DIM));
            } else {
                ui.label(quests.join(", "));
            }
        });

        let mut list: Vec<&crate::goals::Goal> =
            goals.iter().filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0).collect();
        list.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
        section(ui, &format!("갈 곳 ({})", list.len()), |ui| {
            ui.label(
                RichText::new("아직 얻지 않은 사실·태그를 주는 곳입니다. 눌러서 안내를 시작합니다").color(DIM).small(),
            );
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for g in list {
                    let [r, gg, b] = g.tier.rgb();
                    let chosen = state.target == Some(g.id);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("◆").color(Color32::from_rgb(r, gg, b)));
                        let text = format!("{}  ({})", g.label, crate::raster::distance(dist(g)));
                        if ui.selectable_label(chosen, text).on_hover_text(&g.detail).clicked() {
                            state.target = Some(g.id);
                        }
                    });
                }
            });
        });

        let changed =
            (state.compass, state.compass_key, state.cycle_key, state.guide_auto, state.goal_tiers, state.route)
                != (
                    before.compass,
                    before.compass_key,
                    before.cycle_key,
                    before.guide_auto,
                    before.goal_tiers,
                    before.route,
                )
                || state.north_yaw != before.north_yaw;
        if changed {
            state.dirty = true;
        }
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
            egui::Grid::new("debug").num_columns(4).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                for a in &active {
                    let Some(c) = cheats::find(a.cheat) else { continue };
                    for e in c.effects() {
                        let (target, want) = match *e {
                            Effect::Fixed(t, v) => (t, Some(v)),
                            Effect::Chosen(t) => (t, Some(a.value)),
                            Effect::Fill(t, max) => (t, snap.value(max)),
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
                            (Some(_), Some(n)) => ui
                                .label(RichText::new(format!("게임이 바꿈 {n:.2}")).color(BAD))
                                .on_hover_text("넣은 값과 다릅니다. 게임이 매 순간 다시 계산하는 값일 수 있습니다."),
                            _ => ui.label(RichText::new("읽을 수 없음").color(BAD)),
                        };
                        ui.end_row();
                    }
                }
            });
        }

        ui.separator();
        ui.label(RichText::new("테스트 결과 기록").strong());
        ui.label(RichText::new("게임에서 해 보고 눌러 주세요. verify.txt에 저장됩니다.").color(DIM).small());
        let mut changed = false;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
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

    fn held(&mut self, ui: &mut egui::Ui, group: Group, snap: Option<&Snapshot>) {
        let mut changed = false;
        egui::Grid::new(group.label()).num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            for c in CHEATS.iter().filter(|c| c.group == group) {
                let mut on = self.on.get(c.id).copied().unwrap_or(false);
                match c.kind {
                    Kind::Toggle(_) => {
                        changed |= ui.checkbox(&mut on, c.label).changed();
                        ui.label("");
                    }
                    Kind::Slider { min, max, .. } => {
                        changed |= ui.checkbox(&mut on, c.label).changed();
                        let v = self.value.entry(c.id).or_insert(min);
                        let step = if max - min > 50.0 { 10.0 } else { 0.05 };
                        let mut slider = ui.add(egui::Slider::new(v, min..=max).max_decimals(2).step_by(step));
                        if let Some(now) = chosen(c).and_then(|a| snap.and_then(|s| s.value(a))) {
                            slider = slider.on_hover_text(format!("게임의 현재 값: {now:.2}"));
                        }
                        // Live while dragging, at most every 150 ms — and always on release.
                        let due = self.slid.get(c.id).is_none_or(|t| t.elapsed() >= Duration::from_millis(150));
                        if on && ((slider.changed() && due) || slider.drag_stopped()) {
                            self.slid.insert(c.id, Instant::now());
                            changed = true;
                        }
                    }
                    Kind::Set { .. } => continue,
                }
                Self::badge(ui, c);
                self.on.insert(c.id, on);
                ui.end_row();
            }
        });
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
            ui.label(RichText::new(n).color(if attached { WAIT } else { DIM }).small());
        }
        if let Some((ok, text, at)) = &self.reply {
            if at.elapsed() < Duration::from_secs(5) {
                ui.label(RichText::new(text).color(if *ok { OK } else { BAD }).small());
            }
        }
        ui.add_space(4.0);
        ui.label(RichText::new("주인공을 조작하는 동안만 값을 씁니다 · 업적은 차단되지 않습니다").color(DIM).small());
    }

    /// The window is exactly as tall as what is in it: nothing clipped, nothing empty.
    fn fit(&mut self, ui: &egui::Ui, height: f32) {
        let height = height.ceil();
        if (height - self.height).abs() > 1.0 {
            self.height = height;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(WIDTH, height)));
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

        let used = egui::Frame::central_panel(ui.style())
            .show(ui, |ui| {
                ui.set_width(WIDTH - 16.0);
                self.title_bar(ui);
                self.status(ui, snap.as_ref());
                ui.separator();
                ui.add_enabled_ui(open, |ui| self.tabs(ui, snap.as_ref()));
                ui.separator();
                self.footer(ui, snap.as_ref());
            })
            .response
            .rect;
        self.fit(ui, used.height());
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

/// A titled box with room inside it.
fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(9)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().size(15.0));
        ui.add_space(4.0);
        body(ui);
    });
}

/// An on/off switch — clearer at a glance than a checkbox.
fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
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
