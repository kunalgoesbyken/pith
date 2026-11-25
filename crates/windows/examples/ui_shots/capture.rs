use std::path::PathBuf;

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::common::{bgra_to_rgba, bitmap_info, png, window_rect};

pub(crate) const OUT_DIR: &str = "target/ui-shots";

const PARKED_X: i32 = -12000;
const PARKED_Y: i32 = 100;
const PRINT_RENDER_FULL_CONTENT: u32 = 2;

pub(crate) fn pump() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn composite(bgra: &mut [u8], fallback: [u8; 3]) {
    for pixel in bgra.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for (channel, base) in pixel.iter_mut().take(3).zip(fallback.iter().rev()) {
            let covered = u32::from(*channel) * alpha + u32::from(*base) * (255 - alpha);
            *channel = (covered / 255) as u8;
        }
        pixel[3] = 255;
    }
}

pub(crate) fn save(name: &str, width: i32, height: i32, bgra: &[u8]) {
    let path = PathBuf::from(OUT_DIR).join(format!("{name}.png"));
    let mut rgba = bgra.to_vec();
    bgra_to_rgba(&mut rgba);
    std::fs::write(&path, png(width as u32, height as u32, &rgba)).unwrap();
    println!("{}", path.display());
}

pub(crate) fn capture(hwnd: HWND, name: &str, fallback: [u8; 3]) {
    pump();
    let rect = window_rect(hwnd);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let info = bitmap_info(width, height);
    let mut bits = std::ptr::null_mut();
    unsafe {
        let dc = CreateCompatibleDC(None);
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let previous = SelectObject(dc, bitmap.into());
        let _ = PrintWindow(hwnd, dc, PRINT_WINDOW_FLAGS(PRINT_RENDER_FULL_CONTENT));
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
        composite(pixels, fallback);
        save(name, width, height, pixels);
        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
    }
}

pub(crate) fn park(hwnd: HWND, dpi: u32) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            PARKED_X,
            PARKED_Y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if dpi != 96 {
            let at = window_rect(hwnd);
            SendMessageW(
                hwnd,
                WM_DPICHANGED,
                Some(WPARAM((dpi | (dpi << 16)) as usize)),
                Some(LPARAM(&at as *const RECT as isize)),
            );
        }
    }
    pump();
}

pub(crate) fn resize(hwnd: HWND, dpi: u32, logical: (i32, i32)) {
    let scale = |value: i32| value * dpi as i32 / 96;
    let (frame, client) = (window_rect(hwnd), client_rect(hwnd));
    let extra = (
        frame.right - frame.left - client.right,
        frame.bottom - frame.top - client.bottom,
    );
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            PARKED_X,
            PARKED_Y,
            scale(logical.0) + extra.0,
            scale(logical.1) + extra.1,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    pump();
}

fn client_rect(hwnd: HWND) -> RECT {
    let mut area = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut area);
    }
    area
}
