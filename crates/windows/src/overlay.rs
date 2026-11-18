use std::ffi::c_void;

use pith_core::{
    FADE_ALPHAS, FADE_FRAME_MS, Flow, Line, Placement, Rect, anchor_pill, flow_bounds, ink, is_rtl,
    layout, lines_that_fit, median_by_luminance, next_word, next_word_alpha, strong_span,
};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::pill::{Pill, Style};
use crate::ui::{Role, STROKE, Scale, Theme};
use crate::util::{rect_of, wide};

const PILL_PADDING_X: i32 = 10;
const PILL_PADDING_Y: i32 = 5;
const PILL_GAP: i32 = 4;
const PILL_MIN_WIDTH: i32 = 240;
const WHITE: u32 = 0x00FF_FFFF;
const CLR_INVALID: u32 = 0xFFFF_FFFF;
const FALLBACK_FONT: &str = "Segoe UI";
const FADE_TIMER: usize = 1;
const BASE_DPI: u32 = 96;
const SAMPLE_COLUMNS: i32 = 4;
const SAMPLE_ROWS: i32 = 3;
const SAMPLE_STEP: i32 = 8;

fn apply_fade(hwnd: HWND, step: usize) {
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: FADE_ALPHAS[step.min(FADE_ALPHAS.len() - 1)],
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    unsafe {
        let _ = UpdateLayeredWindow(
            hwnd,
            None,
            None,
            None,
            None,
            None,
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
    }
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER if wparam.0 == FADE_TIMER => unsafe {
            let step =
                (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize + 1).min(FADE_ALPHAS.len() - 1);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, step as isize);
            apply_fade(hwnd, step);
            if step + 1 >= FADE_ALPHAS.len() {
                let _ = KillTimer(Some(hwnd), FADE_TIMER);
            }
            return LRESULT(0);
        },
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn monitor_of(caret: Rect) -> (Rect, u32) {
    unsafe {
        let point = POINT {
            x: caret.x + caret.width / 2,
            y: caret.y + caret.height / 2,
        };
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let work = if GetMonitorInfoW(monitor, &mut info).as_bool() {
            rect_of(info.rcWork)
        } else {
            Rect {
                x: caret.x - 4096,
                y: caret.y - 4096,
                width: 8192,
                height: 8192,
            }
        };
        let (mut dpi_x, mut dpi_y) = (BASE_DPI, BASE_DPI);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        (work, dpi_x.max(BASE_DPI))
    }
}

fn focused_field() -> Option<Rect> {
    unsafe {
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(0, &mut info).ok()?;
        let mut raw = RECT::default();
        GetWindowRect(info.hwndFocus, &mut raw).ok()?;
        Some(rect_of(raw))
    }
}

fn sample_background(caret: Rect, anchor: i32, rtl: bool) -> u32 {
    let direction = if rtl { -1 } else { 1 };
    let mut pixels = Vec::new();
    unsafe {
        let screen = GetDC(None);
        for row in 1..=SAMPLE_ROWS {
            for column in 0..SAMPLE_COLUMNS {
                let x = anchor + direction * (SAMPLE_STEP / 2 + column * SAMPLE_STEP);
                let y = caret.y + caret.height * row / (SAMPLE_ROWS + 1);
                let pixel = GetPixel(screen, x, y).0;
                if pixel != CLR_INVALID {
                    pixels.push(pixel);
                }
            }
        }
        ReleaseDC(None, screen);
    }
    median_by_luminance(pixels).unwrap_or(WHITE)
}

struct Face {
    memory: HDC,
    font: HFONT,
    previous: HGDIOBJ,
    line_height: i32,
}

impl Face {
    fn new(screen: HDC, family: &str, px: i32) -> Self {
        unsafe {
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
            let previous = SelectObject(memory, font.into());
            let mut metrics = TEXTMETRICW::default();
            let _ = GetTextMetricsW(memory, &mut metrics);
            Self {
                memory,
                font,
                previous,
                line_height: metrics.tmHeight.max(1),
            }
        }
    }

    fn measure(&self, text: &str) -> i32 {
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

struct Picture<'a> {
    text: &'a str,
    lines: &'a [Line],
    rtl: bool,
    origin: POINT,
    destination: POINT,
    width: i32,
    height: i32,
}

pub struct Overlay {
    hwnd: HWND,
    visible: bool,
    background: u32,
    style: Style,
    opacity: u8,
}

impl Overlay {
    pub fn create(opacity: u8) -> windows::core::Result<Self> {
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
                background: WHITE,
                style: Style::of(&Theme::detect()),
                opacity,
            })
        }
    }

    pub fn set_opacity(&mut self, opacity: u8) {
        self.opacity = opacity;
    }

    pub fn show(
        &mut self,
        text: &str,
        placement: Placement,
        font_family: Option<&str>,
        font_px: Option<f32>,
        color: Option<u32>,
    ) {
        match placement {
            Placement::Inline(caret) => self.show_inline(text, caret, font_family, font_px, color),
            Placement::Anchored(element) => self.show_anchored(text, element),
        }
    }

    fn show_inline(
        &mut self,
        text: &str,
        caret: Rect,
        font_family: Option<&str>,
        font_px: Option<f32>,
        color: Option<u32>,
    ) {
        let rtl = is_rtl(text);
        let anchor = if rtl {
            caret.x
        } else {
            caret.x + caret.width.max(1)
        };
        if !self.visible {
            self.background = sample_background(caret, anchor, rtl);
        }
        let (work, dpi) = monitor_of(caret);
        let scale = |value: i32| value * dpi as i32 / BASE_DPI as i32;
        let px = font_px
            .map_or(((caret.height as f32) * 0.72).round() as i32, |p| {
                p.round() as i32
            })
            .clamp(scale(9), scale(72));
        let paint = ink(color, self.background, self.opacity);
        let strong = next_word_alpha(paint.alpha);
        let strong_end = next_word(text).len();
        unsafe {
            let screen = GetDC(None);
            let face = Face::new(screen, font_family.unwrap_or(FALLBACK_FONT), px);
            let measure = |segment: &str| face.measure(segment);
            let top = caret.y + (caret.height - face.line_height) / 2;
            let bounds = flow_bounds(focused_field(), caret, work);
            let flow = Flow {
                caret,
                top,
                line_height: face.line_height,
                bounds,
                rtl,
                max_lines: lines_that_fit(bounds, top, face.line_height),
            };
            let lines = layout(text, &flow, &measure);
            if lines.is_empty() {
                ReleaseDC(None, screen);
                drop(face);
                self.hide();
                return;
            }
            let left = lines.iter().map(|line| line.x).min().unwrap_or(0);
            let right = lines
                .iter()
                .map(|line| line.x + line.width)
                .max()
                .unwrap_or(1);
            let first = lines[0].y;
            let height = lines
                .last()
                .map_or(1, |line| line.y - first + face.line_height);
            let width = (right - left).max(1);
            let spans: Vec<_> = lines
                .iter()
                .map(|line| strong_span(text, line, strong_end, rtl, &measure))
                .collect();
            let rows = row_spans(&lines, &spans, left, first, face.line_height, height);
            let (red, green, blue) = (
                paint.color & 0xFF,
                (paint.color >> 8) & 0xFF,
                (paint.color >> 16) & 0xFF,
            );
            let shade = |column: i32, row: i32, coverage: u32| {
                let emphasised =
                    rows[row as usize].is_some_and(|(from, to)| (from..to).contains(&column));
                let ceiling = u32::from(if emphasised { strong } else { paint.alpha });
                let alpha = coverage * ceiling / 255;
                (alpha << 24)
                    | ((red * alpha / 255) << 16)
                    | ((green * alpha / 255) << 8)
                    | (blue * alpha / 255)
            };
            let origin = POINT { x: left, y: first };
            let picture = Picture {
                text,
                lines: &lines,
                rtl,
                origin,
                destination: origin,
                width,
                height,
            };
            self.draw(&face, screen, &picture, &shade);
            drop(face);
            ReleaseDC(None, screen);
        }
        self.reveal();
    }

    fn show_anchored(&mut self, text: &str, element: Rect) {
        let rtl = is_rtl(text);
        let (work, dpi) = monitor_of(element);
        let scale = Scale(dpi);
        if !self.visible {
            self.style = Style::of(&Theme::detect());
        }
        let strong_end = next_word(text).len();
        let (padding_x, padding_y) = (scale.px(PILL_PADDING_X), scale.px(PILL_PADDING_Y));
        let room = element.width.max(scale.px(PILL_MIN_WIDTH)).min(work.width) - 2 * padding_x;
        unsafe {
            let screen = GetDC(None);
            let face = Face::new(
                screen,
                FALLBACK_FONT,
                scale.size(Role::Body.size()).round() as i32,
            );
            let measure = |segment: &str| face.measure(segment);
            let flow = Flow {
                caret: Rect {
                    x: if rtl { room } else { -1 },
                    y: 0,
                    width: 0,
                    height: face.line_height,
                },
                top: 0,
                line_height: face.line_height,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    width: room,
                    height: face.line_height,
                },
                rtl,
                max_lines: 1,
            };
            let lines = layout(text, &flow, &measure);
            let height = face.line_height + 2 * padding_y;
            let placed = lines.first().and_then(|line| {
                anchor_pill(
                    element,
                    line.width + 2 * padding_x,
                    height,
                    scale.px(PILL_GAP),
                    work,
                    rtl,
                )
                .map(|pill| (*line, pill))
            });
            let Some((line, pill)) = placed else {
                ReleaseDC(None, screen);
                drop(face);
                self.hide();
                return;
            };
            let origin = POINT {
                x: line.x - padding_x,
                y: -padding_y,
            };
            let span = [strong_span(text, &line, strong_end, rtl, &measure)];
            let rows = row_spans(&[line], &span, origin.x, origin.y, face.line_height, height);
            let shape = Pill {
                width: pill.width,
                height: pill.height,
                edge: scale.px(STROKE).max(1),
            };
            let style = self.style;
            let shade = |column: i32, row: i32, coverage: u32| {
                let emphasised =
                    rows[row as usize].is_some_and(|(from, to)| (from..to).contains(&column));
                style.pixel(shape, column, row, coverage, emphasised)
            };
            let picture = Picture {
                text,
                lines: &[line],
                rtl,
                origin,
                destination: POINT {
                    x: pill.x,
                    y: pill.y,
                },
                width: pill.width,
                height: pill.height,
            };
            self.draw(&face, screen, &picture, &shade);
            drop(face);
            ReleaseDC(None, screen);
        }
        self.reveal();
    }

    fn draw(
        &self,
        face: &Face,
        screen: HDC,
        picture: &Picture,
        shade: &dyn Fn(i32, i32, u32) -> u32,
    ) {
        let (width, height) = (picture.width, picture.height);
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
        unsafe {
            let Ok(bitmap) =
                CreateDIBSection(Some(face.memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                return;
            };
            let previous_bitmap = SelectObject(face.memory, bitmap.into());
            SetBkMode(face.memory, TRANSPARENT);
            SetTextColor(face.memory, COLORREF(WHITE));
            for line in picture.lines {
                let units: Vec<u16> = picture.text[line.start..line.end].encode_utf16().collect();
                let options = if picture.rtl {
                    ETO_RTLREADING
                } else {
                    ETO_OPTIONS(0)
                };
                let _ = ExtTextOutW(
                    face.memory,
                    line.x - picture.origin.x,
                    line.y - picture.origin.y,
                    options,
                    None,
                    PCWSTR(units.as_ptr()),
                    units.len() as u32,
                    None,
                );
            }
            let pixels =
                std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
            for (row, band) in pixels.chunks_exact_mut(width as usize).enumerate() {
                for (column, pixel) in band.iter_mut().enumerate() {
                    let coverage = ((*pixel >> 16) & 0xFF)
                        .max((*pixel >> 8) & 0xFF)
                        .max(*pixel & 0xFF);
                    *pixel = shade(column as i32, row as i32, coverage);
                }
            }
            let step = if self.visible {
                GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as usize
            } else {
                0
            };
            let extent = SIZE {
                cx: width,
                cy: height,
            };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: FADE_ALPHAS[step.min(FADE_ALPHAS.len() - 1)],
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&picture.destination),
                Some(&extent),
                Some(face.memory),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            SelectObject(face.memory, previous_bitmap);
            let _ = DeleteObject(bitmap.into());
        }
    }

    fn reveal(&mut self) {
        unsafe {
            if !self.visible {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                SetTimer(Some(self.hwnd), FADE_TIMER, FADE_FRAME_MS, None);
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
                let _ = KillTimer(Some(self.hwnd), FADE_TIMER);
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }
}

fn row_spans(
    lines: &[Line],
    spans: &[Option<(i32, i32)>],
    left: i32,
    first: i32,
    line_height: i32,
    height: i32,
) -> Vec<Option<(i32, i32)>> {
    let mut rows = vec![None; height as usize];
    for (line, span) in lines.iter().zip(spans) {
        let Some((from, to)) = span else {
            continue;
        };
        let origin = line.x - left;
        let top = (line.y - first) as usize;
        for row in rows.iter_mut().skip(top).take(line_height as usize) {
            *row = Some((origin + from, origin + to));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: i32, y: i32, width: i32) -> Line {
        Line {
            start: 0,
            end: 0,
            x,
            y,
            width,
        }
    }

    #[test]
    fn emphasis_covers_only_the_rows_of_the_line_that_has_it() {
        let lines = [line(10, 100, 50), line(0, 120, 80)];
        let rows = row_spans(&lines, &[Some((0, 30)), None], 0, 100, 20, 40);
        assert_eq!(rows.len(), 40);
        assert!(rows[..20].iter().all(|row| *row == Some((10, 40))));
        assert!(rows[20..].iter().all(Option::is_none));
    }

    #[test]
    fn emphasis_is_relative_to_the_left_edge_of_the_picture() {
        let lines = [line(210, 50, 50)];
        let rows = row_spans(&lines, &[Some((20, 50))], 200, 50, 10, 10);
        assert!(rows.iter().all(|row| *row == Some((30, 60))));
    }

    #[test]
    fn a_line_taller_than_the_picture_is_clipped() {
        let lines = [line(0, 5, 10)];
        let rows = row_spans(&lines, &[Some((0, 10))], 0, 0, 20, 8);
        assert_eq!(rows.iter().filter(|row| row.is_some()).count(), 3);
    }
}
