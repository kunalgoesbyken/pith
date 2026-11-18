use std::time::{Duration, Instant};

use pith_core::{Chord, Event, Key, Keymap, log};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    AcceptAll,
    AcceptWord,
    Dismiss,
    Trigger,
}

impl KeyAction {
    pub fn event(self) -> Event {
        match self {
            KeyAction::AcceptAll => Event::AcceptAll,
            KeyAction::AcceptWord => Event::AcceptWord,
            KeyAction::Dismiss => Event::Dismiss,
            KeyAction::Trigger => Event::Trigger,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub action: KeyAction,
    pub chord: Chord,
}

fn bindings(keymap: &Keymap) -> Vec<Binding> {
    let accept_all = keymap.accept_all.iter().map(|c| (KeyAction::AcceptAll, *c));
    let accept_word = keymap
        .accept_word
        .iter()
        .map(|c| (KeyAction::AcceptWord, *c));
    let trigger = keymap.trigger.iter().map(|c| (KeyAction::Trigger, *c));
    accept_all
        .chain(accept_word)
        .chain([(KeyAction::Dismiss, Chord::ESCAPE)])
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

pub trait Registrar {
    fn register(&self, id: i32, chord: &Chord) -> bool;
    fn unregister(&self, id: i32);
}

impl Registrar for HWND {
    fn register(&self, id: i32, chord: &Chord) -> bool {
        unsafe {
            RegisterHotKey(
                Some(*self),
                id,
                hot_key_modifiers(chord),
                u32::from(virtual_key(chord.key)),
            )
        }
        .is_ok()
    }

    fn unregister(&self, id: i32) {
        unsafe {
            let _ = UnregisterHotKey(Some(*self), id);
        }
    }
}

fn is_wanted(binding: &Binding, armed: bool, trigger_on: bool) -> bool {
    if binding.action == KeyAction::Trigger {
        trigger_on
    } else {
        armed
    }
}

pub struct Keys<R: Registrar> {
    registrar: R,
    bindings: Vec<Binding>,
    registered: Vec<bool>,
    armed: bool,
    trigger_on: bool,
}

impl<R: Registrar> Keys<R> {
    pub fn new(keymap: &Keymap, registrar: R) -> Self {
        let bindings = bindings(keymap);
        Self {
            registrar,
            registered: vec![false; bindings.len()],
            bindings,
            armed: false,
            trigger_on: false,
        }
    }

    pub fn binding(&self, id: i32) -> Option<Binding> {
        let index = usize::try_from(id).ok()?.checked_sub(1)?;
        self.bindings.get(index).copied()
    }

    fn reconcile(&mut self) {
        for (index, binding) in self.bindings.iter().enumerate() {
            let id = index as i32 + 1;
            let wanted = is_wanted(binding, self.armed, self.trigger_on);
            if wanted == self.registered[index] {
                continue;
            }
            if !wanted {
                self.registrar.unregister(id);
                self.registered[index] = false;
            } else if self.registrar.register(id, &binding.chord) {
                self.registered[index] = true;
            } else {
                log(&format!(
                    "hotkey {:?} is taken by another program",
                    binding.chord
                ));
            }
        }
    }

    pub fn arm(&mut self) {
        self.armed = true;
        self.reconcile();
    }

    pub fn disarm(&mut self) {
        self.armed = false;
        self.reconcile();
    }

    pub fn set_trigger(&mut self, on: bool) {
        self.trigger_on = on;
        self.reconcile();
    }

    pub fn rebind(&mut self, keymap: &Keymap) {
        let next = bindings(keymap);
        if next == self.bindings {
            return;
        }
        let (armed, trigger_on) = (self.armed, self.trigger_on);
        self.armed = false;
        self.trigger_on = false;
        self.reconcile();
        self.registered = vec![false; next.len()];
        self.bindings = next;
        self.armed = armed;
        self.trigger_on = trigger_on;
        self.reconcile();
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fake {
        held: RefCell<BTreeMap<i32, Chord>>,
        taken: RefCell<Vec<Chord>>,
        calls: RefCell<usize>,
    }

    impl Registrar for &Fake {
        fn register(&self, id: i32, chord: &Chord) -> bool {
            *self.calls.borrow_mut() += 1;
            if self.taken.borrow().contains(chord) {
                return false;
            }
            self.held.borrow_mut().insert(id, *chord).is_none()
        }

        fn unregister(&self, id: i32) {
            *self.calls.borrow_mut() += 1;
            assert!(self.held.borrow_mut().remove(&id).is_some(), "id {id}");
        }
    }

    impl Fake {
        fn chords(&self) -> Vec<Chord> {
            self.held.borrow().values().copied().collect()
        }
    }

    fn keymap() -> Keymap {
        pith_core::Config::default().keymap()
    }

    fn keymap_of(accept: &[&str], word: &[&str], trigger: Option<&str>) -> Keymap {
        pith_core::Config {
            accept_keys: accept.iter().map(|k| chord(k)).collect(),
            accept_word_keys: word.iter().map(|k| chord(k)).collect(),
            force_trigger: trigger.is_some(),
            force_trigger_key: chord(trigger.unwrap_or("Ctrl+Shift+Space")),
            ..Default::default()
        }
        .keymap()
    }

    fn chord(source: &str) -> Chord {
        Chord::parse(source).unwrap()
    }

    #[test]
    fn arming_registers_everything_but_the_trigger() {
        let fake = Fake::default();
        let mut keys = Keys::new(&keymap(), &fake);
        keys.arm();
        assert_eq!(
            fake.chords(),
            [
                chord("Tab"),
                chord("Shift+Tab"),
                chord("Ctrl+Right"),
                Chord::ESCAPE
            ]
        );
        keys.set_trigger(true);
        assert_eq!(fake.chords().len(), 5);
        keys.disarm();
        assert_eq!(fake.chords(), [chord("Ctrl+Shift+Space")]);
        keys.set_trigger(false);
        assert!(fake.chords().is_empty());
        keys.arm();
        let calls = *fake.calls.borrow();
        keys.arm();
        keys.set_trigger(false);
        assert_eq!(*fake.calls.borrow(), calls);
    }

    #[test]
    fn rebinding_moves_registrations_and_keeps_what_was_off_off() {
        let rows = [
            (
                true,
                vec!["Ctrl+J"],
                vec!["Alt+Right"],
                Some("F9"),
                vec![
                    chord("Ctrl+J"),
                    chord("Alt+Right"),
                    Chord::ESCAPE,
                    chord("F9"),
                ],
                4,
                4,
            ),
            (
                false,
                vec!["Ctrl+J"],
                vec![],
                Some("F9"),
                vec![chord("F9")],
                3,
                3,
            ),
            (
                true,
                vec!["Tab"],
                vec![],
                None,
                vec![chord("Tab"), Chord::ESCAPE],
                2,
                2,
            ),
        ];
        for row in rows {
            let (armed, accept, word, trigger, expected, bound, armed_count) = &row;
            let fake = Fake::default();
            let mut keys = Keys::new(&keymap(), &fake);
            if *armed {
                keys.arm();
            }
            keys.set_trigger(true);
            keys.rebind(&keymap_of(accept, word, *trigger));
            assert_eq!(&fake.chords(), expected, "{row:?}");
            for (id, held) in fake.held.borrow().iter() {
                assert_eq!(keys.binding(*id).unwrap().chord, *held, "{row:?}");
            }
            for id in 1..=5 {
                assert_eq!(keys.binding(id).is_some(), id as usize <= *bound, "{row:?}");
            }
            keys.arm();
            assert_eq!(fake.chords().len(), *armed_count, "{row:?}");
            keys.disarm();
            keys.set_trigger(false);
            assert!(fake.chords().is_empty(), "{row:?}");
        }
        let fake = Fake::default();
        let mut keys = Keys::new(&keymap(), &fake);
        keys.arm();
        let calls = *fake.calls.borrow();
        keys.rebind(&keymap());
        assert_eq!(*fake.calls.borrow(), calls);
    }

    #[test]
    fn a_taken_chord_is_retried_and_never_released_unregistered() {
        let fake = Fake::default();
        fake.taken.borrow_mut().push(chord("Tab"));
        let mut keys = Keys::new(&keymap(), &fake);
        keys.arm();
        assert!(!fake.chords().contains(&chord("Tab")));
        keys.disarm();
        assert!(fake.chords().is_empty());
        fake.taken.borrow_mut().clear();
        keys.arm();
        assert!(fake.chords().contains(&chord("Tab")));
    }
}
