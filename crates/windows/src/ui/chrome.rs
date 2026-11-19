use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_MAINWINDOW, DWMSBT_NONE,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::WindowsAndMessaging::{WINDOW_EX_STYLE, WINDOW_STYLE};
use windows::core::Result;

use super::metrics::{height, width};
use super::theme::Theme;

const MICA_BUILD: u32 = 22621;
const WHOLE_WINDOW: i32 = -1;

fn build_number() -> u32 {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    let _ = unsafe { RtlGetVersion(&mut info) };
    info.dwBuildNumber
}

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

fn extend_frame(hwnd: HWND, by: i32) {
    let margins = MARGINS {
        cxLeftWidth: by,
        cxRightWidth: by,
        cyTopHeight: by,
        cyBottomHeight: by,
    };
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

pub fn apply(hwnd: HWND, theme: &Theme) -> bool {
    let dark = i32::from(theme.is_dark());
    let _ = set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark);
    let _ = set_attribute::<DWM_WINDOW_CORNER_PREFERENCE>(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &DWMWCP_ROUND,
    );
    let mica = !theme.is_contrast()
        && build_number() >= MICA_BUILD
        && set_attribute::<DWM_SYSTEMBACKDROP_TYPE>(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &DWMSBT_MAINWINDOW,
        )
        .is_ok();
    if mica {
        extend_frame(hwnd, WHOLE_WINDOW);
        return true;
    }
    let _ = set_attribute::<DWM_SYSTEMBACKDROP_TYPE>(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_NONE);
    extend_frame(hwnd, 0);
    false
}

pub fn frame_size(client: RECT, style: WINDOW_STYLE, dpi: u32) -> (i32, i32) {
    let mut frame = client;
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut frame, style, false, WINDOW_EX_STYLE(0), dpi);
    }
    (width(frame), height(frame))
}
