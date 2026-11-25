use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

use crate::system::util::{process_name, window_pid};

const SHELL_CLASSES: [&str; 5] = [
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "XamlExplorerHostIslandWindow",
];

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

#[derive(Default)]
pub struct Foreground {
    window: isize,
    app: Option<String>,
}

impl Foreground {
    pub fn app(&self) -> Option<&str> {
        self.app.as_deref()
    }

    pub fn poll(&mut self) {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() || hwnd.0 as isize == self.window {
            return;
        }
        self.window = hwnd.0 as isize;
        let Some(pid) = window_pid(hwnd) else { return };
        if pid == std::process::id() || SHELL_CLASSES.contains(&class_name(hwnd).as_str()) {
            return;
        }
        if let Some(name) = process_name(pid) {
            self.app = Some(name);
        }
    }
}
