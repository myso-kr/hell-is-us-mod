//! What the panel draws, and the requests its controls send. Every tab and row is
//! read off the cheat table; the panel reads the worker's last snapshot and never
//! touches the game itself.

mod backdrop;
mod clues;
mod collect;
mod consent;
mod debug;
mod deep;
mod groups;
mod guide;
mod help;
mod map;
mod map3d;
mod now;
mod quests;
mod saves;
mod splash;

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

use super::theme::{BAD, DIM, OK, WAIT};

/// The window's width before its first page is measured (px).
pub const WIDTH: f32 = 960.0;
/// What a page's width is wrapped in: the window frame's margins and the page's
/// scrollbar (px).
const FRAME: f32 = 2.0 * super::theme::BLOCK;
const SCROLLBAR: f32 = 14.0;
/// The most masonry columns a page gets.
const MAX_COLUMNS: usize = 3;
/// The sidebar's width, and the gap between it and the page with the divider in its
/// middle (px).
const NAV: f32 = 176.0;
const DIVIDER: f32 = 2.0 * super::theme::PAD;
/// The window is never taller than this share of the monitor; the page scrolls
/// inside it instead. Most players are on 1920×1080 (48 %) or 2560×1440 (27 %, Steam's
/// survey, September 2026): at 85 % the panel hid most of the game.
const MAX_SHARE: f32 = 0.72;
/// Nor wider than this share: on 1080p two columns of cards, on 1440p and wider three.
const MAX_WIDTH_SHARE: f32 = 0.55;
/// Room under the page for its footer, and for the title bar and margins (px).
const FOOTER: f32 = 108.0;
const CHROME: f32 = 72.0;

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
    for (name, file) in own
        .into_iter()
        .chain([("malgun", r"C:\Windows\Fonts\malgun.ttf"), ("symbol", r"C:\Windows\Fonts\seguisym.ttf")])
    {
        let Ok(bytes) = std::fs::read(file) else { continue };
        fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.into());
        }
    }
    // The splash's name, bold (the site and video use a condensed bold; Segoe UI Bold
    // is on every Windows). Missing, the body face stands in.
    let display = egui::FontFamily::Name("display".into());
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeuib.ttf") {
        fonts.font_data.insert("display".into(), Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(display.clone()).or_default().push("display".into());
    }
    let body = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.entry(display).or_default().extend(body);
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

/// A pin kind picker: its icon (assets/pins) and name, the list of every kind with
/// theirs. Whether it changed.
fn pin_picker(ui: &mut egui::Ui, id: &str, kind: &mut crate::minimap::PinKind) -> bool {
    let before = *kind;
    let name = |k: crate::minimap::PinKind| {
        let [r, g, b] = k.rgb();
        RichText::new(k.label()).color(Color32::from_rgb(r, g, b))
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        crate::ui::svg::pin(ui, *kind, 18.0);
        egui::ComboBox::from_id_salt(id).width(110.0).selected_text(name(*kind)).show_ui(ui, |ui| {
            for k in crate::minimap::PinKind::ALL {
                ui.horizontal(|ui| {
                    crate::ui::svg::pin(ui, k, 16.0);
                    ui.selectable_value(kind, k, name(k));
                });
            }
        });
    });
    *kind != before
}

/// The Follow button on a place the survey names (guide/track.rs): follow it, or, followed
/// already, let it go.
/// "3 hours ago", as the panel says it: for the overlay's banner.
pub(crate) fn ago_text(secs: u64) -> String {
    now::ago(secs)
}

fn guide_to(state: &mut crate::minimap::MapState, goals: &[crate::goals::Goal], x: &crate::survey::Need) {
    state.toggle(need_track(goals, x));
}

/// Follow a place and bring it into focus, followed already or not (a choice puzzle's
/// answer, asked for).
fn follow_need(state: &mut crate::minimap::MapState, goals: &[crate::goals::Goal], x: &crate::survey::Need) {
    state.ensure(need_track(goals, x));
}

/// A place the survey names, as a track: the goal there if there is one, else the place
/// itself (the overlay adds it as a goal).
fn need_track(goals: &[crate::goals::Goal], x: &crate::survey::Need) -> crate::guide::track::Track {
    // What it is, in words: some pages give a collectible's kind by its key (`RECORDS`).
    let what = match x.what.as_str() {
        w if !w.is_empty() && w.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') => crate::i18n::text(w),
        w => w.to_string(),
    };
    let label = format!("{what} {}", x.label).trim().to_string();
    let goal = goals.iter().find(|g| (g.at[0] - x.at[0]).hypot(g.at[1] - x.at[1]) < 200.0);
    let world = crate::survey::Survey::world_of(&x.world).to_string();
    match goal {
        Some(g) => crate::guide::track::Track {
            id: g.id,
            world,
            at: g.at,
            label,
            place: false,
            colour: 0,
            ..Default::default()
        },
        None => crate::guide::track::Track {
            id: x.id,
            world,
            at: x.at,
            label,
            place: true,
            colour: 0,
            ..Default::default()
        },
    }
}

/// The Follow button on a goal (guide/track.rs), as `guide_to`.
fn follow_goal(state: &mut crate::minimap::MapState, g: &crate::goals::Goal, world: &str) {
    state.toggle(crate::guide::track::Track {
        id: g.id,
        world: crate::survey::Survey::world_of(world).to_string(),
        at: g.at,
        label: g.label.clone(),
        place: false,
        ..Default::default()
    });
}

/// The tool pages in the sidebar, under the cheat groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Now,
    Help,
    Settings,
    Map,
    Guide,
    Quests,
    Clues,
    Puzzles,
    Collect,
    Saves,
    Debug,
}

impl Tool {
    const ALL: [Tool; 11] = [
        Tool::Now,
        Tool::Help,
        Tool::Settings,
        Tool::Map,
        Tool::Guide,
        Tool::Quests,
        Tool::Clues,
        Tool::Puzzles,
        Tool::Collect,
        Tool::Saves,
        Tool::Debug,
    ];

