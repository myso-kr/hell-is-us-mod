//! The console, dropped down over the top of the game while the panel is open (as in
//! Half-Life): the CLI's commands, typed and run without leaving the game — `pose`,
//! `doctor survey`, `doctor inspect hero 1` …
//!
//! A command runs as this same executable with those arguments, in a child process
//! without a window; what it prints comes back line by line as it goes. `help` and
//! `clear` are the console's own. A bare `hiumod` would open a second panel, so it is
//! refused; a leading `hiumod` is dropped. ↑/↓ recall earlier commands, Esc stops the
//! one running.
//!
//! Its look is the panel's (theme.rs, tw.rs): a quiet header, the log as calm monospace
//! on the surface (the command echoed in the accent after a prompt glyph, results in
//! the text colour, warnings and errors in their state colours, a `[hiumod]` tag dim),
//! and the command line as a field like the panel's.

use super::theme::{ACCENT, BAD, CONTROL, DIM, EDGE, R_CARD, R_CHIP, R_CONTROL, SURFACE, TEXT, TITLE, WAIT};
use eframe::egui::{self, Color32, RichText};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

/// How many lines are kept.
const KEEP: usize = 2000;
/// The prompt glyph before the command line and each command echoed in the log.
const PROMPT: &str = "›";
/// The field's padding, across and down (px).
const FIELD_X: i8 = 10;
const FIELD_Y: i8 = 6;
/// The log's padding (px), and the space between its lines.
const LOG_X: i8 = 12;
const LOG_Y: i8 = 8;
const LINE_GAP: f32 = 2.0;
/// How much of the game window's height the console takes.
pub const SHARE: f32 = 0.38;

#[derive(Clone, Copy, PartialEq)]
enum Line {
    Input,
    Out,
    Warn,
    Err,
    Note,
}

impl Line {
    fn colour(self) -> Color32 {
        match self {
            Line::Input => ACCENT,
            Line::Out => TEXT,
            Line::Warn => WAIT,
            Line::Err => BAD,
            Line::Note => DIM,
        }
    }
}

/// What a printed line is, by the pipe it came on and how it starts: `log!` lines go
/// to stderr but are not errors, and its warnings are warnings on either pipe.
fn classify(pipe: Line, text: &str) -> Line {
    if text.starts_with("[hiumod] WARN") {
        Line::Warn
    } else if pipe == Line::Err && text.starts_with("[hiumod]") {
        Line::Out
    } else {
        pipe
    }
}

/// A line's leading meta, drawn dim: short `[tag] ` groups at its start (`[hiumod]`,
/// a timestamp). Returns the meta and the rest.
fn meta(text: &str) -> (&str, &str) {
    let mut end = 0;
    while text[end..].starts_with('[') {
        match text[end..].find(']') {
            Some(close) if close <= 24 => {
                end += close + 1;
                end += text[end..].len() - text[end..].trim_start_matches(' ').len();
            }
            _ => break,
        }
    }
    text.split_at(end)
}

pub struct Console {
    pub open: bool,
    lines: VecDeque<(Line, String)>,
    input: String,
    history: Vec<String>,
    recall: Option<usize>,
    running: Option<(Child, Receiver<(Line, String)>)>,
    focus: bool,
}

impl Default for Console {
    fn default() -> Console {
        let mut c = Console {
            open: false,
            lines: VecDeque::new(),
            input: String::new(),
            history: Vec::new(),
            recall: None,
            running: None,
            focus: false,
        };
        c.push(Line::Note, tr!("TYPE_A_COMMAND_HELP_LISTS_THEM").into());
        c
    }
}

