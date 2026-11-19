use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
    SendInput, VIRTUAL_KEY, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, IsWindowVisible, SW_MAXIMIZE, SetCursorPos, SetForegroundWindow,
    ShowWindow,
};
use windows::core::BOOL;

use crate::common::window_title;

const KEY_GAP: Duration = Duration::from_millis(30);
const WINDOW_WAIT: Duration = Duration::from_secs(25);
const GONE_WAIT: Duration = Duration::from_secs(40);
const WINDOW_SETTLE: Duration = Duration::from_millis(2000);
const CLOSE_SETTLE: Duration = Duration::from_millis(1500);
pub const POLL: Duration = Duration::from_millis(250);

fn keyboard(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn inject(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn press(vk: VIRTUAL_KEY) {
    inject(&[
        keyboard(vk, 0, KEYBD_EVENT_FLAGS(0)),
        keyboard(vk, 0, KEYEVENTF_KEYUP),
    ]);
}

pub fn type_text(text: &str) {
    for unit in text.encode_utf16() {
        inject(&[
            keyboard(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE),
            keyboard(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
        ]);
        std::thread::sleep(KEY_GAP);
    }
}

fn click(x: i32, y: i32) {
    unsafe {
        let _ = SetCursorPos(x, y);
    }
    inject(&[mouse(MOUSEEVENTF_LEFTDOWN), mouse(MOUSEEVENTF_LEFTUP)]);
}

pub fn window_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    rect
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = unsafe { &mut *(lparam.0 as *mut Vec<(HWND, String)>) };
    if unsafe { IsWindowVisible(hwnd).as_bool() } {
        let title = window_title(hwnd);
        if !title.is_empty() {
            found.push((hwnd, title));
        }
    }
    BOOL(1)
}

fn visible_windows() -> Vec<(HWND, String)> {
    let mut found: Vec<(HWND, String)> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut found as *mut Vec<(HWND, String)> as isize),
        );
    }
    found
}

pub fn visible_titles() -> String {
    let titles: Vec<String> = visible_windows()
        .into_iter()
        .map(|(_, title)| title)
        .collect();
    format!("windows {titles:?}")
}

fn find_window(marker: &str) -> Option<HWND> {
    visible_windows()
        .into_iter()
        .find(|(_, title)| title.contains(marker))
        .map(|(hwnd, _)| hwnd)
}

pub fn wait_for_window(marker: &str) -> Option<HWND> {
    let deadline = Instant::now() + WINDOW_WAIT;
    while Instant::now() < deadline {
        if let Some(hwnd) = find_window(marker) {
            return Some(hwnd);
        }
        std::thread::sleep(POLL);
    }
    None
}

pub fn wait_until_gone(marker: &str) {
    let deadline = Instant::now() + GONE_WAIT;
    while find_window(marker).is_some() && Instant::now() < deadline {
        std::thread::sleep(POLL);
    }
    std::thread::sleep(CLOSE_SETTLE);
}

pub fn focus_field(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_MAXIMIZE);
    }
    press(VK_MENU);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    std::thread::sleep(WINDOW_SETTLE);
    let rect = window_rect(hwnd);
    click((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
}
