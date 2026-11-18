use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowTextW, GetWindowThreadProcessId, SW_SHOWNORMAL,
};
use windows::core::{PCWSTR, PWSTR, w};

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

pub fn process_name(pid: u32) -> String {
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
        pith_core::app_name(&path)
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

pub fn remove_contents(dir: &Path, attempts: u32, delay: Duration) -> Vec<PathBuf> {
    let mut remaining = leftovers(dir);
    for _ in 1..attempts {
        if remaining.is_empty() {
            break;
        }
        std::thread::sleep(delay);
        remaining = leftovers(dir);
    }
    remaining
}

fn leftovers(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut failed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let removed = match entry.file_type() {
            Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(&path),
            _ => std::fs::remove_file(&path),
        };
        if removed.is_err() {
            failed.push(path);
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pith-win-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn removes_nested_contents_and_keeps_the_folder_itself() {
        let dir = scratch("nested");
        std::fs::create_dir_all(dir.join("models").join("deep")).unwrap();
        std::fs::write(dir.join("models").join("deep").join("a.gguf"), "x").unwrap();
        std::fs::write(dir.join("pith.log"), "x").unwrap();
        assert!(remove_contents(&dir, 1, Duration::ZERO).is_empty());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_is_nothing_to_remove() {
        let dir = scratch("missing").join("absent");
        assert!(remove_contents(&dir, 3, Duration::ZERO).is_empty());
    }

    #[test]
    fn a_locked_file_is_reported_while_the_rest_goes() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("locked");
        std::fs::write(dir.join("other.txt"), "x").unwrap();
        let locked = dir.join("locked.log");
        let _held = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(0)
            .open(&locked)
            .unwrap();
        assert_eq!(remove_contents(&dir, 2, Duration::from_millis(1)), [locked]);
        assert!(!dir.join("other.txt").exists());
        drop(_held);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
