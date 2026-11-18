use pith_core::{Rect, Rgb, median_by_luminance};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetDC, GetMonitorInfoW, GetPixel, HDC, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    ReleaseDC,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

use crate::caret::gui_thread_info;
use crate::ui::{BASE_DPI, Scale};
use crate::util::rect_of;

const CLR_INVALID: u32 = 0xFFFF_FFFF;
const SAMPLE_COLUMNS: i32 = 4;
const SAMPLE_ROWS: i32 = 3;
const SAMPLE_STEP: i32 = 8;
const UNKNOWN_WORK_REACH: i32 = 4096;

pub struct ScreenDc(pub HDC);

impl ScreenDc {
    pub fn get() -> Self {
        Self(unsafe { GetDC(None) })
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(None, self.0);
        }
    }
}

#[derive(Clone, Copy)]
pub struct Monitor {
    pub work: Rect,
    pub scale: Scale,
}

impl Monitor {
    pub fn nearest(target: Rect) -> Self {
        let centre = POINT {
            x: target.x + target.width / 2,
            y: target.y + target.height / 2,
        };
        let monitor = unsafe { MonitorFromPoint(centre, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let known = unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool();
        let work = if known {
            rect_of(info.rcWork)
        } else {
            Rect {
                x: target.x - UNKNOWN_WORK_REACH,
                y: target.y - UNKNOWN_WORK_REACH,
                width: 2 * UNKNOWN_WORK_REACH,
                height: 2 * UNKNOWN_WORK_REACH,
            }
        };
        let (mut dpi_x, mut dpi_y) = (BASE_DPI, BASE_DPI);
        let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
        Self {
            work,
            scale: Scale(dpi_x.max(BASE_DPI)),
        }
    }
}

pub fn focused_field() -> Option<Rect> {
    let info = gui_thread_info(None)?;
    let mut raw = RECT::default();
    unsafe { GetWindowRect(info.hwndFocus, &mut raw) }.ok()?;
    Some(rect_of(raw))
}

pub fn sample_background(screen: &ScreenDc, caret: Rect, anchor: i32, rtl: bool) -> Rgb {
    let direction = if rtl { -1 } else { 1 };
    let mut pixels = Vec::new();
    for row in 1..=SAMPLE_ROWS {
        for column in 0..SAMPLE_COLUMNS {
            let x = anchor + direction * (SAMPLE_STEP / 2 + column * SAMPLE_STEP);
            let y = caret.y + caret.height * row / (SAMPLE_ROWS + 1);
            let pixel = unsafe { GetPixel(screen.0, x, y) }.0;
            if pixel != CLR_INVALID {
                pixels.push(Rgb::from_bgr(pixel));
            }
        }
    }
    median_by_luminance(pixels).unwrap_or(Rgb::WHITE)
}
