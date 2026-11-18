use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Controls::EM_REPLACESEL;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_V};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_PASTE};
use windows::core::PCWSTR;

use crate::keys::{send_chord, wait_for_modifiers_released};
use crate::util::wide;

const GDI_FORMATS: [u32; 7] = [2, 3, 9, 14, 0x80, 0x82, 0x8E];
const HANDLE_FORMATS: std::ops::RangeInclusive<u32> = 0x200..=0x3FF;
const MODIFIER_WAIT: Duration = Duration::from_secs(2);
const RESTORE_AFTER: Duration = Duration::from_millis(600);
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

fn save() -> Option<Vec<(u32, Vec<u8>)>> {
    let mut saved = Vec::new();
    if !open() {
        return None;
    }
    unsafe {
        let mut format = 0u32;
        loop {
            format = EnumClipboardFormats(format);
            if format == 0 {
                break;
            }
            if GDI_FORMATS.contains(&format) || HANDLE_FORMATS.contains(&format) {
                continue;
            }
            if let Ok(handle) = GetClipboardData(format) {
                let global = HGLOBAL(handle.0);
                let pointer = GlobalLock(global);
                if !pointer.is_null() && GlobalSize(global) > 0 {
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
    Some(saved)
}

fn put(format: u32, data: &[u8]) {
    unsafe {
        let Ok(global) = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) else {
            return;
        };
        let pointer = GlobalLock(global);
        if pointer.is_null() {
            let _ = GlobalFree(Some(global));
            return;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), pointer as *mut u8, data.len());
        let _ = GlobalUnlock(global);
        if SetClipboardData(format, Some(HANDLE(global.0))).is_err() {
            let _ = GlobalFree(Some(global));
        }
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

fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

#[derive(Clone, Copy)]
pub enum Trigger {
    Chord,
    Message(isize),
}

impl Trigger {
    fn ready(self) -> bool {
        match self {
            Trigger::Chord => wait_for_modifiers_released(MODIFIER_WAIT),
            Trigger::Message(_) => true,
        }
    }

    fn fire(self) {
        match self {
            Trigger::Chord => send_chord(&[VK_CONTROL], VK_V),
            Trigger::Message(hwnd) => unsafe {
                SendMessageW(HWND(hwnd as _), WM_PASTE, None, None);
            },
        }
    }
}

fn paste_burst(first: (Trigger, String), queue: &Receiver<(Trigger, String)>) {
    let Some(saved) = save() else { return };
    let mut ours = None;
    let mut next = Some(first);
    while let Some((trigger, text)) = next {
        let untouched = ours.is_none_or(|seq| seq == sequence());
        if !untouched || !trigger.ready() || !set_text(&text) {
            break;
        }
        ours = Some(sequence());
        trigger.fire();
        next = queue.recv_timeout(RESTORE_AFTER).ok();
    }
    if ours.is_some_and(|seq| seq == sequence()) {
        restore(&saved);
    }
}

pub fn paste(text: String, trigger: Trigger) {
    static QUEUE: OnceLock<Mutex<Sender<(Trigger, String)>>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = channel::<(Trigger, String)>();
        std::thread::spawn(move || {
            while let Ok(text) = receiver.recv() {
                paste_burst(text, &receiver);
            }
        });
        Mutex::new(sender)
    });
    if let Ok(sender) = queue.lock() {
        let _ = sender.send((trigger, text));
    }
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

pub fn replace_selection_after(hwnd: isize, text: String, lag: Duration) {
    static QUEUE: OnceLock<Mutex<Sender<(isize, String, Instant)>>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = channel::<(isize, String, Instant)>();
        std::thread::spawn(move || {
            while let Ok((hwnd, text, due)) = receiver.recv() {
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                replace_selection(hwnd, &text);
            }
        });
        Mutex::new(sender)
    });
    if let Ok(sender) = queue.lock() {
        let _ = sender.send((hwnd, text, Instant::now() + lag));
    }
}
