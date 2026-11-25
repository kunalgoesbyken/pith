use std::path::Path;

use pith_core::{Hardware, Rect};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, RECT, SYSTEMTIME,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::SystemInformation::{
    GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowTextW, GetWindowThreadProcessId, SW_SHOWNORMAL,
};
use windows::core::{BOOL, PCWSTR, PWSTR, w};

pub fn rect_of(raw: RECT) -> Rect {
    Rect {
        x: raw.left,
        y: raw.top,
        width: raw.right - raw.left,
        height: raw.bottom - raw.top,
    }
}

unsafe extern "system" fn collect_work_area(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        unsafe { &mut *(data.0 as *mut Vec<Rect>) }.push(rect_of(info.rcWork));
    }
    BOOL(1)
}

pub fn work_areas() -> Vec<Rect> {
    let mut areas: Vec<Rect> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_work_area),
            LPARAM(&mut areas as *mut Vec<Rect> as isize),
        );
    }
    areas
}

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

pub fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        Some(pith_core::app_name(&path).to_string())
    }
}

pub fn local_time() -> SYSTEMTIME {
    unsafe { GetLocalTime() }
}

pub fn single_instance(name: &str) -> bool {
    let name = wide(name);
    unsafe {
        let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr()));
        handle.is_ok() && GetLastError() != ERROR_ALREADY_EXISTS
    }
}

pub fn hardware() -> Hardware {
    let mut memory = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let ram_mib =
        unsafe { GlobalMemoryStatusEx(&mut memory) }.map_or(0, |()| memory.ullTotalPhys >> 20);
    Hardware {
        ram_mib,
        cores: std::thread::available_parallelism().map_or(1, |cores| cores.get()),
    }
}

pub fn open_path(path: &Path) {
    open_target(&path.to_string_lossy());
}

pub fn open_target(target: &str) {
    let target = wide(target);
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

pub fn window_pid(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

pub fn is_elevated(pid: u32) -> bool {
    if pid == std::process::id() {
        return false;
    }
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return true;
        };
        let mut token = HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token);
        let _ = CloseHandle(process);
        if opened.is_err() {
            return true;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let queried = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        queried.is_err() || elevation.TokenIsElevated != 0
    }
}