    /// Above the groups in the sidebar, in none.
    const TOP: [Tool; 3] = [Tool::Now, Tool::Help, Tool::Settings];

    /// The sidebar's groups: their names (i18n keys) and pages.
    const GROUPS: [(&'static str, &'static [Tool]); 3] = [
        ("NAV_PLAY", &[Tool::Quests, Tool::Clues, Tool::Puzzles, Tool::Collect]),
        ("NAV_WAY", &[Tool::Guide, Tool::Map]),
        ("NAV_SYSTEM", &[Tool::Saves, Tool::Debug]),
    ];

    /// Its name in settings.txt.
    fn id(self) -> &'static str {
        match self {
            Tool::Now => "now",
            Tool::Help => "help",
            Tool::Settings => "settings",
            Tool::Map => "map",
            Tool::Guide => "guide",
            Tool::Quests => "quests",
            Tool::Clues => "clues",
            Tool::Puzzles => "puzzles",
            Tool::Collect => "collect",
            Tool::Saves => "saves",
            Tool::Debug => "debug",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tool::Now => tr!("NOW"),
            Tool::Help => tr!("HELP"),
            Tool::Settings => tr!("SETTINGS"),
            Tool::Map => tr!("MAP"),
            Tool::Guide => tr!("GUIDE"),
            Tool::Quests => tr!("QUESTS"),
            Tool::Clues => tr!("CLUES_TAB"),
            Tool::Puzzles => tr!("PUZZLES"),
            Tool::Collect => tr!("COLLECT"),
            Tool::Saves => tr!("SAVES"),
            Tool::Debug => tr!("DEBUG"),
        }
    }

    /// What the page is for, in a few words (the help page).
    fn about(self) -> &'static str {
        match self {
            Tool::Now => tr!("ABOUT_NOW"),
            Tool::Help => "",
            Tool::Settings => tr!("ABOUT_SETTINGS"),
            Tool::Map => tr!("ABOUT_MAP"),
            Tool::Guide => tr!("ABOUT_GUIDE"),
            Tool::Quests => tr!("ABOUT_QUESTS"),
            Tool::Clues => tr!("ABOUT_CLUES"),
            Tool::Puzzles => tr!("ABOUT_PUZZLES"),
            Tool::Collect => tr!("ABOUT_COLLECT"),
            Tool::Saves => tr!("ABOUT_SAVES"),
            Tool::Debug => tr!("ABOUT_DEBUG"),
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
    /// A teleport to a place followed was asked for: the way back is offered (groups.rs).
    went: bool,
    /// The 3D map page's scene and view (map3d.rs).
    map3d: map3d::Map3d,
    /// The Map page's settings card: which tab is open.
    map_settings: u8,
    reply: Option<(bool, String, Instant)>,
    /// The console at the foot of the window, and whether the window was showing on
    /// the last frame (to put the cursor in the console as it opens).
    console: super::console::Console,
    was_visible: bool,
    /// The collectible sort unfolded in the collect tab.
    unfolded_collect: Option<&'static str>,
    /// The "now" page's hero map as a texture, and the overlay's counter it was made from.
    hero_tex: Option<(u64, egui::TextureHandle)>,
    /// Long lists grouped by kind instead of in their first order: the places, the
    /// achievements.
    places_grouped: bool,
    achievements_grouped: bool,
    /// The Map page's previews as textures, likewise: the minimap's, the big map's.
    preview_tex: Option<(u64, egui::TextureHandle)>,
    preview_big_tex: Option<(u64, egui::TextureHandle)>,
    /// The Guide page's map (`Shared::ops`), as a texture, and the view it was drawn with.
    ops_tex: Option<(u64, egui::TextureHandle)>,
    ops_view: Option<crate::minimap::View>,
    /// How far the trail ran in each world when the panel started: the journey's "this run".
    walked_from: std::collections::HashMap<String, f32>,
    /// The clues page: the word searched for, and the entry opened (its story unit).
    clue_query: String,
    clue_open: Option<String>,
    /// Puzzle answers and vault codes asked for, by id (not kept between runs).
    revealed: std::collections::HashSet<u64>,
    /// The quest whose needs the Quests page shows (pressed there; else the one in focus).
    shown_quest: Option<String>,
    /// How far each choice puzzle's set has been told (slots.rs): 1 its clue, 2 its answer.
    slot_hints: std::collections::HashMap<u64, u8>,
    /// The puzzle list shows key doors and item placements too.
    show_placements: bool,
    /// Steam's achievements (game/achievements.rs), read again every 10 s.
    achievements: Option<(std::time::Instant, Vec<crate::game::achievements::Achievement>)>,
    /// The achievement card lists the unlocked ones too.
    show_unlocked: bool,
    /// Masonry columns of the page shown (`fit_columns`).
    columns: usize,
    /// The save backups, as last listed.
    backups: Vec<(String, std::path::PathBuf)>,
    backups_read: Option<Instant>,
    /// What the backups take on disk, all told (bytes): read with the list.
    backups_size: u64,
    /// The size last asked of the window, so it is asked once per change.
    height: f32,
    width: f32,
    /// Where the last session left off (session.rs), read once at start; the "previously"
    /// card until dismissed; when this session's state was last written.
    previous: Option<crate::session::Session>,
    /// The sidebar's two groups, each folded under its heading line: the tools start
    /// open, the cheats folded.
    /// The sidebar's groups (play, way-finding, system), each folded or not.
    groups_open: [bool; 3],
    cheats_open: bool,
    session_saved: Option<Instant>,
    /// What the player turned on — not what is on right now. The game exiting or the
    /// gate closing switches cheats off; that must not become the saved choice, or
    /// nothing would come back next time.
    wanted: Vec<Active>,
    keep: bool,
    /// What the player agreed the mod may show and change; `None` until they chose.
    consent: Option<crate::settings::Consent>,
    /// The backdrop moves (backdrop.rs); a switch on the Settings page.
    motion: bool,
    /// The footer's update from GitHub's releases; checked once, at the first frame.
    updater: super::update::Updater,
    update_checked: bool,
    /// While the splash shows: when it opened.
    splash: Option<Instant>,
    /// When the window last changed size: the splash waits for the panel to settle.
    resized_at: Instant,
    /// Cheats to turn back on from last time, once the gate first opens.
    resume: Option<Vec<Active>>,
    /// settings.txt as last written, and when.
    saved: (String, Instant),
}

