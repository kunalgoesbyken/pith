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
    if binding.action == TRIGGER {
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
    fn default_bindings_cover_every_action_once_per_chord() {
        let all = bindings(&keymap());
        let actions: Vec<i32> = all.iter().map(|b| b.action).collect();
        assert_eq!(
            actions,
            [ACCEPT_ALL, ACCEPT_WORD, ACCEPT_WORD, DISMISS, TRIGGER]
        );
        let fake = Fake::default();
        let keys = Keys::new(&keymap(), &fake);
        for (index, binding) in all.iter().enumerate() {
            assert_eq!(keys.binding(index as i32 + 1), Some(*binding));
        }
    }

    #[test]
    fn unknown_registration_ids_have_no_binding() {
        let fake = Fake::default();
        let keys = Keys::new(&keymap(), &fake);
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
        let all = bindings(&config.keymap());
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
            assert_eq!(
                virtual_key(Chord::parse(source).unwrap().key),
                vk,
                "{source}"
            );
        }
        let plain = Chord::parse("Tab").unwrap();
        assert_eq!(hot_key_modifiers(&plain), MOD_NOREPEAT);
        assert!(held_modifiers(&plain).is_empty());
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
    }

    #[test]
    fn repeated_arming_does_not_touch_the_system_again() {
        let fake = Fake::default();
        let mut keys = Keys::new(&keymap(), &fake);
        keys.arm();
        let calls = *fake.calls.borrow();
        keys.arm();
        keys.set_trigger(false);
        assert_eq!(*fake.calls.borrow(), calls);
    }

    #[test]
    fn rebinding_while_armed_moves_the_registrations_to_the_new_keys() {
        let fake = Fake::default();
        let mut keys = Keys::new(&keymap(), &fake);
        keys.arm();
        keys.set_trigger(true);
        keys.rebind(&keymap_of(&["Ctrl+J"], &["Alt+Right"], Some("F9")));
        assert_eq!(
            fake.chords(),
            [
                chord("Ctrl+J"),
                chord("Alt+Right"),
                Chord::ESCAPE,
                chord("F9")
            ]
        );
        for (id, held) in fake.held.borrow().iter() {
            assert_eq!(keys.binding(*id).unwrap().chord, *held);
        }
        keys.disarm();
        keys.set_trigger(false);
        assert!(fake.chords().is_empty());
    }

    #[test]
    fn rebinding_keeps_what_was_off_off() {
        let fake = Fake::default();
        let mut keys = Keys::new(&keymap(), &fake);
        keys.set_trigger(true);
        keys.rebind(&keymap_of(&["Ctrl+J"], &[], Some("F9")));
        assert_eq!(fake.chords(), [chord("F9")]);
        keys.arm();
        assert_eq!(fake.chords().len(), 3);
    }

    #[test]
    fn rebinding_to_a_smaller_map_releases_the_surplus_ids() {
        let fake = Fake::default();
        let mut keys = Keys::new(&keymap(), &fake);
        keys.arm();
        keys.set_trigger(true);
        keys.rebind(&keymap_of(&["Tab"], &[], None));
        assert_eq!(fake.chords(), [chord("Tab"), Chord::ESCAPE]);
        assert_eq!(keys.binding(3), None);
    }

    #[test]
    fn rebinding_to_the_same_keys_is_a_no_op() {
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
