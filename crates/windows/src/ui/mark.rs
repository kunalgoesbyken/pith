use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::UI::WindowsAndMessaging::{HICON, LoadIconW};
use windows::core::PCWSTR;

use super::canvas::Canvas;
use super::theme::Rgb;
use crate::brand::{
    BOWL_CENTER, BOWL_RADIUS, HALF_STROKE, MARK_HEIGHT, MARK_WIDTH, STEM_BOTTOM, STEM_TOP, STEM_X,
};

const APP_ICON_ID: usize = 1;

pub fn mark_width(height: i32) -> i32 {
    (height as f32 * MARK_WIDTH / MARK_HEIGHT).round() as i32
}

pub fn paint_mark(canvas: &Canvas, origin: (i32, i32), height: i32, stem: Rgb, ghost: Rgb) {
    let unit = height as f32 / MARK_HEIGHT;
    let at = |x: f32, y: f32| (origin.0 as f32 + x * unit, origin.1 as f32 + y * unit);
    let thickness = 2.0 * HALF_STROKE * unit;
    let (cx, cy) = at(BOWL_CENTER.0, BOWL_CENTER.1);
    canvas.ring((cx, cy), BOWL_RADIUS * unit, thickness, ghost);
    let (top, bottom) = (at(STEM_X, STEM_TOP), at(STEM_X, STEM_BOTTOM));
    canvas.line(top, bottom, thickness, stem);
    canvas.disc(top, HALF_STROKE * unit, stem);
    canvas.disc(bottom, HALF_STROKE * unit, stem);
}

pub fn app_icon(instance: HINSTANCE) -> HICON {
    unsafe { LoadIconW(Some(instance), PCWSTR(APP_ICON_ID as *const u16)) }.unwrap_or_default()
}
