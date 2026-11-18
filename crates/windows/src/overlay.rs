use std::ffi::c_void;

use pith_core::Rect;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::util::wide;

const DARK_TEXT: u32 = 0x0000_0000;
const LIGHT_TEXT: u32 = 0x00FF_FFFF;
const GHOST_OPACITY: u32 = 120;
const FALLBACK_FONT: &str = "Segoe UI";

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn contrasting_color(x: i32, y: i32) -> u32 {
    let background = unsafe {
        let screen = GetDC(None);
        let pixel = GetPixel(screen, x, y);
        ReleaseDC(None, screen);
        pixel.0
    };
    let (red, green, blue) = (background & 0xFF, (background >> 8) & 0xFF, (background >> 16) & 0xFF);
    if red * 299 + green * 587 + blue * 114 < 128_000 {
        LIGHT_TEXT
    } else {
        DARK_TEXT
    }
}

pub struct Overlay {
    hwnd: HWND,
    visible: bool,
    fallback_color: u32,
}

impl Overlay {
    pub fn create() -> windows::core::Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("PithGhostText");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW,
                class,
                w!("pith"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            Ok(Self {
                hwnd,
                visible: false,
                fallback_color: DARK_TEXT,
            })
        }
    }

    pub fn show(
        &mut self,
        text: &str,
        caret: Rect,
        font_family: Option<&str>,
        font_px: Option<f32>,
        color: Option<u32>,
    ) {
        if color.is_none() && !self.visible {
            self.fallback_color =
                contrasting_color(caret.x + caret.width.max(1) + 2, caret.y + caret.height / 2);
        }
        let color = color.unwrap_or(self.fallback_color);
        let (red, green, blue) = (color & 0xFF, (color >> 8) & 0xFF, (color >> 16) & 0xFF);
        let px = font_px
            .map_or(((caret.height as f32) * 0.72).round() as i32, |p| {
                p.round() as i32
            })
            .clamp(9, 72);
        let family = font_family.unwrap_or(FALLBACK_FONT);
        unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
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
            let previous_font = SelectObject(memory, font.into());
            let text16: Vec<u16> = text.encode_utf16().collect();
            let mut size = SIZE::default();
            let _ = GetTextExtentPoint32W(memory, &text16, &mut size);
            let (width, height) = (size.cx.max(1), size.cy.max(1));
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            if let Ok(bitmap) =
                CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                let previous_bitmap = SelectObject(memory, bitmap.into());
                SetBkMode(memory, TRANSPARENT);
                SetTextColor(memory, COLORREF(0x00FF_FFFF));
                let _ = TextOutW(memory, 0, 0, &text16);
                for pixel in
                    std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize)
                {
                    let coverage = ((*pixel >> 16) & 0xFF)
                        .max((*pixel >> 8) & 0xFF)
                        .max(*pixel & 0xFF);
                    let alpha = coverage * GHOST_OPACITY / 255;
                    *pixel = (alpha << 24)
                        | ((red * alpha / 255) << 16)
                        | ((green * alpha / 255) << 8)
                        | (blue * alpha / 255);
                }
                let destination = POINT {
                    x: caret.x + caret.width.max(1),
                    y: caret.y + (caret.height - height) / 2,
                };
                let extent = SIZE {
                    cx: width,
                    cy: height,
                };
                let origin = POINT { x: 0, y: 0 };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let _ = UpdateLayeredWindow(
                    self.hwnd,
                    Some(screen),
                    Some(&destination),
                    Some(&extent),
                    Some(memory),
                    Some(&origin),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                SelectObject(memory, previous_bitmap);
                let _ = DeleteObject(bitmap.into());
            }
            SelectObject(memory, previous_font);
            let _ = DeleteObject(font.into());
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            if !self.visible {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }
}