impl Panel {
    pub fn new(shared: Arc<Shared>, tx: Sender<Request>) -> Panel {
        let saved = settings::load();
        // The splash first (splash.rs): the hotkey thread leaves the window be till it goes.
        shared.splash.store(true, Ordering::SeqCst);
        shared.consent.store(saved.consent.map_or(0, |c| c.0), Ordering::SeqCst);
        crate::settings::LIVE.store(saved.consent.map_or(0, |c| c.0), Ordering::SeqCst);
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
        let previous = crate::session::read();
        *shared.previous.lock().unwrap() = previous.clone();
        // Back after a break: open on the "now" page, where the "previously" card is.
        let back = previous.as_ref().is_some_and(|p| crate::session::now().saturating_sub(p.when) >= 30 * 60);
        Panel {
            fonts_for: crate::i18n::culture(),
            shared,
            tx,
            tab: Group::ALL.into_iter().find(|g| Some(g.id()) == tab).unwrap_or(Group::Survival),
            tool: if saved.consent.is_none() {
                Some(Tool::Settings)
            } else if back {
                Some(Tool::Now)
            } else {
                Tool::ALL.into_iter().find(|t| Some(t.id()) == tab)
            },
            unfolded: [false; 6],
            wanted: resume.clone(),
            keep: saved.keep,
            updater: Default::default(),
            consent: saved.consent,
            motion: saved.motion,
            update_checked: false,
            splash: Some(Instant::now()),
            resized_at: Instant::now(),
            resume: (saved.keep && !resume.is_empty()).then_some(resume),
            saved: (settings::render(&saved), Instant::now()),
            marks: verify::load(),
            on: HashMap::new(),
            value,
            slid: HashMap::new(),
            sent: None,
            went: false,
            map3d: Default::default(),
            map_settings: 0,
            reply: None,
            console: Default::default(),
            was_visible: false,
            unfolded_collect: None,
            hero_tex: None,
            preview_tex: None,
            preview_big_tex: None,
            ops_tex: None,
            ops_view: None,
            walked_from: Default::default(),
            places_grouped: false,
            achievements_grouped: false,
            clue_query: String::new(),
            clue_open: None,
            revealed: Default::default(),
            shown_quest: None,
            slot_hints: Default::default(),
            show_placements: false,
            achievements: None,
            show_unlocked: false,
            columns: 2,
            backups: Vec::new(),
            backups_size: 0,
            backups_read: None,
            height: 0.0,
            width: 0.0,
            previous,
            groups_open: [true; 3],
            cheats_open: !back && !Tool::ALL.iter().any(|t| Some(t.id()) == tab),
            session_saved: None,
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
            tab: Some(self.tool.map_or(self.tab.id(), Tool::id).to_string()),
            pos: *self.shared.pos.lock().unwrap(),
            on: self.wanted.iter().map(|a| (a.cheat.to_string(), a.value)).collect(),
            values: self.value.iter().map(|(id, v)| (id.to_string(), *v)).collect(),
            consent: self.consent,
            motion: self.motion,
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

    /// Whether the player agreed to this (`Consent`'s bits).
    fn grants(&self, bit: u8) -> bool {
        self.consent.is_some_and(|c| c.has(bit))
    }

    /// Whether a page may show: the map's and the guide's with the map, the puzzles' with
    /// answers; the rest always.
    fn allowed(&self, tool: Tool) -> bool {
        use crate::settings::Consent;
        match tool {
            Tool::Map => self.grants(Consent::MAP),
            Tool::Guide => self.grants(Consent::GUIDE),
            Tool::Puzzles => self.grants(Consent::ANSWERS),
            _ => true,
        }
    }

    /// The player's choice, kept and handed to the overlay. Cheats taken back are switched
    /// off now, their original values restored.
    fn set_consent(&mut self, c: crate::settings::Consent) {
        use crate::settings::Consent;
        if self.grants(Consent::CHEATS) && !c.has(Consent::CHEATS) {
            let _ = self.tx.send(Request::Restore);
            self.on.clear();
            self.wanted.clear();
            self.resume = None;
            self.sent = Some(Instant::now());
        }
        self.consent = Some(c);
        self.shared.consent.store(c.0, Ordering::SeqCst);
        crate::settings::LIVE.store(c.0, Ordering::SeqCst);
    }

    /// Turn last time's cheats back on — the first time the gate is open.
    fn resume(&mut self, snap: &Snapshot) {
        if snap.gate.is_err() {
            return;
        }
        if !self.grants(crate::settings::Consent::CHEATS) {
            return;
        }
        let Some(resume) = self.resume.take() else { return };
        for a in &resume {
            self.on.insert(a.cheat, true);
            self.value.insert(a.cheat, a.value);
        }
        self.send_active();
        self.reply = Some((true, trf!("TURNED_CHEATS_FROM_LAST_TIME_BACK", count = resume.len()), Instant::now()));
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
        let (rect, bar) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click_and_drag());
        if bar.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        // Laid out after the drag area, so the buttons sit on top of it and get their
        // own clicks rather than starting a drag.
        let row = egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center));
        ui.scope_builder(row, |ui| {
            ui.label(RichText::new("◆").color(super::theme::ACCENT));
            ui.label(RichText::new("Hell Is Us Mod").strong().color(super::theme::TITLE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(" × ").on_hover_text(tr!("CLOSE_RESTORE_THE_ORIGINAL_VALUES_AND")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(" — ").on_hover_text(tr!("HIDE_OPENS_IT_AGAIN")).clicked() {
                    hotkey::hide(&self.shared);
                }
                // The panel's and the console's keys, as keycaps (`<kbd>`), right to left:
                // written as the keys pressed, ` and Ctrl+` — "~" is Ctrl+` only on some
                // layouts (US, Korean), another key on others (UK, AZERTY).
                ui.add_space(super::theme::INLINE);
                let console = if self.console.open { super::theme::ACCENT } else { DIM };
                ui.label(RichText::new(tr!("CONSOLE")).small().color(console)).on_hover_text(tr!("KEY_LEFT_OF_1"));
                tw::kbd(ui, "`").on_hover_text(tr!("KEY_LEFT_OF_1"));
                ui.label(RichText::new("+").small().color(DIM));
                tw::kbd(ui, "Ctrl");
                ui.add_space(super::theme::BLOCK);
                ui.label(RichText::new(tr!("PANEL_KEY_LABEL")).small().color(DIM)).on_hover_text(tr!("KEY_LEFT_OF_1"));
                tw::kbd(ui, "`").on_hover_text(tr!("KEY_LEFT_OF_1"));
            });
        });
        ui.add_space(super::theme::TIGHT);
        ui.separator();
        ui.add_space(super::theme::TIGHT);
    }

