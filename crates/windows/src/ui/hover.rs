use std::cell::Cell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{WM_MOUSEMOVE, WM_NCDESTROY};

const SUBCLASS_ID: usize = 0x5049_5448;

thread_local! {
    static HOT: Cell<isize> = const { Cell::new(0) };
}

pub fn is_hot(hwnd: HWND) -> bool {
    HOT.with(Cell::get) == hwnd.0 as isize
}

fn set_hot(hwnd: HWND, hot: bool) {
    let next = if hot { hwnd.0 as isize } else { 0 };
    let previous = HOT.with(|cell| cell.replace(next));
    for changed in [previous, next].into_iter().filter(|window| *window != 0) {
        unsafe {
            let _ = InvalidateRect(Some(HWND(changed as _)), None, false);
        }
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
    match message {
        WM_MOUSEMOVE if !is_hot(hwnd) => {
            set_hot(hwnd, true);
            let mut request = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut request);
            }
        }
        WM_MOUSELEAVE if is_hot(hwnd) => set_hot(hwnd, false),
        WM_NCDESTROY => {
            if is_hot(hwnd) {
                HOT.with(|cell| cell.set(0));
            }
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID);
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn attach(hwnd: HWND) {
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID, 0);
    }
}
