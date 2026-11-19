use pith_core::{Chord, Key};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, ModMask};

use super::display::{Display, DisplayError};

const KEYSYM_TAB: u32 = 0xff09;
const KEYSYM_RIGHT: u32 = 0xff53;
const KEYSYM_ESCAPE: u32 = 0xff1b;
const KEYSYM_SPACE: u32 = 0x20;
const KEYSYM_FUNCTION_1: u32 = 0xffbe;
const KEYSYM_NUM_LOCK: u32 = 0xff7f;
const KEYSYM_SCROLL_LOCK: u32 = 0xff14;
const KEYSYM_CONTROL_LEFT: u32 = 0xffe3;
const KEYSYMS_ALT: [u32; 2] = [0xffe9, 0xffea];
const KEYSYMS_SUPER: [u32; 2] = [0xffeb, 0xffec];
const KEYSYM_PASTE_LETTER: u32 = 0x76;
const MODIFIER_SLOTS: usize = 8;

pub(crate) struct Keyboard {
    first: u8,
    per_keycode: usize,
    keysyms: Vec<u32>,
    lock_masks: Vec<u16>,
    alt: Option<u16>,
    super_key: Option<u16>,
}

fn keysym(key: Key) -> u32 {
    match key {
        Key::Tab => KEYSYM_TAB,
        Key::Right => KEYSYM_RIGHT,
        Key::Escape => KEYSYM_ESCAPE,
        Key::Space => KEYSYM_SPACE,
        Key::Char(c) => c.to_ascii_lowercase() as u32,
        Key::Function(n) => KEYSYM_FUNCTION_1 + u32::from(n) - 1,
    }
}

fn subsets(masks: &[u16]) -> Vec<u16> {
    masks.iter().fold(vec![0], |all, mask| {
        let with: Vec<u16> = all.iter().map(|existing| existing | mask).collect();
        all.into_iter().chain(with).collect()
    })
}

fn modifier_mask(keycodes: &[u8], keyboard: &Keyboard, wanted: &[u32]) -> Option<u16> {
    let per_modifier = keycodes.len() / MODIFIER_SLOTS;
    let codes: Vec<u8> = wanted
        .iter()
        .filter_map(|keysym| keyboard.keycode_of(*keysym))
        .collect();
    (0..MODIFIER_SLOTS)
        .find(|slot| {
            keycodes[slot * per_modifier..(slot + 1) * per_modifier]
                .iter()
                .any(|keycode| codes.contains(keycode))
        })
        .map(|slot| 1 << slot)
}

impl Keyboard {
    pub(crate) fn read(display: &Display) -> Result<Self, DisplayError> {
        let setup = display.connection.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let mapping = display
            .connection
            .get_keyboard_mapping(first, last - first + 1)?
            .reply()?;
        let mut keyboard = Self {
            first,
            per_keycode: usize::from(mapping.keysyms_per_keycode),
            keysyms: mapping.keysyms,
            lock_masks: vec![u16::from(ModMask::LOCK)],
            alt: None,
            super_key: None,
        };
        let modifiers = display.connection.get_modifier_mapping()?.reply()?;
        let extra: Vec<u16> = [KEYSYM_NUM_LOCK, KEYSYM_SCROLL_LOCK]
            .into_iter()
            .filter_map(|keysym| modifier_mask(&modifiers.keycodes, &keyboard, &[keysym]))
            .collect();
        keyboard.lock_masks.extend(extra);
        keyboard.alt = modifier_mask(&modifiers.keycodes, &keyboard, &KEYSYMS_ALT);
        keyboard.super_key = modifier_mask(&modifiers.keycodes, &keyboard, &KEYSYMS_SUPER);
        Ok(keyboard)
    }

    pub(crate) fn keycode_of(&self, keysym: u32) -> Option<u8> {
        self.keysyms
            .chunks(self.per_keycode)
            .position(|levels| levels.contains(&keysym))
            .and_then(|index| u8::try_from(index).ok())
            .map(|index| self.first + index)
    }

    pub(crate) fn keycode(&self, key: Key) -> Option<u8> {
        self.keycode_of(keysym(key))
    }

    pub(crate) fn control_keycode(&self) -> Option<u8> {
        self.keycode_of(KEYSYM_CONTROL_LEFT)
    }

    pub(crate) fn paste_keycode(&self) -> Option<u8> {
        self.keycode_of(KEYSYM_PASTE_LETTER)
    }

    pub(crate) fn chord_mask(&self, chord: &Chord) -> Option<u16> {
        let mut mask = 0;
        if chord.shift {
            mask |= u16::from(ModMask::SHIFT);
        }
        if chord.ctrl {
            mask |= u16::from(ModMask::CONTROL);
        }
        if chord.alt {
            mask |= self.alt?;
        }
        if chord.win {
            mask |= self.super_key?;
        }
        Some(mask)
    }

    pub(crate) fn chord_bits(&self) -> u16 {
        [
            Some(u16::from(ModMask::SHIFT)),
            Some(u16::from(ModMask::CONTROL)),
            self.alt,
            self.super_key,
        ]
        .into_iter()
        .flatten()
        .fold(0, |all, bit| all | bit)
    }

    pub(crate) fn lock_variants(&self) -> Vec<u16> {
        subsets(&self.lock_masks)
    }
}