    /// The game and the hero gate, stacked for the sidebar.
    fn status(&self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let launching = self.shared.launched.load(Ordering::SeqCst);
        let line = |ui: &mut egui::Ui, title: &str, text: RichText| {
            ui.label(RichText::new(title).color(DIM).small());
            ui.add(egui::Label::new(text).wrap());
            ui.add_space(super::theme::INLINE);
        };
        match snap.map(|s| &s.game) {
            Some(Ok((pid, version))) => {
                // "● Connected" in the body size, the build and PID under it small.
                let connected = trf!("CONNECTED_V_PID", version = version, pid = pid);
                let (state, detail) = connected.split_once('\n').unwrap_or((&connected, ""));
                ui.label(RichText::new(tr!("GAME")).color(DIM).small());
                ui.add(egui::Label::new(RichText::new(state).color(OK)).wrap());
                if !detail.is_empty() {
                    ui.add(egui::Label::new(RichText::new(detail).color(DIM).small()).wrap());
                }
                ui.add_space(super::theme::INLINE);
                // A build the mod was not checked on: names may have moved.
                if version != crate::game::TESTED_BUILD {
                    let warn = trf!("CHECKED_ON_BUILD_IF_SOMETHING_IS", tested = crate::game::TESTED_BUILD);
                    line(ui, "", RichText::new(warn).color(WAIT).small());
                }
            }
            _ if launching => line(ui, tr!("GAME"), RichText::new(tr!("LAUNCHING_CONNECTS_ONCE_IT_IS_UP")).color(WAIT)),
            Some(Err(e)) => line(ui, tr!("GAME"), RichText::new(trf!("NOT_CONNECTED", e = e)).color(DIM)),
            None => line(ui, tr!("GAME"), RichText::new(tr!("STARTING")).color(DIM)),
        }
        match snap.map(|s| &s.gate) {
            Some(Ok(())) => line(ui, tr!("HERO_GATE"), RichText::new(tr!("OPEN_IN_CONTROL")).color(OK)),
            Some(Err(e)) if snap.is_some_and(|s| s.game.is_ok()) => {
                line(ui, tr!("HERO_GATE"), RichText::new(trf!("GATE_CLOSED_WHY", e = e)).color(WAIT))
            }
            _ => line(ui, tr!("HERO_GATE"), RichText::new(tr!("GATE_CLOSED")).color(DIM)),
        }
        self.game_data(ui, snap);
    }

    /// The guide's game data (gamedata.rs) and the runtime it needs (runtime.rs): what
    /// is being read, or the button that installs .NET 8 — the one step left to the
    /// player, since it puts software on their machine.
    fn game_data(&self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        use crate::gamedata::{Kind, Run};
        use crate::runtime::Install;
        ui.label(RichText::new(tr!("GAME_DATA")).color(DIM).small());
        let build = snap.and_then(|s| s.game.as_ref().ok()).map(|(_, v)| v.clone());
        let needs = build.as_deref().and_then(crate::gamedata::next);
        // The state in the body size, as the game's and the gate's above; what to do
        // about it, small — the same two sizes as every sidebar line.
        let text = |ui: &mut egui::Ui, t: String, c: Color32| {
            ui.add(egui::Label::new(RichText::new(t).color(c)).wrap());
        };
        let small = |ui: &mut egui::Ui, t: String, c: Color32| {
            ui.add(egui::Label::new(RichText::new(t).color(c).small()).wrap());
        };
        match (crate::gamedata::state(), crate::runtime::install_state()) {
            (Run::Running(Kind::Survey), _) => text(ui, tr!("READING_THE_GAMES_MAPS").into(), WAIT),
            (Run::Running(Kind::Locale), _) => text(ui, tr!("READING_THE_GAMES_TEXT").into(), WAIT),
            (_, Install::Winget) => text(ui, tr!("INSTALLING_NET_8_WINGET").into(), WAIT),
            (_, Install::Script) => text(ui, tr!("INSTALLING_NET_8_MODS_FOLDER").into(), WAIT),
            (Run::Failed(_, e), _) if needs.is_some() => text(ui, trf!("READING_FAILED", e = e), BAD),
            _ if needs.is_none() && build.is_some() => text(ui, tr!("GAME_DATA_READY").into(), OK),
            _ if crate::runtime::available() == Some(false) => {
                if let Install::Failed(e) = crate::runtime::install_state() {
                    text(ui, trf!("INSTALL_FAILED", e = e), BAD);
                }
                small(ui, tr!("THE_SURVEY_NEEDS_THE_NET_8_RUNTIME").into(), DIM);
                if ui.button(tr!("INSTALL_NET_8")).on_hover_text(tr!("INSTALL_NET_8_HOW")).clicked() {
                    crate::runtime::install();
                }
            }
            _ if build.is_some() => small(ui, tr!("READ_ONCE_THE_HERO_IS_IN_CONTROL").into(), DIM),
            _ => text(ui, tr!("WAITING_FOR_THE_GAME").into(), DIM),
        }
        ui.add_space(super::theme::INLINE);
    }

