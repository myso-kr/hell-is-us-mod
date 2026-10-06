//! The player's own input, which stops a take the mod moves: a key or button newly pressed, or the
//! camera turned by the mouse away from where the take put it. Not Windows' last-input time: that
//! is touched by anything — a virtual device's driver, a mouse's sensor on a trembling desk — and
//! stopped takes no one had touched.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

/// How far the mouse must turn the camera, summed as it fades over `FADE` (degrees, s).
const TURNED: f32 = 3.0;
const FADE: f32 = 0.5;

/// Why a take stopped for the player.
pub(super) enum Touch {
    Key(u8),
    Turned(f32),
}

pub(super) struct Watch {
    /// Keys down when the take began (the one that started it): not counted until let go.
    held: [bool; 256],
    turned: f32,
}

fn down(vk: u8) -> bool {
    // SAFETY: a plain state query.
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

impl Watch {
    pub fn new() -> Watch {
        let mut held = [false; 256];
        for vk in 1..=254u8 {
            held[vk as usize] = down(vk);
        }
        Watch { held, turned: 0.0 }
    }

    /// The player's touch this tick, if any: `drift` how far the camera is from where the take
    /// last put it (degrees).
    pub fn touch(&mut self, drift: f32, dt: f32) -> Option<Touch> {
        for vk in 1..=254u8 {
            let d = down(vk);
            if self.held[vk as usize] {
                self.held[vk as usize] = d;
            } else if d {
                return Some(Touch::Key(vk));
            }
        }
        self.turned = self.turned * (1.0 - dt / FADE).max(0.0) + drift;
        (self.turned > TURNED).then_some(Touch::Turned(self.turned))
    }
}
