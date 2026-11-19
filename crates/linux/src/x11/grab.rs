use std::sync::{Arc, Mutex, PoisonError, RwLock};

use pith_core::{Chord, Registrar, log};
use x11rb::connection::Connection;
use x11rb::protocol::xkb::{BoolCtrl, ConnectionExt as _, ID, PerClientFlag};
use x11rb::protocol::xproto::{Allow, ConnectionExt, GrabMode, ModMask};

use super::display::{Display, DisplayError};
use super::keymap::Keyboard;

const XKB_MAJOR: u16 = 1;
const XKB_MINOR: u16 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Consume,
    Replay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub keycode: u8,
    pub state: u16,
    pub time: u32,
}

struct Grabbed {
    id: i32,
    chord: Chord,
    keycode: u8,
    modifiers: u16,
    masks: Vec<u16>,
}

#[derive(Clone)]
pub struct Grabs {
    display: Arc<Display>,
    keyboard: Arc<RwLock<Keyboard>>,
    table: Arc<Mutex<Vec<Grabbed>>>,
}

impl Grabs {
    pub fn new(display: Arc<Display>) -> Result<Self, DisplayError> {
        let keyboard = Keyboard::read(&display)?;
        display
            .connection
            .xkb_use_extension(XKB_MAJOR, XKB_MINOR)?
            .reply()?;
        display
            .connection
            .xkb_per_client_flags(
                u16::from(ID::USE_CORE_KBD),
                PerClientFlag::DETECTABLE_AUTO_REPEAT,
                PerClientFlag::DETECTABLE_AUTO_REPEAT,
                BoolCtrl::from(0u32),
                BoolCtrl::from(0u32),
                BoolCtrl::from(0u32),
            )?
            .reply()?;
        Ok(Self {
            display,
            keyboard: Arc::new(RwLock::new(keyboard)),
            table: Arc::default(),
        })
    }

    pub fn resolve(&self, stroke: Stroke) -> Option<i32> {
        let held = stroke.state
            & self
                .keyboard
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .chord_bits();
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|grabbed| grabbed.keycode == stroke.keycode && grabbed.modifiers == held)
            .map(|grabbed| grabbed.id)
    }

    pub fn is_down(&self, keycode: u8) -> bool {
        self.display
            .connection
            .query_keymap()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| (reply.keys[usize::from(keycode / 8)] >> (keycode % 8)) & 1 == 1)
    }

    pub fn release(&self, verdict: Verdict, time: u32) {
        let connection = &self.display.connection;
        let _ = match verdict {
            Verdict::Consume => connection.ungrab_keyboard(time),
            Verdict::Replay => connection.allow_events(Allow::REPLAY_KEYBOARD, time),
        };
        let _ = connection.flush();
    }

    pub fn remap(&self) {
        let keyboard = match Keyboard::read(&self.display) {
            Ok(keyboard) => keyboard,
            Err(error) => {
                log(&format!("keyboard map not reread: {error}"));
                return;
            }
        };
        *self
            .keyboard
            .write()
            .unwrap_or_else(PoisonError::into_inner) = keyboard;
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let held: Vec<(i32, Chord)> = table
            .drain(..)
            .map(|grabbed| {
                self.withdraw(&grabbed);
                (grabbed.id, grabbed.chord)
            })
            .collect();
        for (id, chord) in held {
            match self.establish(id, chord) {
                Some(grabbed) => table.push(grabbed),
                None => log(&format!("{chord} cannot be grabbed after a keymap change")),
            }
        }
        let _ = self.display.connection.flush();
    }

    fn establish(&self, id: i32, chord: Chord) -> Option<Grabbed> {
        let keyboard = self.keyboard.read().unwrap_or_else(PoisonError::into_inner);
        let keycode = keyboard.keycode(chord.key)?;
        let modifiers = keyboard.chord_mask(&chord)?;
        let masks: Vec<u16> = keyboard
            .lock_variants()
            .into_iter()
            .map(|lock| modifiers | lock)
            .collect();
        drop(keyboard);
        let mut taken = Vec::new();
        for mask in &masks {
            let cookie = self.display.connection.grab_key(
                false,
                self.display.root,
                ModMask::from(*mask),
                keycode,
                GrabMode::ASYNC,
                GrabMode::SYNC,
            );
            if cookie.ok().and_then(|cookie| cookie.check().ok()).is_none() {
                for earlier in taken {
                    self.ungrab(keycode, earlier);
                }
                return None;
            }
            taken.push(*mask);
        }
        Some(Grabbed {
            id,
            chord,
            keycode,
            modifiers,
            masks,
        })
    }

    fn withdraw(&self, grabbed: &Grabbed) {
        for mask in &grabbed.masks {
            self.ungrab(grabbed.keycode, *mask);
        }
    }

    fn ungrab(&self, keycode: u8, mask: u16) {
        let _ = self
            .display
            .connection
            .ungrab_key(keycode, self.display.root, ModMask::from(mask));
    }
}

impl Registrar for Grabs {
    fn register(&self, id: i32, chord: &Chord) -> bool {
        let Some(grabbed) = self.establish(id, *chord) else {
            return false;
        };
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(grabbed);
        let _ = self.display.connection.flush();
        true
    }

    fn unregister(&self, id: i32) {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(index) = table.iter().position(|grabbed| grabbed.id == id) else {
            return;
        };
        self.withdraw(&table.remove(index));
        let _ = self.display.connection.flush();
    }
}