    /// Where this session is, every half minute while the hero is in play — the next
    /// session's "previously" card (session.rs).
    fn save_session(&mut self, snap: &Snapshot) {
        if self.session_saved.is_some_and(|at| at.elapsed() < Duration::from_secs(30)) || snap.gate.is_err() {
            return;
        }
        let (Some(world), Some((p, _))) = (snap.world.as_deref(), snap.pose) else { return };
        crate::session::write(&crate::session::Session {
            when: crate::session::now(),
            world: crate::survey::Survey::world_of(world).to_string(),
            quest: self.shared.map.lock().unwrap().quest.clone(),
            at: [p[0] as f32, p[1] as f32, p[2] as f32],
        });
        self.session_saved = Some(Instant::now());
    }

    /// Start reading what game data is missing, once the hero is in control and the
    /// runtime is there — never while a run is going or after one failed (the panel
    /// says why; the console's `doctor survey` tries again).
    fn read_game_data(&self, snap: &Snapshot) {
        let (Ok((_, build)), Ok(())) = (&snap.game, &snap.gate) else { return };
        if !matches!(crate::gamedata::state(), crate::gamedata::Run::Idle | crate::gamedata::Run::Done) {
            return;
        }
        if crate::runtime::available() != Some(true) {
            return;
        }
        if let Some(kind) = crate::gamedata::next(build) {
            crate::gamedata::start(kind, build);
        }
    }

