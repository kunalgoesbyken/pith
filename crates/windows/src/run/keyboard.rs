use std::time::Instant;

use pith_core::{Keymap, PressQueue};
use windows::Win32::Foundation::HWND;

use super::{App, TIMER_REARM};
use crate::input::keys::{Binding, Hotkeys, Keys, replay};

const REARM_MS: u32 = 60;

type Press = pith_core::Press<()>;

pub struct Keyboard {
    keys: Keys<Hotkeys>,
    global: bool,
    queue: PressQueue<()>,
}

impl Keyboard {
    pub fn new(keymap: &Keymap, control: HWND, global: bool) -> Self {
        Self {
            keys: Keys::new(keymap, Hotkeys(control)),
            global,
            queue: PressQueue::default(),
        }
    }

    pub fn arm(&mut self) {
        if self.global {
            self.keys.arm();
        }
    }

    pub fn disarm(&mut self) {
        self.keys.disarm();
    }

    pub fn set_trigger(&mut self, on: bool) {
        if self.global {
            self.keys.set_trigger(on);
        }
    }

    pub fn rebind(&mut self, keymap: &Keymap) {
        self.keys.rebind(keymap);
    }

    pub fn stop(&mut self) {
        self.keys.disarm();
        self.keys.set_trigger(false);
    }

    pub fn serves(&self, served: Option<u64>) -> bool {
        self.queue.serves(served)
    }
}

impl App {
    pub(super) fn hotkey_pressed(&mut self, id: i32) {
        let Some(Binding { action, chord }) = self.keyboard.keys.binding(id) else {
            return;
        };
        self.hotkey(Press {
            action,
            chord,
            at: Instant::now(),
            origin: (),
        });
    }

    fn hotkey(&mut self, press: Press) {
        if self.keyboard.queue.passes_through(&press, &self.engine) {
            self.pass_through(press);
            return;
        }
        let read = self.keyboard.queue.push(press);
        self.reader.request(read);
    }

    pub(super) fn release_keys(&mut self) {
        for press in self.keyboard.queue.take() {
            if press.handled_by(&self.engine) {
                self.dispatch(press.action.event());
            } else {
                self.pass_through(press);
            }
        }
    }

    fn pass_through(&mut self, press: Press) {
        if !self.keyboard.global {
            return;
        }
        self.keyboard.disarm();
        replay(&press.chord);
        if self.engine.is_visible() {
            self.set_timer(TIMER_REARM, REARM_MS);
        }
    }

    pub(super) fn rearm(&mut self) {
        self.kill_timer(TIMER_REARM);
        if self.engine.is_visible() {
            self.keyboard.arm();
        }
    }
}
