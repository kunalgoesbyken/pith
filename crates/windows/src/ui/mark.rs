use windows::Win32::Foundation::{HINSTANCE, RECT};
use windows::Win32::UI::WindowsAndMessaging::{HICON, LoadIconW};
use windows::core::PCWSTR;

use super::canvas::{Canvas, Text, measure};
use super::metrics::{Role, Scale, height, rect};
use super::shape::Corner;
use super::theme::Palette;

const APP_ICON_ID: usize = 1;
const WORDMARK: &str = "pith";
const CARET_WIDTH: i32 = 2;
const CARET_GAP: i32 = 2;
const CARET_EM: f32 = 0.9;

pub fn paint_wordmark(canvas: &Canvas, scale: Scale, bounds: RECT, palette: &Palette) {
    let style = Text::new(Role::Wordmark, palette.text);
    canvas.text(bounds, WORDMARK, style);
    let word = measure(scale, WORDMARK, style, 0).0;
    let caret_height = scale.size(Role::Wordmark.size() * CARET_EM).round() as i32;
    let caret = rect(
        bounds.left + word + scale.px(CARET_GAP),
        bounds.top + (height(bounds) - caret_height) / 2,
        scale.px(CARET_WIDTH),
        caret_height,
    );
    canvas.fill_shape(canvas.shape(caret, Corner::Pill), palette.mark_caret);
}

pub fn app_icon(instance: HINSTANCE) -> HICON {
    unsafe { LoadIconW(Some(instance), PCWSTR(APP_ICON_ID as *const u16)) }.unwrap_or_default()
}
