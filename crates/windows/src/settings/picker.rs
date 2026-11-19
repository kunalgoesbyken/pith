use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow,
    GetWindowLongPtrW, GetWindowTextLengthW, IsWindowVisible, MF_DISABLED, MF_STRING,
    TPM_LEFTALIGN, TPM_RETURNCMD, TPM_TOPALIGN, TrackPopupMenu, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PCWSTR};

use crate::util::{process_name, wide, window_pid};

fn is_cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    let read = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        )
    };
    read.is_ok() && cloaked != 0
}

fn is_app_window(window: HWND) -> bool {
    let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } as u32;
    unsafe { IsWindowVisible(window) }.as_bool()
        && unsafe { GetWindow(window, GW_OWNER) }.is_err()
        && unsafe { GetWindowTextLengthW(window) } > 0
        && style & WS_EX_TOOLWINDOW.0 == 0
        && !is_cloaked(window)
}

unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> BOOL {
    let apps = unsafe { &mut *(data.0 as *mut Vec<String>) };
    let own = std::process::id();
    let name = is_app_window(window)
        .then(|| window_pid(window))
        .flatten()
        .filter(|pid| *pid != own)
        .and_then(process_name);
    if let Some(name) = name
        && !apps.iter().any(|known| known.eq_ignore_ascii_case(&name))
    {
        apps.push(name);
    }
    BOOL(1)
}

pub fn running_apps() -> Vec<String> {
    let mut apps: Vec<String> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut apps as *mut Vec<String> as isize),
        );
    }
    apps.sort_by_key(|name| name.to_lowercase());
    apps
}

pub fn choose(window: HWND, at: (i32, i32), labels: &[String], empty: &str) -> Option<usize> {
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    if labels.is_empty() {
        let text = wide(empty);
        unsafe {
            let _ = AppendMenuW(menu, MF_STRING | MF_DISABLED, 0, PCWSTR(text.as_ptr()));
        }
    }
    for (index, label) in labels.iter().enumerate() {
        let text = wide(label);
        unsafe {
            let _ = AppendMenuW(menu, MF_STRING, index + 1, PCWSTR(text.as_ptr()));
        }
    }
    let picked = unsafe {
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD,
            at.0,
            at.1,
            None,
            window,
            None,
        )
    };
    unsafe {
        let _ = DestroyMenu(menu);
    }
    usize::try_from(picked.0)
        .ok()
        .and_then(|id| id.checked_sub(1))
}
