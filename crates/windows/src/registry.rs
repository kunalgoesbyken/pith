use std::path::Path;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_DWORD,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use windows::core::{PCWSTR, Result};

use crate::util::wide;

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "Pith";
pub const THEME_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

pub fn run_command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(subkey: &str, write: bool) -> Result<Key> {
    let subkey = wide(subkey);
    let mut key = HKEY::default();
    let status = unsafe {
        if write {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
        } else {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                KEY_READ,
                &mut key,
            )
        }
    };
    status.ok()?;
    Ok(Key(key))
}

pub fn read_string(subkey: &str, name: &str) -> Option<String> {
    let key = open(subkey, false).ok()?;
    let name = wide(name);
    let mut size = 0u32;
    unsafe {
        let probe = RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            None,
            None,
            Some(&mut size),
        );
        if probe != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let status = RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        );
        if status != ERROR_SUCCESS {
            return None;
        }
        let length = (size as usize / 2).min(buffer.len());
        let text = &buffer[..length];
        let end = text.iter().position(|unit| *unit == 0).unwrap_or(text.len());
        Some(String::from_utf16_lossy(&text[..end]))
    }
}

pub fn read_dword(subkey: &str, name: &str) -> Option<u32> {
    let name = wide(name);
    let subkey = wide(subkey);
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(value)
}

pub fn write_string(subkey: &str, name: &str, value: Option<&str>) -> Result<()> {
    let key = open(subkey, true)?;
    let name = wide(name);
    unsafe {
        match value {
            Some(value) => {
                let data = wide(value);
                let bytes = std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
                RegSetValueExW(key.0, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)).ok()
            }
            None => {
                let status = RegDeleteValueW(key.0, PCWSTR(name.as_ptr()));
                if status == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                    Ok(())
                } else {
                    status.ok()
                }
            }
        }
    }
}

pub fn start_with_windows() -> bool {
    read_string(RUN_KEY, RUN_VALUE).is_some()
}

pub fn set_start_with_windows(enabled: bool) -> Result<()> {
    let value = if enabled {
        let exe = std::env::current_exe().map_err(|error| {
            windows::core::Error::new(windows::Win32::Foundation::E_FAIL, error.to_string())
        })?;
        Some(run_command(&exe))
    } else {
        None
    };
    write_string(RUN_KEY, RUN_VALUE, value.as_deref())
}

pub fn apps_use_dark_theme() -> bool {
    read_dword(THEME_KEY, "AppsUseLightTheme") == Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_key() -> String {
        format!(r"Software\PithTest\{}", std::process::id())
    }

    fn remove(subkey: &str) {
        use windows::Win32::System::Registry::RegDeleteTreeW;
        let subkey = wide(subkey);
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()));
        }
    }

    #[test]
    fn run_command_quotes_paths_with_spaces_and_unicode() {
        assert_eq!(
            run_command(Path::new(r"C:\Program Files\Pith\pith.exe")),
            r#""C:\Program Files\Pith\pith.exe""#
        );
        assert_eq!(
            run_command(Path::new(r"C:\Users\名前\pith.exe")),
            "\"C:\\Users\\名前\\pith.exe\""
        );
    }

    #[test]
    fn strings_round_trip_overwrite_and_delete_in_a_scratch_key() {
        let key = scratch_key();
        assert_eq!(read_string(&key, "Value"), None);
        for value in ["\"C:\\a b\\pith.exe\"", "名前 \u{1F600}", "x"] {
            write_string(&key, "Value", Some(value)).unwrap();
            assert_eq!(read_string(&key, "Value").as_deref(), Some(value));
        }
        write_string(&key, "Value", None).unwrap();
        assert_eq!(read_string(&key, "Value"), None);
        write_string(&key, "Value", None).unwrap();
        remove(&key);
    }

    #[test]
    fn missing_dword_reads_as_none() {
        assert_eq!(read_dword(&scratch_key(), "AppsUseLightTheme"), None);
    }
}
