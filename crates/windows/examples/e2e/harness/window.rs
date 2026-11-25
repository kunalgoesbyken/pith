use std::sync::mpsc::channel;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::common::wide;

pub(crate) const ES_MULTILINE: u32 = 0x0004;
pub(crate) const ES_AUTOVSCROLL: u32 = 0x0040;
pub(crate) const ES_PASSWORD: u32 = 0x0020;
pub(crate) const ES_AUTOHSCROLL: u32 = 0x0080;

#[derive(Clone, Copy)]
pub(crate) struct Field {
    pub(crate) host: isize,
    pub(crate) edit: isize,
}

unsafe extern "system" fn host_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub(crate) fn spawn_test_window(title: &str, left: i32, top: i32, edit_style: u32) -> Field {
    spawn_window(title, left, top, move |host, instance| unsafe {
        rich_edit(host, instance, edit_style, (10, 10, 860, 130))
    })
}

pub(crate) fn spawn_form_window(title: &str, left: i32, top: i32) -> Field {
    spawn_window(title, left, top, |host, instance| unsafe {
        let child = |class: PCWSTR, text: PCWSTR, style: u32, (x, y, width): (i32, i32, i32)| {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                text,
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                x,
                y,
                width,
                22,
                Some(host),
                None,
                Some(instance),
                None,
            )
            .unwrap();
        };
        child(w!("STATIC"), w!("Subject"), 0, (10, 8, 80));
        child(
            w!("EDIT"),
            w!("Quarterly numbers"),
            ES_AUTOHSCROLL,
            (95, 8, 300),
        );
        child(w!("STATIC"), w!("Password"), 0, (410, 8, 80));
        child(
            w!("EDIT"),
            w!("hunter2"),
            ES_PASSWORD | ES_AUTOHSCROLL,
            (495, 8, 200),
        );
        child(w!("STATIC"), w!("Message"), 0, (10, 40, 80));
        rich_edit(
            host,
            instance,
            ES_MULTILINE | ES_AUTOVSCROLL,
            (95, 40, 775, 110),
        )
    })
}

unsafe fn rich_edit(
    host: HWND,
    instance: HINSTANCE,
    style: u32,
    (x, y, width, height): (i32, i32, i32, i32),
) -> HWND {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RICHEDIT50W"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
            x,
            y,
            width,
            height,
            Some(host),
            None,
            Some(instance),
            None,
        )
        .unwrap()
    }
}

fn spawn_window(
    title: &str,
    left: i32,
    top: i32,
    build: impl FnOnce(HWND, HINSTANCE) -> HWND + Send + 'static,
) -> Field {
    let (tx, rx) = channel();
    let title = wide(title);
    std::thread::spawn(move || unsafe {
        let _ = LoadLibraryW(w!("Msftedit.dll"));
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("PithE2EHost");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(host_procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let host = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            left,
            top,
            900,
            200,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let edit = build(host, instance.into());
        let _ = ShowWindow(host, SW_SHOWNOACTIVATE);
        tx.send(Field {
            host: host.0 as isize,
            edit: edit.0 as isize,
        })
        .unwrap();
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    rx.recv().unwrap()
}
