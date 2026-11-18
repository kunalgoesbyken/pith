use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateFontW, DEFAULT_CHARSET,
    DEFAULT_PITCH, DeleteDC, DeleteObject, FW_NORMAL, GetTextExtentPoint32W, GetTextMetricsW, HDC,
    HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, TEXTMETRICW,
};
use windows::core::PCWSTR;

use super::screen::ScreenDc;
use crate::util::wide;

pub const FALLBACK_FAMILY: &str = "Segoe UI";

pub struct Face {
    pub memory: HDC,
    pub line_height: i32,
    font: HFONT,
    previous: HGDIOBJ,
}

impl Face {
    pub fn new(screen: &ScreenDc, family: &str, px: i32) -> Self {
        unsafe {
            let memory = CreateCompatibleDC(Some(screen.0));
            let name = wide(family);
            let font = CreateFontW(
                -px,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                DEFAULT_PITCH.0 as u32,
                PCWSTR(name.as_ptr()),
            );
            let previous = SelectObject(memory, font.into());
            let mut metrics = TEXTMETRICW::default();
            let _ = GetTextMetricsW(memory, &mut metrics);
            Self {
                memory,
                line_height: metrics.tmHeight.max(1),
                font,
                previous,
            }
        }
    }

    pub fn measure(&self, text: &str) -> i32 {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        unsafe {
            let _ = GetTextExtentPoint32W(self.memory, &units, &mut size);
        }
        size.cx
    }
}

impl Drop for Face {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.previous);
            let _ = DeleteObject(self.font.into());
            let _ = DeleteDC(self.memory);
        }
    }
}
