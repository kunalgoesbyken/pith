use std::time::{Duration, Instant};

use pith_core::{Binding, Keymap, Keys, PressQueue};

use super::App;
use crate::held::Held;
use crate::x11::{Grabs, Stroke, Verdict};

const KEY_DEADLINE: Duration = Duration::from_millis(150);
pub(super) const HOLD_LIMIT: Duration = Duration::from_millis(600);
const KEY_POLL: Duration = Duration::from_millis(30);
const KEY_POLL_CAP: Duration = Duration::from_millis(1500);

type Press = pith_core::Press<Stroke>;

pub(super) struct Keyboard {
    pub(super) keys: Keys<Grabs>,
    grabs: Grabs,
    held: Held,
    queue: PressQueue<Stroke>,
    pressed: Option<(Stroke, Instant)>,
    holding: Option<Stroke>,
}

impl Keyboard {
    pub(super) fn new(keymap: &Keymap, grabs: Grabs, held: Held) -> Self {
        Self {
            keys: Keys::new(keymap, grabs.clone()),
            grabs,
            held,
            queue: PressQueue::default(),
            pressed: None,
            holding: None,
        }
    }

    pub(super) fn stop(&mut self) {
        self.keys.disarm();
        self.keys.set_trigger(false);
    }

    pub(super) fn serves(&self, served: Option<u64>) -> bool {
        self.queue.serves(served)
    }

    fn release(&self, verdict: Verdict, time: u32) {
        self.held.end();
        self.grabs.release(verdict, time);
    }

    fn hold(&mut self, stroke: Stroke) {
        self.pressed = Some((stroke, Instant::now()));
        self.holding = Some(stroke);
    }

    pub(super) fn let_go(&mut self) {
        if let Some(stroke) = self.holding.take() {
            self.release(Verdict::Consume, stroke.time);
        }
    }

    fn is_repeat(&self, stroke: Stroke) -> bool {
        self.pressed
            .is_some_and(|(last, _)| last.keycode == stroke.keycode)
    }

    fn still_down(&mut self) -> bool {
        let Some((stroke, since)) = self.pressed else {
            return false;
        };
        if self.grabs.is_down(stroke.keycode) && since.elapsed() < KEY_POLL_CAP {
            return true;
        }
        self.pressed = None;
        false
    }
}

impl App {
    pub(super) fn key_down(&mut self, stroke: Stroke) {
        if self.keyboard.is_repeat(stroke) {
            self.keyboard.hold(stroke);
            self.keyboard.let_go();
            self.timers.start_key_poll(KEY_POLL);
            return;
        }
        let binding = self
            .keyboard
            .grabs
            .resolve(stroke)
            .and_then(|id| self.keyboard.keys.binding(id));
        let Some(Binding { action, chord }) = binding else {
            self.keyboard.release(Verdict::Replay, stroke.time);
            return;
        };
        let press = Press {
            action,
            chord,
            at: Instant::now(),
            origin: stroke,
        };
        if self.keyboard.queue.passes_through(&press, &self.engine) {
            self.keyboard.release(Verdict::Replay, stroke.time);
            return;
        }
        let read = self.keyboard.queue.push(press);
        self.timers.start_key_deadline(KEY_DEADLINE);
        self.reader.request(read);
    }

    pub(super) fn release_keys(&mut self) {
        self.timers.clear_key_deadline();
        for press in self.keyboard.queue.take() {
            if press.handled_by(&self.engine) {
                self.keyboard.hold(press.origin);
                self.timers.start_key_poll(KEY_POLL);
                self.dispatch(press.action.event());
                if self.effects.inserting(HOLD_LIMIT) {
                    self.timers.start_key_deadline(HOLD_LIMIT);
                } else {
                    self.keyboard.let_go();
                }
            } else {
                self.keyboard.release(Verdict::Replay, press.origin.time);
            }
        }
    }

    pub(super) fn poll_key(&mut self) {
        if self.keyboard.still_down() {
            self.timers.start_key_poll(KEY_POLL);
        }
    }

    pub(super) fn fail_open(&mut self) {
        self.keyboard.let_go();
        for press in self.keyboard.queue.take() {
            self.keyboard.release(Verdict::Replay, press.origin.time);
        }
    }
}
