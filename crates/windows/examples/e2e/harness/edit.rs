use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Controls::{
    EM_GETFIRSTVISIBLELINE, EM_LINEFROMCHAR, EM_LINESCROLL, EM_REPLACESEL, EM_SETSEL,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::common::wide;

pub(crate) fn send(edit: isize, message: u32, wparam: usize, lparam: isize) -> isize {
    unsafe {
        SendMessageW(
            HWND(edit as _),
            message,
            Some(WPARAM(wparam)),
            Some(LPARAM(lparam)),
        )
        .0
    }
}

fn scroll_to_caret(edit: isize) {
    let line = send(edit, EM_LINEFROMCHAR, usize::MAX, 0);
    let first = send(edit, EM_GETFIRSTVISIBLELINE, 0, 0);
    send(edit, EM_LINESCROLL, 0, line - first);
}

pub(crate) fn send_text(edit: isize, text: &str) {
    let text = wide(text);
    send(edit, EM_REPLACESEL, 1, text.as_ptr() as isize);
    scroll_to_caret(edit);
}

pub(crate) fn text_length(edit: isize) -> usize {
    send(edit, WM_GETTEXTLENGTH, 0, 0) as usize
}

pub(crate) fn select_range(edit: isize, start: usize, end: usize) {
    send(edit, EM_SETSEL, start, end as isize);
    scroll_to_caret(edit);
}

pub(crate) fn select(edit: isize, at: usize) {
    select_range(edit, at, at);
}

pub(crate) fn select_end(edit: isize) {
    select(edit, text_length(edit));
}

pub(crate) fn set_text(edit: isize, text: &str) {
    let text = wide(text);
    send(edit, WM_SETTEXT, 0, text.as_ptr() as isize);
    select_end(edit);
}

pub(crate) fn get_text(edit: isize) -> String {
    let mut buffer = vec![0u16; text_length(edit) + 1];
    let length = send(edit, WM_GETTEXT, buffer.len(), buffer.as_mut_ptr() as isize);
    String::from_utf16_lossy(&buffer[..length as usize]).replace('\r', "\n")
}
