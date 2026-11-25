use windows::Win32::Foundation::HWND;
use windows::Win32::Globalization::{LCIDToLocaleName, LOCALE_ALLOW_NEUTRAL_NAMES};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

const MAX_LOCALE_NAME: usize = 85;
const LANGUAGE_ID_MASK: usize = 0xFFFF;

pub fn input_language(window: HWND) -> Option<String> {
    let layout = unsafe { GetKeyboardLayout(GetWindowThreadProcessId(window, None)) };
    let language_id = (layout.0 as usize & LANGUAGE_ID_MASK) as u32;
    let mut name = [0u16; MAX_LOCALE_NAME];
    let length =
        unsafe { LCIDToLocaleName(language_id, Some(&mut name), LOCALE_ALLOW_NEUTRAL_NAMES) };
    let length = usize::try_from(length).ok().filter(|length| *length > 1)?;
    Some(String::from_utf16_lossy(&name[..length - 1]))
}
