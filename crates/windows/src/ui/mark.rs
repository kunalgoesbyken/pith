use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::UI::WindowsAndMessaging::{HICON, LoadIconW};
use windows::core::PCWSTR;

use super::canvas::Canvas;
use super::theme::Rgb;
use pith_core::{Block, MARK_HEIGHT, MARK_WIDTH, TRAIL, trail_colors};

const APP_ICON_ID: usize = 1;

pub fn mark_width(height: i32) -> i32 {
    (height as f32 * MARK_WIDTH / MARK_HEIGHT).round() as i32
}

pub fn paint_mark(canvas: &Canvas, origin: (i32, i32), height: i32, caret: Rgb, trail: Rgb) {
    let scale = height as f32 / MARK_HEIGHT;
    let origin = (origin.0 as f32, origin.1 as f32);
    for (block, color) in TRAIL.iter().zip(trail_colors(caret, trail)) {
        let Block {
            x,
            y,
            width,
            height,
            radius,
        } = block.scaled(scale, origin);
        canvas.fill_round_at((x, y), (width, height), radius, color);
    }
}

pub fn app_icon(instance: HINSTANCE) -> HICON {
    unsafe { LoadIconW(Some(instance), PCWSTR(APP_ICON_ID as *const u16)) }.unwrap_or_default()
}