/// Split a command line on spaces, keeping "quoted parts" whole.
pub fn split(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl Console {
    fn push(&mut self, kind: Line, text: String) {
        self.lines.push_back((kind, text));
        while self.lines.len() > KEEP {
            self.lines.pop_front();
        }
    }

    fn run(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.push(Line::Input, line.to_string());
        if self.history.last().map(String::as_str) != Some(line) {
            self.history.push(line.to_string());
        }
        self.recall = None;
        let mut args = split(line);
        if args.first().is_some_and(|a| a.eq_ignore_ascii_case("hiumod")) {
            args.remove(0);
        }
        match args.first().map(String::as_str) {
            None => self.push(Line::Err, tr!("HIUMOD_ALONE_OPENS_ANOTHER_PANEL_ADD").into()),
            Some("clear" | "cls") => self.lines.clear(),
            Some("help" | "?") => {
                for l in crate::cli::usage().lines() {
                    self.push(Line::Out, l.to_string());
                }
            }
            Some(_) if self.running.is_some() => self.push(Line::Err, tr!("THE_LAST_COMMAND_IS_STILL_RUNNING").into()),
            Some(_) => self.spawn(&args),
        }
    }

    fn spawn(&mut self, args: &[String]) {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => return self.push(Line::Err, trf!("EXECUTABLE_NOT_FOUND", e = e)),
        };
        let mut cmd = Command::new(exe);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console window flashes up over the game.
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return self.push(Line::Err, trf!("COULD_NOT_RUN", e = e)),
        };
        let (tx, rx) = channel();
        for (pipe, kind) in [
            (child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>), Line::Out),
            (child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>), Line::Err),
        ] {
            let Some(pipe) = pipe else { continue };
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(pipe);
                let mut buf = Vec::new();
                while r.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
                    let text = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                    if tx.send((classify(kind, &text), text)).is_err() {
                        break;
                    }
                    buf.clear();
                }
            });
        }
        self.running = Some((child, rx));
    }

    /// Take what the running command printed; note when it ends.
    fn pump(&mut self) {
        let Some((child, rx)) = self.running.as_mut() else { return };
        let mut got = Vec::new();
        while let Ok(l) = rx.try_recv() {
            got.push(l);
        }
        let ended = match child.try_wait() {
            Ok(Some(status)) => Some(status.code()),
            Ok(None) => None,
            Err(_) => Some(None),
        };
        for (k, t) in got {
            self.push(k, t);
        }
        if let Some(code) = ended {
            // What is still in the pipes after it ended.
            if let Some((_, rx)) = self.running.take() {
                while let Ok((k, t)) = rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    self.push(k, t);
                }
            }
            match code {
                Some(0) => {}
                Some(c) => self.push(Line::Err, trf!("DONE_CODE", c = c)),
                None => self.push(Line::Err, tr!("DONE").into()),
            }
        }
    }

    fn stop(&mut self) {
        if let Some((mut child, _)) = self.running.take() {
            let _ = child.kill();
            self.push(Line::Err, tr!("STOPPED").into());
        }
    }

    /// Ask for keyboard focus on the input on the next frame (when the panel opens).
    pub fn focus(&mut self) {
        self.focus = true;
    }

    /// Whether the console's window should take the keyboard now.
    pub fn wants_focus(&self) -> bool {
        self.focus
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.pump();
        if self.running.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.header(ui);
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let row = ui.text_style_height(&egui::TextStyle::Monospace);
        let field = row + 2.0 * (FIELD_Y as f32 + 1.0);
        let gap = ui.spacing().item_spacing.y;
        let height = (ui.available_height() - field - gap - 2.0 * LOG_Y as f32).max(60.0);
        egui::Frame::new()
            .fill(SURFACE.gamma_multiply(0.9))
            .corner_radius(R_CARD)
            .inner_margin(egui::Margin::symmetric(LOG_X, LOG_Y))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("console")
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = LINE_GAP;
                        let width = ui.available_width();
                        for (kind, text) in &self.lines {
                            ui.label(layout(*kind, text, &font, width));
                        }
                    });
            });
        // The command line: a field like the panel's (its control well, a hairline edge,
        // the accent edge while it has the keyboard), the prompt glyph and a dim hint.
        let id = ui.id().with("console-field");
        let focused = ui.ctx().memory(|m| m.has_focus(id));
        egui::Frame::new()
            .fill(CONTROL)
            .stroke(egui::Stroke::new(1.0, if focused { ACCENT.gamma_multiply(0.6) } else { EDGE }))
            .corner_radius(R_CONTROL)
            .inner_margin(egui::Margin::symmetric(FIELD_X, FIELD_Y))
            .show(ui, |ui| self.command_line(ui, id, &font));
    }

    /// The header: the title, a pill while a command runs, and the keys as keycaps on
    /// the right, what they do in the hover text.
    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let how = tr!("TYPE_A_COMMAND_HELP_LISTS_THEM");
            ui.label(RichText::new(tr!("CONSOLE")).strong().size(13.5).color(TITLE)).on_hover_text(how);
            if self.running.is_some() {
                pill(ui, tr!("CONSOLE_RUNNING"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = super::theme::TIGHT;
                for key in ["Esc", "↑↓", "help"] {
                    keycap(ui, key).on_hover_text(how);
                }
            });
        });
    }

    /// Inside the field: the prompt glyph (an ellipsis while a command runs), the line
    /// typed, and its keys: Enter runs, ↑/↓ recall, Esc stops.
    fn command_line(&mut self, ui: &mut egui::Ui, id: egui::Id, font: &egui::FontId) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = super::theme::INLINE;
            let (glyph, colour) = if self.running.is_some() { ("…", DIM) } else { (PROMPT, ACCENT) };
            ui.label(RichText::new(glyph).font(font.clone()).color(colour));
            let edit = egui::TextEdit::singleline(&mut self.input)
                .id(id)
                .font(font.clone())
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .text_color(TEXT)
                .desired_width(f32::INFINITY)
                .hint_text(RichText::new(tr!("COMMAND_HELP")).font(font.clone()).color(DIM));
            let r = ui.add(edit);
            // The panel's own key opens the panel, not a character to type.
            self.input.retain(|c| c != '`');
            if self.focus {
                r.request_focus();
                if r.has_focus() {
                    self.focus = false;
                }
            }
            if r.has_focus() {
                let (up, down, esc) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowUp),
                        i.key_pressed(egui::Key::ArrowDown),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                if up && !self.history.is_empty() {
                    let i = self.recall.map_or(self.history.len() - 1, |i| i.saturating_sub(1));
                    self.recall = Some(i);
                    self.input = self.history[i].clone();
                }
                if down {
                    match self.recall {
                        Some(i) if i + 1 < self.history.len() => {
                            self.recall = Some(i + 1);
                            self.input = self.history[i + 1].clone();
                        }
                        _ => {
                            self.recall = None;
                            self.input.clear();
                        }
                    }
                }
                if esc {
                    self.stop();
                }
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let line = std::mem::take(&mut self.input);
                self.run(&line);
                r.request_focus();
            }
        });
    }
}

