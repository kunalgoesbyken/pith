use pith_core::{PILL_ELEVATION, Scale};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::EM_POSFROMCHAR;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::BOOL;

use crate::common::{overlay_visible, overlay_window, window_rect};
use crate::harness::edit::send;
use crate::harness::window::Field;

pub(crate) fn caret_screen(edit: isize, index: usize) -> (i32, i32) {
    let mut point = POINT::default();
    send(
        edit,
        EM_POSFROMCHAR,
        &mut point as *mut POINT as usize,
        index as isize,
    );
    unsafe {
        MapWindowPoints(
            Some(HWND(edit as _)),
            None,
            std::slice::from_mut(&mut point),
        );
    }
    (point.x, point.y)
}

pub(crate) fn monitor_of(hwnd: HWND) -> HMONITOR {
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) }
}

pub(crate) fn monitor_rect(monitor: HMONITOR) -> RECT {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetMonitorInfoW(monitor, &mut info);
    }
    info.rcWork
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    found: LPARAM,
) -> BOOL {
    unsafe { &mut *(found.0 as *mut Vec<RECT>) }.push(monitor_rect(monitor));
    BOOL(1)
}

fn monitors() -> Vec<RECT> {
    let mut found: Vec<RECT> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut found as *mut Vec<RECT> as isize),
        );
    }
    found
}

const OFF_SCREEN_PX: i32 = 300;

pub(crate) fn move_caret_off_screen(field: Field, rect: RECT, caret_x: i32) {
    let rightmost = monitors().iter().map(|m| m.right).max().unwrap_or_default();
    move_host(
        field,
        rect.left + rightmost + OFF_SCREEN_PX - caret_x,
        rect.top,
    );
}

pub(crate) fn move_host(field: Field, left: i32, top: i32) {
    unsafe {
        let _ = SetWindowPos(
            HWND(field.host as _),
            None,
            left,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

pub(crate) fn pill_body() -> Option<RECT> {
    let hwnd = overlay_window().filter(|_| overlay_visible())?;
    let margin = Scale(unsafe { GetDpiForWindow(hwnd) }).px(PILL_ELEVATION);
    let outer = window_rect(hwnd);
    Some(RECT {
        left: outer.left + margin,
        top: outer.top + margin,
        right: outer.right - margin,
        bottom: outer.bottom - margin,
    })
}
