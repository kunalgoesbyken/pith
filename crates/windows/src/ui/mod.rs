mod buffer;
mod canvas;
mod chrome;
mod control;
mod font;
mod hover;
mod keycap;
mod mark;
mod metrics;
mod opaque;
mod skin;
mod slot;
mod theme;
mod widgets;

pub use canvas::{Align, Canvas, Text, measure, paint};
pub use chrome::{apply as apply_chrome, frame_size};
pub use control::{
    BUTTON_HEIGHT, ChipState, FOCUS_MARGIN, Look, Parent, SLIDER_WIDTH, Seam, Surface,
    TOGGLE_HEIGHT, Tone, button_width, draw, expand, paint_backdrop, segment_width, toggle_width,
};
pub use font::gdi_font;
pub use hover::attach as track_hover;
pub use keycap::{KEYCAP_HEIGHT, key_labels, keycaps, keycaps_width};
pub use mark::{app_icon, mark_width, paint_mark};
pub use metrics::{BASE_DPI, Role, STROKE, Scale, height, inset, rect, width};
pub use opaque::make_opaque;
pub use skin::request_draw;
pub use slot::Slot;
pub use theme::{INK, Mode, Rgb, Taskbar, Theme, colorref, force_dark_theme};
pub use widgets::{chip_width, slider_fraction_at};

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
