use std::time::Duration;

use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

fn read_clipboard_text() -> Option<String> {
    unsafe {
        IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).ok()?;
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
        let global = windows::Win32::Foundation::HGLOBAL(handle.0);
        let pointer = GlobalLock(global) as *const u16;
        if pointer.is_null() {
            return None;
        }
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
        let _ = GlobalUnlock(global);
        Some(text)
    }
}

pub(crate) fn clipboard_text() -> Option<String> {
    for _ in 0..50 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            let text = read_clipboard_text();
            let _ = unsafe { CloseClipboard() };
            return text;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}
