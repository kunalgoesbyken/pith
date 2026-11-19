use std::time::Instant;

use pith_core::{Chord, Key, Keymap};
use windows::Win32::Foundation::HWND;

use super::{App, TIMER_REARM};
use crate::keys::{Binding, KeyAction, Keys, replay};

const REARM_MS: u32 = 60;

#[derive(Clone, Copy)]
struct Press {
    action: KeyAction,
    chord: Chord,
    at: Instant,
}

impl Press {
    fn moves_caret(&self) -> bool {
        self.chord.key == Key::Right
    }
}

pub struct Keyboard {
    keys: Keys<HWND>,
    global: bool,
    awaiting_read: Vec<Press>,
    read: u64,
}

impl Keyboard {
    pub fn new(keymap: &Keymap, control: HWND, global: bool) -> Self {
        Self {
            keys: Keys::new(keymap, control),
            global,
            awaiting_read: Vec::new(),
            read: 0,
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
        served.is_some_and(|id| id >= self.read)
    }

    fn queue(&mut self, press: Press) -> u64 {
        self.awaiting_read.push(press);
        self.read += 1;
        self.read
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
        });
    }

    fn hotkey(&mut self, press: Press) {
        let needs_field = press.action == KeyAction::Trigger;
        if !needs_field && !self.engine.is_visible() && self.keyboard.awaiting_read.is_empty() {
            self.pass_through(press);
            return;
        }
        let read = self.keyboard.queue(press);
        self.reader.request(read);
    }

    pub(super) fn release_keys(&mut self) {
        for press in std::mem::take(&mut self.keyboard.awaiting_read) {
            let handled = match press.action {
                KeyAction::Trigger => true,
                KeyAction::Dismiss => self.engine.is_visible(),
                KeyAction::AcceptAll | KeyAction::AcceptWord => {
                    self.engine.can_accept(press.at, press.moves_caret())
                }
            };
            if handled {
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
