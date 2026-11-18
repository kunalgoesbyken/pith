mod buffer;
mod canvas;
mod chrome;
mod control;
mod font;
mod hover;
mod metrics;
mod opaque;
mod slot;
mod theme;

pub use canvas::{Align, Canvas, Text, measure, paint};
pub use chrome::{apply as apply_chrome, frame_size};
pub use control::{
    BUTTON_HEIGHT, FOCUS_MARGIN, Look, Parent, Surface, Tone, draw, expand, paint_backdrop,
    toggle_width,
};
pub use font::gdi_font;
pub use metrics::{
    BASE_DPI, CARD_RADIUS, CONTROL_RADIUS, Role, STROKE, Scale, height, inset, rect, width,
};
pub use opaque::make_opaque;
pub use slot::Slot;
pub use theme::{Rgb, Theme, force_dark_theme};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{SPI_SETHIGHCONTRAST, WM_SETTINGCHANGE};
use windows::core::{PCWSTR, w};

pub fn is_theme_change(message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    if message != WM_SETTINGCHANGE {
        return false;
    }
    if lparam.0 == 0 {
        return wparam.0 == SPI_SETHIGHCONTRAST.0 as usize;
    }
    let area = PCWSTR(lparam.0 as *const u16);
    unsafe { area.as_wide() == w!("ImmersiveColorSet").as_wide() }
}