    /// The sidebar's pages: the tools, then the cheat groups (the heading counts the
    /// cheats on), each group folded or open under its heading.
    fn nav(&mut self, ui: &mut egui::Ui) {
        // A page not agreed to stays in the list, greyed and not to be opened: what the
        // mod could do stays in sight, with where to allow it on hover.
        let item = |ui: &mut egui::Ui, on: bool, text: String, allowed: bool| {
            let label = RichText::new(text).size(13.0).color(if on { super::theme::TITLE } else { DIM });
            let button = egui::Button::new(label)
                .fill(if on { super::theme::ACCENT_DEEP } else { Color32::TRANSPARENT })
                .stroke(if on { egui::Stroke::new(1.0, super::theme::ACCENT) } else { egui::Stroke::NONE })
                .corner_radius(super::theme::R_CONTROL)
                .min_size(egui::vec2(ui.available_width(), 28.0));
            ui.add_enabled(allowed, button).on_disabled_hover_text(tr!("CONSENT_NEEDED")).clicked()
        };
        // A group's heading folds it; folding leaves the page shown as it is.
        let heading = |ui: &mut egui::Ui, open: &mut bool, text: String| {
            let line = format!("{}  {text}", if *open { "▾" } else { "▸" });
            let head = egui::Button::new(RichText::new(line).color(DIM).small())
                .fill(Color32::TRANSPARENT)
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(ui.available_width(), 22.0));
            if ui.add(head).clicked() {
                *open = !*open;
            }
        };
        ui.spacing_mut().item_spacing.y = 2.0;
        // The pages a player opens first stand alone at the top, in no group.
        for tool in Tool::TOP {
            if item(ui, self.tool == Some(tool), tool.label().to_string(), true) {
                self.tool = Some(tool);
            }
        }
        // In the order they are used: playing (what to do, what is known, how to open
        // it, how far along), finding the way, then the system's own pages.
        for (i, (name, tools)) in Tool::GROUPS.iter().enumerate() {
            ui.add_space(super::theme::BLOCK);
            heading(ui, &mut self.groups_open[i], crate::i18n::tr(name).to_string());
            if self.groups_open[i] {
                for &tool in *tools {
                    let allowed = self.allowed(tool);
                    if item(ui, self.tool == Some(tool), tool.label().to_string(), allowed) {
                        self.tool = Some(tool);
                    }
                }
            }
        }
        ui.add_space(super::theme::BLOCK);
        let cheats = self.grants(crate::settings::Consent::CHEATS);
        let on = CHEATS.iter().filter(|c| self.on.get(c.id).copied().unwrap_or(false)).count();
        let title = if on > 0 { format!("{}  ({on})", tr!("CHEATS")) } else { tr!("CHEATS").to_string() };
        heading(ui, &mut self.cheats_open, title);
        if self.cheats_open {
            for g in Group::ALL {
                let on = CHEATS.iter().filter(|c| c.group == g && self.on.get(c.id).copied().unwrap_or(false)).count();
                let text = if on > 0 { format!("{}  ({on})", g.label()) } else { g.label().to_string() };
                if item(ui, self.tool.is_none() && self.tab == g, text, cheats) {
                    self.tab = g;
                    self.tool = None;
                }
            }
        }
    }

    /// The page chosen in the sidebar: cards on an auto-fit grid (tw.rs) — side by
    /// side while each can be `CARD_MIN` wide, stacked when not.
    fn page(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        // A page not agreed to (or a cheat page without the cheats) is the Settings page.
        let shown = self.tool.map_or(self.grants(crate::settings::Consent::CHEATS), |t| self.allowed(t));
        if !shown {
            self.tool = Some(Tool::Settings);
        }
        // The overlay draws the Map page's preview only while that page shows.
        self.shared.preview_wanted.store(self.tool == Some(Tool::Map), std::sync::atomic::Ordering::SeqCst);
        self.shared.ops_wanted.store(self.tool == Some(Tool::Guide), std::sync::atomic::Ordering::SeqCst);
        match self.tab {
            // The figures beside what the cheats on write; the test record across the page.
            _ if self.tool == Some(Tool::Debug) => {
                let cols = self.columns;
                tw::spans(t, cols, &[1, 2, 3, 3], |t, i| match i {
                    0 => self.debug_status(t, snap),
                    1 => self.debug_writes(t, snap),
                    2 => self.debug_guide(t),
                    _ => self.debug_marks(t),
                })
            }
            _ if self.tool == Some(Tool::Saves) => {
                // The figures and actions across the top; the backups two columns wide,
                // the game's own files beside them.
                let cols = self.columns;
                tw::spans(t, cols, &[3, 2, 1], |t, i| match i {
                    0 => self.saves_overview(t),
                    1 => self.backups_card(t),
                    _ => self.slots_card(t),
                })
            }
            _ if self.tool.is_some() => self.map_tab(t, snap),
            // A group's cheats two columns wide, what is on (every group's) beside them;
            // Movement adds the teleport to what is followed and the saved positions.
            g => {
                let cols = self.columns;
                let spans: &[u16] = if g == Group::Movement { &[2, 1, 2, 1] } else { &[2, 1] };
                tw::spans(t, cols, spans, |t, i| match i {
                    0 => card(t, g.label(), |t| self.held(t, g, snap)),
                    1 => self.summary(t, snap),
                    2 => self.teleports(t, snap),
                    _ => self.positions(t, snap),
                })
            }
        }
    }

    /// The footer's left half: restore all and keep settings, then a note (a reply for a
    /// few seconds, else the game's notice, else the rule). Mirrored by `footer_right`:
    /// buttons in the first row, small text in the second.
    fn footer_left(&mut self, ui: &mut egui::Ui, snap: Option<&Snapshot>) {
        let pending = snap.map_or(0, |s| s.pending);
        let attached = snap.is_some_and(|s| s.game.is_ok());
        let small = |t: String, c: Color32| RichText::new(t).small().color(c);
        let note = match (&self.reply, snap.and_then(|s| s.notice.clone())) {
            (Some((ok, text, at)), _) if at.elapsed() < Duration::from_secs(5) => {
                small(text.clone(), if *ok { OK } else { BAD })
            }
            (_, Some(n)) => small(n, if attached { WAIT } else { DIM }),
            _ => small(tr!("WRITES_ONLY_WHILE_THE_HERO_IS").into(), DIM),
        };
        ui.horizontal(|ui| {
            let restore = ui.add_enabled(pending > 0, egui::Button::new(trf!("RESTORE_ALL", pending = pending)));
            if restore.clicked() {
                let _ = self.tx.send(Request::Restore);
                self.on.clear();
                self.wanted.clear();
                self.resume = None;
                self.sent = Some(Instant::now());
            }
            ui.add_space(super::theme::INLINE);
            toggle(ui, &mut self.keep).on_hover_text(tr!("NEXT_RUN_ONCE_THE_HERO_CAN"));
            ui.label(tr!("KEEP_SETTINGS")).on_hover_text(tr!("NEXT_RUN_ONCE_THE_HERO_CAN"));
        });
        ui.add_space(super::theme::TIGHT);
        ui.add(egui::Label::new(note).truncate());
    }

    /// The footer's right half, right-aligned: the update, then the version, GitHub and
    /// the copyright. The update is checked once, at the first frame.
    fn footer_right(&mut self, ui: &mut egui::Ui) {
        if !self.update_checked {
            self.update_checked = true;
            self.updater.check(ui.ctx());
        }
        let small = |t: String| RichText::new(t).small().color(DIM);
        let width = ui.available_width();
        let row = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(egui::vec2(width, row), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            self.update_control(ui)
        });
        ui.add_space(super::theme::TIGHT);
        let line = ui.text_style_height(&egui::TextStyle::Small);
        ui.allocate_ui_with_layout(egui::vec2(width, line), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(small("© 2026 myso-kr · MIT".into()));
            ui.label(small("·".into()));
            if ui.link(RichText::new(tr!("GITHUB")).small()).clicked() {
                super::update::open(super::update::PAGE);
            }
            ui.label(small("·".into()));
            ui.label(small(format!("v{}", env!("CARGO_PKG_VERSION"))));
        });
    }

    /// The splash, a window of its own in the middle of the screen (splash.rs), while
    /// the panel lays itself out off the screen. It goes once it has shown `splash::MIN`,
    /// the first reading is in with nothing still being read, the cheats kept from last
    /// time are back on, and the panel has kept its size a moment (or `splash::MAX` has
    /// gone by); the panel then comes to its place.
    fn splash_window(&mut self, ctx: &egui::Context, since: Instant, snap: Option<&Snapshot>) {
        use crate::gamedata::Run;
        let reading = matches!(crate::gamedata::state(), Run::Running(_));
        let game = snap.map(|s| s.game.is_ok());
        let ready = match game {
            Some(true) => !reading,
            Some(false) => true,
            None => false,
        };
        let settled = self.resized_at.elapsed() >= Duration::from_millis(300);
        // The cheats kept from last time, back on: once the hero is in control they are
        // sent (`resume`), and the engine is given a moment to take them.
        let gate = snap.map(|s| s.gate.is_ok());
        let cheats = gate != Some(true)
            || (self.resume.is_none() && self.sent.is_none_or(|t| t.elapsed() > Duration::from_millis(800)));
        let elapsed = since.elapsed();
        if elapsed >= splash::MAX || (elapsed >= splash::MIN && ready && settled && cheats) {
            self.splash = None;
            self.shared.splash.store(false, Ordering::SeqCst);
            // To its place (the saved one, else over the game) and in front.
            hotkey::show(&self.shared, ctx);
            return;
        }
        let colour = |done: Option<bool>, busy: bool| match done {
            Some(true) => OK,
            _ if busy => WAIT,
            _ => DIM,
        };
        let steps = [
            splash::Step { name: tr!("GAME").into(), colour: colour(game, snap.is_none()) },
            splash::Step { name: tr!("HERO_GATE").into(), colour: colour(gate, game == Some(true)) },
            splash::Step {
                name: tr!("GAME_DATA").into(),
                colour: if reading { WAIT } else { colour(game.map(|g| g && ready), false) },
            },
        ];
        // Time and readiness together: never full before it may close.
        let by_time = elapsed.as_secs_f32() / splash::MIN.as_secs_f32();
        let progress = if ready && settled && cheats { by_time } else { by_time.min(0.85) };
        let (w, h) = splash::screen();
        let ppp = ctx.pixels_per_point();
        let at = egui::pos2((w / ppp - splash::SIZE.x) / 2.0, (h / ppp - splash::SIZE.y) / 2.0);
        let builder = egui::ViewportBuilder::default()
            .with_title("Hell Is Us Mod")
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false)
            .with_position(at)
            .with_inner_size(splash::SIZE);
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("hiumod-splash"), builder, |ctx, _| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| splash::draw(ui, since, &steps, progress));
        });
        ctx.request_repaint_after(Duration::from_millis(16));
    }

    /// Updating from GitHub's releases, by hand at every step: check, download, restart.
    /// (Right to left: what is added first sits furthest right.)
    fn update_control(&mut self, ui: &mut egui::Ui) {
        use super::update::{self, Stage};
        // Right to left: the button at the edge, its status before it.
        match self.updater.stage() {
            stage @ (Stage::Idle | Stage::Current | Stage::Failed(_)) => {
                if ui.button(tr!("CHECK_FOR_UPDATES")).on_hover_text(tr!("CHECK_FOR_UPDATES_HINT")).clicked() {
                    self.updater.check(ui.ctx());
                }
                match &stage {
                    Stage::Current => {
                        ui.label(RichText::new(tr!("UP_TO_DATE")).color(OK));
                    }
                    Stage::Failed(why) => {
                        ui.label(RichText::new(tr!("UPDATE_FAILED")).color(BAD)).on_hover_text(why);
                    }
                    _ => {}
                }
            }
            Stage::Checking => {
                ui.add_enabled(false, egui::Button::new(tr!("CHECK_FOR_UPDATES")));
                ui.label(RichText::new(tr!("CHECKING_FOR_UPDATES")).color(DIM));
            }
            Stage::Found(release) => {
                let label = trf!("DOWNLOAD_VERSION", version = release.version);
                if ui.button(RichText::new(label).color(super::theme::ACCENT)).clicked() {
                    self.updater.fetch(ui.ctx(), release.clone());
                }
                if ui.link(tr!("RELEASE_NOTES")).clicked() {
                    update::open(&release.page);
                }
            }
            Stage::Fetching(_) => {
                ui.add_enabled(false, egui::Button::new(tr!("DOWNLOADING_UPDATE")));
            }
            Stage::Ready(version, files) => {
                let label = trf!("RESTART_TO_UPDATE", version = version);
                let button = egui::Button::new(RichText::new(label).color(super::theme::ACCENT));
                if ui.add(button).on_hover_text(tr!("RESTART_TO_UPDATE_HINT")).clicked() {
                    match update::apply(&files) {
                        // Closed as × closes it: the game's values put back first.
                        Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(e) => self.updater.fail(e),
                    }
                }
            }
        }
    }

    /// The window as big as what is in it: the page area is the same on every page, so
    /// it changes only with the monitor (and the console's or the update's footer).
    fn fit(&mut self, ui: &egui::Ui, width: f32, height: f32) {
        let (width, height) = (width.ceil(), height.ceil());
        if (height - self.height).abs() > 1.0 || (width - self.width).abs() > 1.0 {
            (self.width, self.height) = (width, height);
            self.resized_at = Instant::now();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
        }
    }

    /// How wide every page is: the monitor's columns of cards, the same on every page, so
    /// the window keeps its width from page to page (it changed with each before).
    fn page_width(&self) -> f32 {
        tw::cards_width(self.columns as u32)
    }

    /// Masonry columns, the same for every page: as many as the monitor has room for in
    /// `MAX_WIDTH_SHARE`, up to three (two on 1080p, three on 1440p and wider). They were
    /// two or three by the page's cards, and the window grew and shrank from page to page;
    /// a page with fewer cards now spreads them, or its two-column grid, over the width.
    fn fit_columns(&mut self, monitor_w: f32) {
        let room = monitor_w * MAX_WIDTH_SHARE - (FRAME + NAV + DIVIDER + SCROLLBAR);
        let fits = (((room + tw::GAP) / (tw::CARD + tw::GAP)).floor() as usize).max(1);
        self.columns = fits.min(MAX_COLUMNS);
    }
}

