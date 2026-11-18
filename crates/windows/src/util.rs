use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, SW_SHOWNORMAL};
use windows::core::{PCWSTR, PWSTR, w};

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

static NAMES: Mutex<Option<HashMap<u32, String>>> = Mutex::new(None);

pub fn process_name(pid: u32) -> String {
    if let Ok(mut guard) = NAMES.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(name) = cache.get(&pid) {
            return name.clone();
        }
        let name = query_process_name(pid);
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(pid, name.clone());
        return name;
    }
    query_process_name(pid)
}

fn query_process_name(pid: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(handle);
        if result.is_err() {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        path.rsplit('\\')
            .next()
            .unwrap_or_default()
            .trim_end_matches(".exe")
            .trim_end_matches(".EXE")
            .to_string()
    }
}

pub fn single_instance(name: &str) -> bool {
    let name = wide(name);
    unsafe {
        let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr()));
        handle.is_ok() && GetLastError() != ERROR_ALREADY_EXISTS
    }
}

pub fn open_path(path: &Path) {
    let target = wide(&path.to_string_lossy());
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}
