use std::time::{Duration, Instant};

use pith_core::{Chord, Key, Registrar};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub use pith_core::{Binding, KeyAction, Keys};

pub struct Hotkeys(pub HWND);

pub fn virtual_key(key: Key) -> u16 {
    match key {
        Key::Tab => VK_TAB.0,
        Key::Right => VK_RIGHT.0,
        Key::Escape => VK_ESCAPE.0,
        Key::Space => VK_SPACE.0,
        Key::Char(c) => c as u16,
        Key::Function(n) => VK_F1.0 + u16::from(n) - 1,
    }
}

pub fn key_of(code: u16) -> Option<Key> {
    let function_keys = VK_F1.0..VK_F1.0 + 24;
    match code {
        code if code == VK_TAB.0 => Some(Key::Tab),
        code if code == VK_RIGHT.0 => Some(Key::Right),
        code if code == VK_ESCAPE.0 => Some(Key::Escape),
        code if code == VK_SPACE.0 => Some(Key::Space),
        code if function_keys.contains(&code) => Some(Key::Function((code - VK_F1.0 + 1) as u8)),
        code => u8::try_from(code)
            .map(char::from)
            .ok()
            .filter(|key| key.is_ascii_digit() || key.is_ascii_uppercase())
            .map(Key::Char),
    }
}

fn hot_key_modifiers(chord: &Chord) -> HOT_KEY_MODIFIERS {
    let flags = [
        (chord.ctrl, MOD_CONTROL),
        (chord.shift, MOD_SHIFT),
        (chord.alt, MOD_ALT),
        (chord.win, MOD_WIN),
    ];
    flags
        .into_iter()
        .filter(|(held, _)| *held)
        .fold(MOD_NOREPEAT, |all, (_, flag)| {
            HOT_KEY_MODIFIERS(all.0 | flag.0)
        })
}

fn held_modifiers(chord: &Chord) -> Vec<VIRTUAL_KEY> {
    [
        (chord.ctrl, VK_CONTROL),
        (chord.shift, VK_SHIFT),
        (chord.alt, VK_MENU),
        (chord.win, VK_LWIN),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .map(|(_, vk)| vk)
    .collect()
}

impl Registrar for Hotkeys {
    fn register(&self, id: i32, chord: &Chord) -> bool {
        unsafe {
            RegisterHotKey(
                Some(self.0),
                id,
                hot_key_modifiers(chord),
                u32::from(virtual_key(chord.key)),
            )
        }
        .is_ok()
    }

    fn unregister(&self, id: i32) {
        unsafe {
            let _ = UnregisterHotKey(Some(self.0), id);
        }
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

fn is_down(vk: VIRTUAL_KEY) -> bool {
    let state = unsafe { GetAsyncKeyState(i32::from(vk.0)) };
    state < 0
}

pub fn replay(chord: &Chord) {
    let to_press: Vec<VIRTUAL_KEY> = held_modifiers(chord)
        .into_iter()
        .filter(|vk| !is_down(*vk))
        .collect();
    send_chord(&to_press, VIRTUAL_KEY(virtual_key(chord.key)));
}

pub fn wait_for_modifiers_released(limit: Duration) -> bool {
    let started = Instant::now();
    loop {
        if ![VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .into_iter()
            .any(is_down)
        {
            return true;
        }
        if started.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