impl Panel {
    /// The console, dropped down from the top of the game window across its width and
    /// see-through, as in Half-Life: its own window (an egui viewport), shown while the
    /// panel is and the console is open.
    fn console_window(&mut self, ctx: &egui::Context) {
        // Opened and closed by its key (hotkey.rs, `Shared::console_open`); opening puts the
        // cursor in it. Kept while open, made hidden: whether it shows is `watch`'s call.
        let open = self.shared.console_open.load(std::sync::atomic::Ordering::SeqCst);
        if open != self.console.open {
            self.console.open = open;
            if open {
                self.console.focus();
            }
        }
        let game = self.shared.game_pid.load(std::sync::atomic::Ordering::SeqCst);
        let open = self.console.open && game != 0;
        let Some((_, r)) = open.then(|| hotkey::game_window(game)).flatten() else {
            return;
        };
        let ppp = ctx.pixels_per_point();
        let (w, h) = ((r.right - r.left) as f32 / ppp, (r.bottom - r.top) as f32 / ppp);
        let size = egui::vec2(w, (h * super::console::SHARE).max(180.0));
        let builder = egui::ViewportBuilder::default()
            .with_title("Hell Is Us Mod · console")
            .with_visible(false)
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
            self.read_game_data(s);
            self.save_session(s);
        }
        let open = snap.as_ref().is_some_and(|s| s.gate.is_ok());

