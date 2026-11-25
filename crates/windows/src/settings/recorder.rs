use pith_core::Chord;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    DLGC_WANTALLKEYS, GetDlgCtrlID, GetParent, PostMessageW, WM_CHAR, WM_DEADCHAR, WM_GETDLGCODE,
    WM_KEYDOWN, WM_KEYUP, WM_NCDESTROY, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::action::Action;
use super::model::read;
use crate::control::messages::WM_RECORDED;
use crate::input::keys::key_of;

const SUBCLASS_ID: usize = 0x5245_434F;
const CONTROL: u32 = 1;
const SHIFT: u32 = 2;
const ALT: u32 = 4;
const MODIFIER_SHIFT: u32 = 16;
const KEY_MASK: u32 = 0xFFFF;
const MODIFIERS: [VIRTUAL_KEY; 11] = [
    VK_CONTROL,
    VK_LCONTROL,
    VK_RCONTROL,
    VK_SHIFT,
    VK_LSHIFT,
    VK_RSHIFT,
    VK_MENU,
    VK_LMENU,
    VK_RMENU,
    VK_LWIN,
    VK_RWIN,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pressed {
    pub key: u16,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

pub fn unpack(packed: LPARAM) -> Pressed {
    let bits = packed.0 as u32;
    let modifiers = bits >> MODIFIER_SHIFT;
    Pressed {
        key: (bits & KEY_MASK) as u16,
        ctrl: modifiers & CONTROL != 0,
        shift: modifiers & SHIFT != 0,
        alt: modifiers & ALT != 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    Cancel,
    Clear,
    Chord(Chord),
    Unusable,
}

pub fn interpret(packed: LPARAM) -> Recorded {
    let pressed = unpack(packed);
    let plain = !(pressed.ctrl || pressed.shift || pressed.alt);
    match (pressed.key, plain) {
        (key, true) if key == VK_ESCAPE.0 => Recorded::Cancel,
        (key, true) if key == VK_BACK.0 => Recorded::Clear,
        (key, _) => match key_of(key) {
            Some(key) => Recorded::Chord(Chord {
                ctrl: pressed.ctrl,
                shift: pressed.shift,
                alt: pressed.alt,
                win: false,
                key,
            }),
            None => Recorded::Unusable,
        },
    }
}

fn held(key: VIRTUAL_KEY) -> bool {
    let state = unsafe { GetKeyState(i32::from(key.0)) };
    state < 0
}

fn is_modifier(key: u16) -> bool {
    MODIFIERS.iter().any(|modifier| modifier.0 == key)
}

fn is_recording(hwnd: HWND) -> bool {
    let id = unsafe { GetDlgCtrlID(hwnd) };
    read(|model| {
        let action = model.layout.control(id).map(|placed| &placed.item.action);
        matches!(action, Some(Action::Record(binding, slot)) if model.recording == Some((*binding, *slot)))
    })
    .unwrap_or(false)
}

fn post_key(hwnd: HWND, key: u16) {
    let Ok(parent) = (unsafe { GetParent(hwnd) }) else {
        return;
    };
    let modifiers = [
        (held(VK_CONTROL), CONTROL),
        (held(VK_SHIFT), SHIFT),
        (held(VK_MENU), ALT),
    ]
    .into_iter()
    .filter(|(down, _)| *down)
    .fold(0, |all, (_, bit)| all | bit);
    let packed = u32::from(key) | modifiers << MODIFIER_SHIFT;
    let id = unsafe { GetDlgCtrlID(hwnd) } as usize;
    unsafe {
        let _ = PostMessageW(
            Some(parent),
            WM_RECORDED,
            WPARAM(id),
            LPARAM(packed as isize),
        );
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID);
        }
    } else if is_recording(hwnd) {
        match message {
            WM_GETDLGCODE => return LRESULT(DLGC_WANTALLKEYS as isize),
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let key = wparam.0 as u16;
                if !is_modifier(key) {
                    post_key(hwnd, key);
                }
                return LRESULT(0);
            }
            WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR | WM_DEADCHAR => return LRESULT(0),
            _ => {}
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn attach(hwnd: HWND) {
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID, 0);
    }
}
