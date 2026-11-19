use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HDC, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::Controls::{
    TBM_GETRANGEMAX, TBM_GETRANGEMIN, TBM_SETPOS, TBM_SETRANGEMAX, TBM_SETRANGEMIN,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR,
    VK_RIGHT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BN_SETFOCUS, CreateWindowExW, DLGC_WANTARROWS, DefWindowProcW, GetClientRect, GetDlgCtrlID,
    GetParent, HMENU, IDC_ARROW, LoadCursorW, RegisterClassW, SendMessageW, WINDOW_EX_STYLE,
    WM_COMMAND, WM_ERASEBKGND, WM_GETDLGCODE, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_PRINTCLIENT, WM_SETFOCUS, WM_USER,
    WNDCLASSW, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

use crate::ui::{BASE_DPI, Scale, height, request_draw, slider_fraction_at, track_hover, width};
use crate::util::wide;

pub const TBM_GETPOS: u32 = WM_USER;
pub const MOVED: u32 = 0x100;
pub const RELEASED: u32 = 0x101;
const CLASS: PCWSTR = w!("PithSlider");
const PAGE_DIVISOR: i32 = 10;

#[derive(Clone, Copy, Default)]
struct Track {
    low: i32,
    high: i32,
    value: i32,
    dragging: bool,
}

thread_local! {
    static TRACKS: RefCell<HashMap<isize, Track>> = RefCell::new(HashMap::new());
}

fn track(hwnd: HWND) -> Track {
    TRACKS.with(|tracks| {
        tracks
            .borrow()
            .get(&(hwnd.0 as isize))
            .copied()
            .unwrap_or_default()
    })
}

fn store(hwnd: HWND, change: impl FnOnce(&mut Track)) {
    TRACKS.with(|tracks| change(tracks.borrow_mut().entry(hwnd.0 as isize).or_default()));
}

fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn notify(hwnd: HWND, code: u32) {
    let Ok(parent) = (unsafe { GetParent(hwnd) }) else {
        return;
    };
    let id = unsafe { GetDlgCtrlID(hwnd) } as usize;
    unsafe {
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM((code as usize) << 16 | id)),
            Some(LPARAM(hwnd.0 as isize)),
        );
    }
}

fn move_to(hwnd: HWND, wanted: i32) {
    let current = track(hwnd);
    let value = wanted.clamp(current.low, current.high);
    if value == current.value {
        return;
    }
    store(hwnd, |track| track.value = value);
    redraw(hwnd);
    notify(hwnd, MOVED);
}

fn value_at(hwnd: HWND, lparam: LPARAM) -> i32 {
    let mut area = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut area);
    }
    let scale = Scale(unsafe { GetDpiForWindow(hwnd) }.max(BASE_DPI));
    let x = i32::from((lparam.0 & 0xFFFF) as i16);
    let Track { low, high, .. } = track(hwnd);
    let fraction = slider_fraction_at(scale, width(area), x);
    low + (fraction * (high - low) as f32).round() as i32
}

fn stepped(hwnd: HWND, key: u16) -> Option<i32> {
    let Track {
        low, high, value, ..
    } = track(hwnd);
    let page = ((high - low) / PAGE_DIVISOR).max(1);
    match key {
        key if key == VK_LEFT.0 || key == VK_DOWN.0 => Some(value - 1),
        key if key == VK_RIGHT.0 || key == VK_UP.0 => Some(value + 1),
        key if key == VK_PRIOR.0 => Some(value + page),
        key if key == VK_NEXT.0 => Some(value - page),
        key if key == VK_HOME.0 => Some(low),
        key if key == VK_END.0 => Some(high),
        _ => None,
    }
}

fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
    request_draw(hwnd, hdc, track(hwnd).dragging);
    unsafe {
        let _ = EndPaint(hwnd, &paint);
    }
}

fn press(hwnd: HWND, lparam: LPARAM) {
    unsafe {
        let _ = SetFocus(Some(hwnd));
        SetCapture(hwnd);
    }
    store(hwnd, |track| track.dragging = true);
    move_to(hwnd, value_at(hwnd, lparam));
}

fn release(hwnd: HWND) {
    if !track(hwnd).dragging {
        return;
    }
    store(hwnd, |track| track.dragging = false);
    unsafe {
        let _ = ReleaseCapture();
    }
    redraw(hwnd);
    notify(hwnd, RELEASED);
}

fn press_key(hwnd: HWND, key: u16) {
    if let Some(value) = stepped(hwnd, key) {
        move_to(hwnd, value);
        notify(hwnd, RELEASED);
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => paint(hwnd),
        WM_PRINTCLIENT => request_draw(hwnd, HDC(wparam.0 as _), false),
        WM_ERASEBKGND => return LRESULT(1),
        WM_GETDLGCODE => return LRESULT(DLGC_WANTARROWS as isize),
        WM_LBUTTONDOWN => press(hwnd, lparam),
        WM_MOUSEMOVE if track(hwnd).dragging => move_to(hwnd, value_at(hwnd, lparam)),
        WM_LBUTTONUP => release(hwnd),
        WM_KEYDOWN => press_key(hwnd, wparam.0 as u16),
        WM_SETFOCUS => {
            redraw(hwnd);
            notify(hwnd, BN_SETFOCUS);
        }
        WM_KILLFOCUS => redraw(hwnd),
        TBM_SETRANGEMIN => store(hwnd, |track| track.low = lparam.0 as i32),
        TBM_SETRANGEMAX => store(hwnd, |track| track.high = lparam.0 as i32),
        TBM_SETPOS => store(hwnd, |track| track.value = lparam.0 as i32),
        TBM_GETPOS => return LRESULT(track(hwnd).value as isize),
        TBM_GETRANGEMIN => return LRESULT(track(hwnd).low as isize),
        TBM_GETRANGEMAX => return LRESULT(track(hwnd).high as isize),
        WM_NCDESTROY => {
            TRACKS.with(|tracks| tracks.borrow_mut().remove(&(hwnd.0 as isize)));
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        _ => return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
    LRESULT(0)
}

pub fn register(instance: HINSTANCE) {
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: CLASS,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&class);
    }
}

pub fn create(
    parent: HWND,
    instance: HINSTANCE,
    (id, name): (i32, &str),
    bounds: RECT,
) -> windows::core::Result<HWND> {
    let name = wide(name);
    let slider = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            PCWSTR(name.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            bounds.left,
            bounds.top,
            width(bounds),
            height(bounds),
            Some(parent),
            Some(HMENU(id as isize as _)),
            Some(instance),
            None,
        )?
    };
    track_hover(slider);
    Ok(slider)
}
