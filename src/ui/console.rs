//! The console, dropped down over the top of the game while the panel is open (as in
//! Half-Life): the CLI's commands, typed and run without leaving the game — `pose`,
//! `doctor survey`, `doctor inspect hero 1` …
//!
//! A command runs as this same executable with those arguments, in a child process
//! without a window; what it prints comes back line by line as it goes. `help` and
//! `clear` are the console's own. A bare `hiumod` would open a second panel, so it is
//! refused; a leading `hiumod` is dropped. ↑/↓ recall earlier commands, Esc stops the
//! one running.

use eframe::egui::{self, Color32, RichText};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

/// How many lines are kept.
const KEEP: usize = 2000;
/// How much of the game window's height the console takes.
pub const SHARE: f32 = 0.38;

#[derive(Clone, Copy, PartialEq)]
enum Line {
    Input,
    Out,
    Err,
    Note,
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
        c.push(Line::Note, "명령을 입력하세요 — `help` 로 목록, ↑/↓ 이전 명령, Esc 로 실행 중지".into());
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
        self.push(Line::Input, format!("> {line}"));
        if self.history.last().map(String::as_str) != Some(line) {
            self.history.push(line.to_string());
        }
        self.recall = None;
        let mut args = split(line);
        if args.first().is_some_and(|a| a.eq_ignore_ascii_case("hiumod")) {
            args.remove(0);
        }
        match args.first().map(String::as_str) {
            None => self.push(Line::Err, "hiumod 만으로는 패널이 하나 더 열립니다 — 명령을 붙여 주세요 (help)".into()),
            Some("clear" | "cls") => self.lines.clear(),
            Some("help" | "?") => {
                for l in crate::cli::USAGE.lines() {
                    self.push(Line::Out, l.to_string());
                }
            }
            Some(_) if self.running.is_some() => self.push(Line::Err, "앞의 명령이 아직 실행 중입니다 (Esc 로 중지)".into()),
            Some(_) => self.spawn(&args),
        }
    }

    fn spawn(&mut self, args: &[String]) {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => return self.push(Line::Err, format!("실행 파일을 찾지 못함: {e}")),
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
            Err(e) => return self.push(Line::Err, format!("실행하지 못함: {e}")),
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
                    // `log!` lines go to stderr but are not errors.
                    let kind = if kind == Line::Err && text.starts_with("[hiumod]") && !text.contains("WARN") { Line::Out } else { kind };
                    if tx.send((kind, text)).is_err() {
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
                Some(c) => self.push(Line::Err, format!("(끝남: 코드 {c})")),
                None => self.push(Line::Err, "(끝남)".into()),
            }
        }
    }

    fn stop(&mut self) {
        if let Some((mut child, _)) = self.running.take() {
            let _ = child.kill();
            self.push(Line::Err, "(중지함)".into());
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
        let mono = egui::TextStyle::Monospace;
        let height = (ui.available_height() - 34.0).max(60.0);
        egui::ScrollArea::vertical()
            .id_salt("console")
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for (kind, text) in &self.lines {
                    let colour = match kind {
                        Line::Input => Color32::from_rgb(140, 200, 255),
                        Line::Out => Color32::from_gray(210),
                        Line::Err => Color32::from_rgb(255, 150, 120),
                        Line::Note => Color32::from_gray(130),
                    };
                    ui.add(egui::Label::new(RichText::new(text).text_style(mono.clone()).color(colour)).wrap());
                }
            });
        ui.horizontal(|ui| {
            ui.label(RichText::new(if self.running.is_some() { "…" } else { ">" }).text_style(mono.clone()));
            let edit = egui::TextEdit::singleline(&mut self.input)
                .font(mono.clone())
                .desired_width(f32::INFINITY)
                .hint_text("명령 (help)");
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
                    (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::Escape))
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
}
