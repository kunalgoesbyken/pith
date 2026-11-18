use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub const ACCEPT_ALL: i32 = 1;
pub const ACCEPT_WORD: i32 = 2;
pub const DISMISS: i32 = 3;

const BINDINGS: [(i32, HOT_KEY_MODIFIERS, VIRTUAL_KEY); 3] = [
    (ACCEPT_ALL, MOD_NOREPEAT, VK_TAB),
    (
        ACCEPT_WORD,
        HOT_KEY_MODIFIERS(MOD_SHIFT.0 | MOD_NOREPEAT.0),
        VK_TAB,
    ),
    (DISMISS, MOD_NOREPEAT, VK_ESCAPE),
];

pub struct Keys {
    armed: bool,
}

impl Keys {
    pub fn new() -> Self {
        Self { armed: false }
    }

    pub fn arm(&mut self, hwnd: HWND) {
        if self.armed {
            return;
        }
        for (id, modifiers, key) in BINDINGS {
            unsafe {
                let _ = RegisterHotKey(Some(hwnd), id, modifiers, key.0 as u32);
            }
        }
        self.armed = true;
    }

    pub fn disarm(&mut self, hwnd: HWND) {
        if !self.armed {
            return;
        }
        for (id, _, _) in BINDINGS {
            unsafe {
                let _ = UnregisterHotKey(Some(hwnd), id);
            }
        }
        self.armed = false;
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn send_chord(modifiers: &[VIRTUAL_KEY], vk: VIRTUAL_KEY) {
    let mut inputs: Vec<INPUT> = modifiers.iter().map(|m| key(*m, false)).collect();
    inputs.push(key(vk, false));
    inputs.push(key(vk, true));
    inputs.extend(modifiers.iter().rev().map(|m| key(*m, true)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn replay(id: i32) {
    match id {
        ACCEPT_ALL => send_chord(&[], VK_TAB),
        ACCEPT_WORD => send_chord(&[VK_SHIFT], VK_TAB),
        DISMISS => send_chord(&[], VK_ESCAPE),
        _ => {}
    }
}

pub fn wait_for_modifiers_released(limit: Duration) {
    let started = Instant::now();
    let held = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
    while started.elapsed() < limit
        && [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .into_iter()
            .any(held)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
}
