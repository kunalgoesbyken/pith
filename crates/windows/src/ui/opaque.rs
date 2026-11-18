use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, PRF_CLIENT, PRF_ERASEBKGND, SendMessageW, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT,
    WM_PRINTCLIENT,
};

use super::buffer::Buffer;
use super::metrics::{height, width};

const SUBCLASS_ID: usize = 0x4F50_4151;

fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
    let mut area = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut area);
    }
    if let Some(buffer) = Buffer::new((width(area), height(area))) {
        unsafe {
            SendMessageW(
                hwnd,
                WM_PRINTCLIENT,
                Some(WPARAM(buffer.memory.0 as usize)),
                Some(LPARAM((PRF_CLIENT | PRF_ERASEBKGND) as isize)),
            );
        }
        buffer.make_opaque();
        buffer.copy_to(hdc, (0, 0));
    }
    unsafe {
        let _ = EndPaint(hwnd, &paint);
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
        WM_PAINT => {
            paint(hwnd);
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID);
        },
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn make_opaque(control: HWND) {
    unsafe {
        let _ = SetWindowSubclass(control, Some(procedure), SUBCLASS_ID, 0);
    }
}
