use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    SM_CXSMICON, SPI_GETCLIENTAREAANIMATION, SPI_SETCLIENTAREAANIMATION, SPI_SETICONMETRICS,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_SETTINGCHANGE, WM_SYSCOLORCHANGE, WM_THEMECHANGED,
};
use windows::core::BOOL;

use crate::ui::{Taskbar, Theme, is_theme_change};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub theme: Theme,
    pub taskbar: Taskbar,
    pub animated: bool,
    pub icon_px: u32,
}

impl Appearance {
    pub fn detect() -> Self {
        let theme = Theme::detect();
        Self {
            theme,
            taskbar: Taskbar::of(&theme),
            animated: animations_enabled(),
            icon_px: icon_px(),
        }
    }
}

fn animations_enabled() -> bool {
    let mut enabled = BOOL(1);
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    enabled.as_bool()
}

fn icon_px() -> u32 {
    unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) as u32 }
}

pub fn is_appearance_change(message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    match message {
        WM_SYSCOLORCHANGE | WM_THEMECHANGED | WM_DISPLAYCHANGE | WM_DPICHANGED => true,
        WM_SETTINGCHANGE => {
            is_theme_change(message, wparam, lparam)
                || (lparam.0 == 0
                    && [SPI_SETCLIENTAREAANIMATION, SPI_SETICONMETRICS]
                        .iter()
                        .any(|setting| setting.0 as usize == wparam.0))
        }
        _ => false,
    }
}
