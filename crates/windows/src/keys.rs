use std::time::{Duration, Instant};

use pith_core::{Chord, Key, Keymap, log};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub const ACCEPT_ALL: i32 = 1;
pub const ACCEPT_WORD: i32 = 2;
pub const DISMISS: i32 = 3;
pub const TRIGGER: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub action: i32,
    pub chord: Chord,
}

fn bindings(keymap: &Keymap) -> Vec<Binding> {
    let accept_all = keymap.accept_all.iter().map(|c| (ACCEPT_ALL, *c));
    let accept_word = keymap.accept_word.iter().map(|c| (ACCEPT_WORD, *c));
    let trigger = keymap.trigger.iter().map(|c| (TRIGGER, *c));
    accept_all
        .chain(accept_word)
        .chain([(DISMISS, Chord::ESCAPE)])
        .chain(trigger)
        .map(|(action, chord)| Binding { action, chord })
        .collect()
}

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

pub struct Keys {
    bindings: Vec<Binding>,
    armed: bool,
    trigger_registered: bool,
}

impl Keys {
    pub fn new(keymap: &Keymap) -> Self {
        Self {
            bindings: bindings(keymap),
            armed: false,
            trigger_registered: false,
        }
    }

    pub fn binding(&self, id: i32) -> Option<Binding> {
        let index = usize::try_from(id).ok()?.checked_sub(1)?;
        self.bindings.get(index).copied()
    }

    fn register(&self, hwnd: HWND, wanted: impl Fn(&Binding) -> bool) {
        for (index, binding) in self.bindings.iter().enumerate().filter(|(_, b)| wanted(b)) {
            let id = index as i32 + 1;
            let registered = unsafe {
                RegisterHotKey(
                    Some(hwnd),
                    id,
                    hot_key_modifiers(&binding.chord),
                    u32::from(virtual_key(binding.chord.key)),
                )
            };
            if registered.is_err() {
                log(&format!("hotkey {:?} is taken by another program", binding.chord));
            }
        }
    }

    fn unregister(&self, hwnd: HWND, wanted: impl Fn(&Binding) -> bool) {
        for (index, _) in self.bindings.iter().enumerate().filter(|(_, b)| wanted(b)) {
            unsafe {
                let _ = UnregisterHotKey(Some(hwnd), index as i32 + 1);
            }
        }
    }

    pub fn arm(&mut self, hwnd: HWND) {
        if self.armed {
            return;
        }
        self.register(hwnd, |b| b.action != TRIGGER);
        self.armed = true;
    }

    pub fn disarm(&mut self, hwnd: HWND) {
        if !self.armed {
            return;
        }
        self.unregister(hwnd, |b| b.action != TRIGGER);
        self.armed = false;
    }

    pub fn set_trigger(&mut self, hwnd: HWND, on: bool) {
        if self.trigger_registered == on {
            return;
        }
        if on {
            self.register(hwnd, |b| b.action == TRIGGER);
        } else {
            self.unregister(hwnd, |b| b.action == TRIGGER);
        }
        self.trigger_registered = on;
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

pub fn replay(chord: &Chord) {
    send_chord(&held_modifiers(chord), VIRTUAL_KEY(virtual_key(chord.key)));
}

pub fn wait_for_modifiers_released(limit: Duration) -> bool {
    let started = Instant::now();
    let held = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
    loop {
        if ![VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .into_iter()
            .any(held)
        {
            return true;
        }
        if started.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keymap() -> Keymap {
        pith_core::Config::default().keymap().unwrap()
    }

    #[test]
    fn default_bindings_cover_every_action_once_per_chord() {
        let all = bindings(&keymap());
        let actions: Vec<i32> = all.iter().map(|b| b.action).collect();
        assert_eq!(actions, [ACCEPT_ALL, ACCEPT_WORD, ACCEPT_WORD, DISMISS, TRIGGER]);
        for (index, binding) in all.iter().enumerate() {
            let keys = Keys::new(&keymap());
            assert_eq!(keys.binding(index as i32 + 1), Some(*binding));
        }
    }

    #[test]
    fn unknown_registration_ids_have_no_binding() {
        let keys = Keys::new(&keymap());
        for id in [0, -1, 6, 99, i32::MAX, i32::MIN] {
            assert_eq!(keys.binding(id), None, "{id}");
        }
    }

    #[test]
    fn disabling_the_trigger_leaves_no_trigger_binding() {
        let config = pith_core::Config {
            force_trigger: false,
            ..Default::default()
        };
        let all = bindings(&config.keymap().unwrap());
        assert!(all.iter().all(|b| b.action != TRIGGER));
    }

    #[test]
    fn custom_chords_map_to_virtual_keys_and_modifiers() {
        let chord = Chord::parse("Ctrl+Alt+F5").unwrap();
        assert_eq!(virtual_key(chord.key), VK_F5.0);
        assert_eq!(
            hot_key_modifiers(&chord).0,
            MOD_NOREPEAT.0 | MOD_CONTROL.0 | MOD_ALT.0
        );
        assert_eq!(held_modifiers(&chord), [VK_CONTROL, VK_MENU]);
        for (source, vk) in [
            ("Tab", VK_TAB.0),
            ("Right", VK_RIGHT.0),
            ("Esc", VK_ESCAPE.0),
            ("Ctrl+Space", VK_SPACE.0),
            ("Ctrl+j", 'J' as u16),
            ("Alt+7", '7' as u16),
            ("F1", VK_F1.0),
            ("F24", VK_F24.0),
        ] {
            assert_eq!(virtual_key(Chord::parse(source).unwrap().key), vk, "{source}");
        }
        let plain = Chord::parse("Tab").unwrap();
        assert_eq!(hot_key_modifiers(&plain), MOD_NOREPEAT);
        assert!(held_modifiers(&plain).is_empty());
    }
}
