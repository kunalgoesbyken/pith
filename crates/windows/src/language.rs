use windows::Win32::Foundation::HWND;
use windows::Win32::Globalization::{LCIDToLocaleName, LOCALE_ALLOW_NEUTRAL_NAMES};
use windows::Win32::System::SystemServices::SORT_DEFAULT;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, HKL};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

const MAX_LOCALE_NAME: usize = 85;
const LANGUAGE_ID_MASK: usize = 0xFFFF;
const SORT_ID_SHIFT: u32 = 16;

fn locale_id(layout: HKL) -> u32 {
    let language_id = (layout.0 as usize & LANGUAGE_ID_MASK) as u32;
    language_id | (SORT_DEFAULT << SORT_ID_SHIFT)
}

pub fn input_language(window: HWND) -> Option<String> {
    let layout = unsafe { GetKeyboardLayout(GetWindowThreadProcessId(window, None)) };
    let mut name = [0u16; MAX_LOCALE_NAME];
    let length = unsafe {
        LCIDToLocaleName(
            locale_id(layout),
            Some(&mut name),
            LOCALE_ALLOW_NEUTRAL_NAMES,
        )
    };
    let length = usize::try_from(length).ok().filter(|length| *length > 1)?;
    Some(String::from_utf16_lossy(&name[..length - 1]))
}