/// One line of the log, wrapping at `width`: a command after the prompt glyph in the
/// accent; anything else in its kind's colour, its leading meta dim.
fn layout(kind: Line, text: &str, font: &egui::FontId, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    let format = |colour: Color32| egui::TextFormat { font_id: font.clone(), color: colour, ..Default::default() };
    if kind == Line::Input {
        job.append(&format!("{PROMPT} {text}"), 0.0, format(ACCENT));
        return job;
    }
    let (tag, rest) = meta(text);
    if !tag.is_empty() {
        job.append(tag, 0.0, format(DIM));
    }
    job.append(rest, 0.0, format(kind.colour()));
    job
}

/// A state pill as tw.rs draws one (`Tone::Accent`): the accent's text on a faint wash.
fn pill(ui: &mut egui::Ui, text: &str) -> egui::Response {
    egui::Frame::new()
        .fill(ACCENT.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.35)))
        .corner_radius(R_CHIP)
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| ui.label(RichText::new(text).small().color(ACCENT)))
        .response
}

/// A key as a keycap, as tw.rs draws one, quieter: a hint, not a control.
fn keycap(ui: &mut egui::Ui, key: &str) -> egui::Response {
    egui::Frame::new()
        .fill(CONTROL)
        .stroke(egui::Stroke::new(1.0, EDGE))
        .corner_radius(R_CONTROL)
        .inner_margin(egui::Margin::symmetric(7, 1))
        .show(ui, |ui| ui.label(RichText::new(key).monospace().small().color(DIM)))
        .response
}

/// A command still running when the panel closes is stopped with it.
impl Drop for Console {
    fn drop(&mut self) {
        if let Some((mut child, _)) = self.running.take() {
            let _ = child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_split_on_spaces_but_not_in_quotes() {
        assert_eq!(split("doctor inspect hero 1"), ["doctor", "inspect", "hero", "1"]);
        assert_eq!(split(r#"doctor find "Max Speed""#), ["doctor", "find", "Max Speed"]);
        assert!(split("   ").is_empty());
    }

    #[test]
    fn log_lines_are_told_apart_by_pipe_and_prefix() {
        assert!(classify(Line::Err, "[hiumod] reading") == Line::Out);
        assert!(classify(Line::Err, "[hiumod] WARN slow") == Line::Warn);
        assert!(classify(Line::Out, "[hiumod] WARN slow") == Line::Warn);
        assert!(classify(Line::Err, "panicked") == Line::Err);
    }

    #[test]
    fn leading_tags_are_meta() {
        assert_eq!(meta("[hiumod] WARN slow"), ("[hiumod] ", "WARN slow"));
        assert_eq!(meta("[12:00:01] [hiumod] ok"), ("[12:00:01] [hiumod] ", "ok"));
        assert_eq!(meta("plain [x]"), ("", "plain [x]"));
        assert_eq!(meta("[unclosed"), ("", "[unclosed"));
    }
}
