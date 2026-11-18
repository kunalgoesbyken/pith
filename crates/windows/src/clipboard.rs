use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Controls::EM_REPLACESEL;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_V};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
use windows::core::PCWSTR;

use crate::keys::{send_chord, wait_for_modifiers_released};
use crate::util::wide;

const GDI_FORMATS: [u32; 7] = [2, 3, 9, 14, 0x80, 0x82, 0x8E];
const PRIVACY_FORMATS: [&str; 3] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard",
];

fn open() -> bool {
    for _ in 0..50 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn save() -> Vec<(u32, Vec<u8>)> {
    let mut saved = Vec::new();
    if !open() {
        return saved;
    }
    unsafe {
        let mut format = 0u32;
        loop {
            format = EnumClipboardFormats(format);
            if format == 0 {
                break;
            }
            if GDI_FORMATS.contains(&format) {
                continue;
            }
            if let Ok(handle) = GetClipboardData(format) {
                let global = HGLOBAL(handle.0);
                let pointer = GlobalLock(global);
                if !pointer.is_null() {
                    saved.push((
                        format,
                        std::slice::from_raw_parts(pointer as *const u8, GlobalSize(global))
                            .to_vec(),
                    ));
                    let _ = GlobalUnlock(global);
                }
            }
        }
        let _ = CloseClipboard();
    }
    saved
}

fn put(format: u32, data: &[u8]) {
    unsafe {
        let Ok(global) = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) else {
            return;
        };
        let pointer = GlobalLock(global);
        if pointer.is_null() {
            return;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), pointer as *mut u8, data.len());
        let _ = GlobalUnlock(global);
        let _ = SetClipboardData(format, Some(HANDLE(global.0)));
    }
}

fn register(name: &str) -> u32 {
    let name = wide(name);
    unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) }
}

fn set_text(text: &str) -> bool {
    if !open() {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        let bytes: Vec<u8> = text
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        put(CF_UNICODETEXT.0 as u32, &bytes);
        for name in PRIVACY_FORMATS {
            put(register(name), &[0, 0, 0, 0]);
        }
        let _ = CloseClipboard();
    }
    true
}

fn restore(saved: &[(u32, Vec<u8>)]) {
    if !open() {
        return;
    }
    unsafe {
        let _ = EmptyClipboard();
        for (format, data) in saved {
            put(*format, data);
        }
        let _ = CloseClipboard();
    }
}

pub fn paste(text: String) {
    std::thread::spawn(move || {
        let saved = save();
        if !set_text(&text) {
            return;
        }
        wait_for_modifiers_released(Duration::from_millis(400));
        send_chord(&[VK_CONTROL], VK_V);
        std::thread::sleep(Duration::from_millis(300));
        restore(&saved);
    });
}

pub fn replace_selection(hwnd: isize, text: &str) {
    let text = wide(text);
    unsafe {
        SendMessageW(
            HWND(hwnd as _),
            EM_REPLACESEL,
            Some(WPARAM(1)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
}
