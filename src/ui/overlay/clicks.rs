//! Where the player's clicks go, for the log: a click the game did not get (the window in front, or
//! the one under the pointer, not the game's) is written out with what was there; the others are
//! counted every half a minute. The user found clicks ignored at times (2026-10-07).

use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, WindowFromPoint, GA_ROOT,
};

const REPORT: Duration = Duration::from_secs(30);

pub struct Clicks {
    down: bool,
    to_game: u32,
    since: Instant,
}

impl Default for Clicks {
    fn default() -> Clicks {
        Clicks { down: false, to_game: 0, since: Instant::now() }
    }
}

fn pid(w: HWND) -> u32 {
    let mut p = 0;
    // SAFETY: a plain query on any window handle, null included.
    unsafe { GetWindowThreadProcessId(w, &mut p) };
    p
}

fn class(w: HWND) -> String {
    let mut b = [0u16; 64];
    // SAFETY: the buffer's length is passed.
    let n = unsafe { GetClassNameW(w, b.as_mut_ptr(), b.len() as i32) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}

impl Clicks {
    /// One look, `game` the game's pid (0 not running), `filming` whether a take rolls, `cursor`
    /// whether the game shows its cursor (a menu).
    pub fn look(&mut self, game: u32, filming: bool, cursor: bool) {
        if game == 0 {
            return;
        }
        // SAFETY: plain state queries.
        let down = unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } as u16 & 0x8000 != 0;
        let pressed = down && !self.down;
        self.down = down;
        if pressed {
            let mut at = POINT { x: 0, y: 0 };
            // SAFETY: as above.
            let (front, under) = unsafe {
                GetCursorPos(&mut at);
                (GetForegroundWindow(), GetAncestor(WindowFromPoint(at), GA_ROOT))
            };
            if pid(front) == game && pid(under) == game {
                self.to_game += 1;
            } else {
                let who = |w: HWND| {
                    let p = pid(w);
                    let owner = if p == game {
                        "game"
                    } else if p == std::process::id() {
                        "hiumod"
                    } else {
                        "other"
                    };
                    format!("{owner}:{}", class(w))
                };
                crate::logfile::line(&format!(
                    "click not to the game: in front {}, under the pointer {} at ({}, {}); filming {filming}, game cursor {cursor}",
                    who(front),
                    who(under),
                    at.x,
                    at.y
                ));
            }
        }
        if self.since.elapsed() >= REPORT {
            if self.to_game > 0 {
                crate::logfile::line(&format!("clicks to the game in the last 30 s: {}", self.to_game));
            }
            self.to_game = 0;
            self.since = Instant::now();
        }
    }
}
