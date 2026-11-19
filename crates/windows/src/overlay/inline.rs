use pith_core::{Rect, Rgb, Shading, TextStyle, inline_font_px};

use super::face::{FALLBACK_FAMILY, Face};
use super::screen::{Monitor, ScreenDc};

pub struct Backdrop {
    pub color: Rgb,
    pub opacity: u8,
}

pub fn face(screen: &ScreenDc, monitor: Monitor, caret: Rect, style: &TextStyle) -> Face {
    Face::new(
        screen,
        style.family.as_deref().unwrap_or(FALLBACK_FAMILY),
        inline_font_px(caret, style, monitor.scale),
    )
}

pub fn shading(style: &TextStyle, backdrop: Backdrop) -> Shading {
    Shading::inline(style.color, backdrop.color, backdrop.opacity)
}
