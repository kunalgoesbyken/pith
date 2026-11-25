use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_CAPTION_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_TEXT_COLOR,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmSetWindowAttribute,
};
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::WindowsAndMessaging::{WINDOW_EX_STYLE, WINDOW_STYLE};
use windows::core::Result;

use super::metrics::{height, width};
use super::theme::{Theme, colorref};

fn set_attribute<T>(
    hwnd: HWND,
    attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
    value: &T,
) -> Result<()> {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (value as *const T).cast(),
            size_of::<T>() as u32,
        )
    }
}

pub fn apply(hwnd: HWND, theme: &Theme) {
    let dark = i32::from(theme.is_dark());
    let _ = set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark);
    let _ = set_attribute::<DWM_WINDOW_CORNER_PREFERENCE>(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &DWMWCP_ROUND,
    );
    let (caption, text) = if theme.is_contrast() {
        (DWMWA_COLOR_DEFAULT, DWMWA_COLOR_DEFAULT)
    } else {
        (
            colorref(theme.palette.page).0,
            colorref(theme.palette.text).0,
        )
    };
    let _ = set_attribute(hwnd, DWMWA_CAPTION_COLOR, &caption);
    let _ = set_attribute(hwnd, DWMWA_TEXT_COLOR, &text);
}

pub fn frame_size(client: RECT, style: WINDOW_STYLE, dpi: u32) -> (i32, i32) {
    let mut frame = client;
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut frame, style, false, WINDOW_EX_STYLE(0), dpi);
    }
    (width(frame), height(frame))
}
