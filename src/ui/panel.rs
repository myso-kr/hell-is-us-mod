//! What the panel draws, and the requests its controls send. Every tab and row is
//! read off the cheat table; the panel reads the worker's last snapshot and never
//! touches the game itself.

use super::{hotkey, Request, Shared};
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

pub const WIDTH: f32 = 420.0;

/// egui's own fonts have no Hangul. Malgun Gothic ships with Windows, so it is
/// borrowed from the system rather than bundled; without it the panel still works,
/// with boxes where the Korean would be.
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
            tab: Some(if self.debug { "debug" } else { self.tab.id() }.to_string()),
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
                if ui.selectable_label(!self.debug && self.tab == g, text).clicked() {
                    self.tab = g;
                    self.debug = false;
                }
            }
            ui.separator();
            if ui.selectable_label(self.debug, "디버그").clicked() {
                self.debug = true;
            }
        });
        ui.add_space(4.0);
        match self.tab {
            _ if self.debug => self.debug_tab(ui, snap),
            g => self.held(ui, g, snap),
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
        ui.label(RichText::new("혼자 플레이할 때만 동작합니다 · 업적은 차단되지 않습니다").color(DIM).small());
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