        // A share of the monitor tall, whatever the page: past that the page scrolls and the
        // sidebar and footer stay put.
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
        self.fit_columns(monitor_w);
        // The scroll bar's lane is the frame's right margin, not a gutter of its own:
        // the frame gives up that much on the right, the header and footer take it back
        // as padding, so every edge is `BLOCK` from the window's whether the page
        // scrolls or not.
        let lane = {
            let s = ui.spacing().scroll;
            (s.bar_width + s.bar_inner_margin + s.bar_outer_margin).min(super::theme::BLOCK)
        };
        // The columns already fit MAX_WIDTH_SHARE; a page wider by itself (debug) up to 90 %.
        let width = (FRAME + NAV + DIVIDER + self.page_width()).min(monitor_w * 0.9);
        let margin = egui::Margin {
            left: super::theme::BLOCK as i8,
            right: (super::theme::BLOCK - lane) as i8,
            top: super::theme::BLOCK as i8,
            bottom: super::theme::BLOCK as i8,
        };
        let motion = self.motion;
        let used = egui::Frame::central_panel(ui.style())
            .inner_margin(margin)
            .show(ui, |ui| {
                // The backdrop first, under everything: it shows between the cards.
                // Not while parked off the screen for the console alone (hotkey.rs `park`).
                if motion && self.shared.visible.load(Ordering::SeqCst) {
                    let t = ui.input(|i| i.time) as f32;
                    backdrop::draw(ui.painter(), ui.clip_rect(), t);
                    ui.ctx().request_repaint_after(Duration::from_secs_f32(1.0 / backdrop::FPS));
                }
                let inner = width - FRAME + lane;
                ui.set_width(inner);
                ui.set_max_width(inner);
                ui.allocate_ui_with_layout(
                    egui::vec2(inner - lane, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.title_bar(ui),
                );
                // `grid grid-cols-[176px_1fr]`, three rows: the sidebar and the page (which
                // scrolls inside the window past `page_height`, over its own grid of
                // cards), then the footer across both.
                let left = ui.cursor().min.x;
                let top = ui.cursor().min.y;
                let shell = egui_taffy::taffy::Style {
                    gap: egui_taffy::taffy::Size {
                        width: egui_taffy::taffy::prelude::length(DIVIDER),
                        height: egui_taffy::taffy::prelude::length(super::theme::INLINE),
                    },
                    ..tw::full(tw::sidebar(NAV, DIVIDER))
                };
                let mut body = 0.0_f32;
                tui(ui, ui.id().with("shell")).reserve_available_width().style(shell).show(|t| {
                    let side = block(t, |ui| {
                        self.status(ui, snap.as_ref());
                        ui.add_space(super::theme::TIGHT);
                        ui.separator();
                        ui.add_space(super::theme::INLINE);
                        self.nav(ui);
                        ui.min_rect().bottom()
                    });
                    let page_w = self.page_width();
                    let page = tw::block_at_least(t, page_w + lane, |ui| {
                        // A ScrollArea is no taller than the room its parent has, and a taffy
                        // leaf's room is the height it reported last frame: from 0, it stayed
                        // 0 and the page drew nothing. Its room is given here outright, and it
                        // takes all of it, whatever the page: the window's height no longer
                        // changes from page to page (a short page leaves room under its cards,
                        // a long one scrolls).
                        let room = egui::vec2(page_w + lane, page_height);
                        ui.allocate_ui_with_layout(room, egui::Layout::top_down(egui::Align::Min), |ui| {
                            // The scroll area shrinks to a short page; the room it is in does
                            // not, so the footer stays where it is from page to page.
                            ui.set_min_height(page_height);
                            tw::scroll(ui, "page", page_height, page_height, super::theme::SURFACE, |ui| {
                                // The cards' width, whether the bar shows or not.
                                ui.set_width(page_w);
                                ui.add_enabled_ui(open, |ui| {
                                    tui(ui, ui.id().with("page"))
                                        .reserve_available_width()
                                        .style(tw::full(tw::col(tw::GAP)))
                                        .show(|t| self.page(t, snap.as_ref()));
                                });
                            });
                        });
                        ui.min_rect().bottom()
                    });
                    body = side.max(page);
                    // The footer: across both columns, a hairline over two even halves.
                    let foot = tw::span_all(egui_taffy::taffy::Style {
                        padding: egui_taffy::taffy::Rect {
                            left: egui_taffy::taffy::prelude::length(0.0_f32),
                            right: egui_taffy::taffy::prelude::length(lane),
                            top: egui_taffy::taffy::prelude::length(0.0_f32),
                            bottom: egui_taffy::taffy::prelude::length(0.0_f32),
                        },
                        ..tw::col(super::theme::TIGHT)
                    });
                    t.style(foot).add(|t| {
                        block(t, |ui| ui.separator());
                        t.style(tw::row(super::theme::INLINE)).add(|t| {
                            tw::share(t, |ui| self.footer_left(ui, snap.as_ref()));
                            tw::share(t, |ui| self.footer_right(ui));
                        });
                    });
                });
                let x = left + NAV + DIVIDER / 2.0;
                let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                ui.painter().vline(x, top..=body, stroke);
            })
            .response
            .rect;
        self.fit(ui, width, used.height().min(max_height));
        self.console_window(ui.ctx());
        if let Some(since) = self.splash {
            self.splash_window(ui.ctx(), since, snap.as_ref());
        }
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

/// An on/off switch — clearer at a glance than a checkbox. As tall as a line of body
/// text, so a label beside it (top-aligned, as it may wrap) sits on its first line.
pub(super) fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = ui.text_style_height(&egui::TextStyle::Body) * egui::vec2(2.0, 1.0);
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
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), radius - 2.0, super::theme::TITLE);
    }
    response
}

/// A pill that is filled with the kind's colour while on.
fn chip(text: &str, on: bool, colour: Color32) -> egui::Button<'static> {
    let label = RichText::new(text.to_string()).color(if on { super::theme::GROUND } else { DIM });
    egui::Button::new(label)
        .fill(if on { colour } else { Color32::TRANSPARENT })
        .stroke(egui::Stroke::new(1.0, if on { colour } else { colour.gamma_multiply(0.5) }))
        .corner_radius(super::theme::R_CHIP)
}
