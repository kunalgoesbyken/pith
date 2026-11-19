use std::time::Instant;

use crate::chord::{Chord, Key};
use crate::config::Keymap;
use crate::engine::{Engine, Event};
use crate::log::log;

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

pub trait Registrar {
    fn register(&self, id: i32, chord: &Chord) -> bool;
    fn unregister(&self, id: i32);
}

fn is_wanted(binding: &Binding, armed: bool, trigger_on: bool) -> bool {
    if binding.action == KeyAction::Trigger {
        trigger_on
    } else {
        armed
    }
}

struct Slot {
    binding: Binding,
    registered: bool,
}

fn slots(keymap: &Keymap) -> Vec<Slot> {
    bindings(keymap)
        .into_iter()
        .map(|binding| Slot {
            binding,
            registered: false,
        })
        .collect()
}

fn hotkey_id(index: usize) -> i32 {
    index as i32 + 1
}

pub struct Keys<R: Registrar> {
    registrar: R,
    slots: Vec<Slot>,
    armed: bool,
    trigger_on: bool,
}

impl<R: Registrar> Keys<R> {
    pub fn new(keymap: &Keymap, registrar: R) -> Self {
        Self {
            registrar,
            slots: slots(keymap),
            armed: false,
            trigger_on: false,
        }
    }

    pub fn binding(&self, id: i32) -> Option<Binding> {
        (0..self.slots.len())
            .find(|index| hotkey_id(*index) == id)
            .map(|index| self.slots[index].binding)
    }

    fn reconcile(&mut self) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let wanted = is_wanted(&slot.binding, self.armed, self.trigger_on);
            if wanted == slot.registered {
                continue;
            }
            if !wanted {
                self.registrar.unregister(hotkey_id(index));
                slot.registered = false;
            } else if self
                .registrar
                .register(hotkey_id(index), &slot.binding.chord)
            {
                slot.registered = true;
            } else {
                log(&format!(
                    "hotkey {:?} is taken by another program",
                    slot.binding.chord
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
        let unchanged = bindings(keymap)
            .into_iter()
            .eq(self.slots.iter().map(|slot| slot.binding));
        if unchanged {
            return;
        }
        let (armed, trigger_on) = (self.armed, self.trigger_on);
        self.armed = false;
        self.trigger_on = false;
        self.reconcile();
        self.slots = slots(keymap);
        self.armed = armed;
        self.trigger_on = trigger_on;
        self.reconcile();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Press<T> {
    pub action: KeyAction,
    pub chord: Chord,
    pub at: Instant,
    pub origin: T,
}

impl<T> Press<T> {
    pub fn handled_by(&self, engine: &Engine) -> bool {
        match self.action {
            KeyAction::Trigger => true,
            KeyAction::Dismiss => engine.is_visible(),
            KeyAction::AcceptAll | KeyAction::AcceptWord => {
                engine.can_accept(self.at, self.chord.key == Key::Right)
            }
        }
    }
}

pub struct PressQueue<T> {
    waiting: Vec<Press<T>>,
    read: u64,
}

impl<T> Default for PressQueue<T> {
    fn default() -> Self {
        Self {
            waiting: Vec::new(),
            read: 0,
        }
    }
}

impl<T> PressQueue<T> {
    pub fn passes_through(&self, press: &Press<T>, engine: &Engine) -> bool {
        press.action != KeyAction::Trigger && !engine.is_visible() && self.waiting.is_empty()
    }

    pub fn push(&mut self, press: Press<T>) -> u64 {
        self.waiting.push(press);
        self.read += 1;
        self.read
    }

    pub fn serves(&self, served: Option<u64>) -> bool {
        served.is_some_and(|id| id >= self.read)
    }

    pub fn take(&mut self) -> Vec<Press<T>> {
        std::mem::take(&mut self.waiting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use crate::config::Config;

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
        Config::default().keymap()
    }

    fn keymap_of(accept: &[&str], word: &[&str], trigger: Option<&str>) -> Keymap {
        Config {
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
