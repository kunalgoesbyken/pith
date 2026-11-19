use pith_core::{
    Flow, Rect, Rgb, TextStyle, flow_bounds, ink, layout, lines_that_fit, next_word_alpha,
};
use windows::Win32::Foundation::{POINT, SIZE};

use super::face::{FALLBACK_FAMILY, Face};
use super::scene::{Frame, Placed, Scene};
use super::screen::{Monitor, ScreenDc};
use super::shading::Shading;
use crate::ui::Scale;

const FONT_TO_CARET_HEIGHT: f32 = 0.72;
const MIN_FONT_PX: i32 = 9;
const MAX_FONT_PX: i32 = 72;

pub struct Backdrop {
    pub color: Rgb,
    pub opacity: u8,
}

pub fn anchor(caret: Rect, rtl: bool) -> i32 {
    if rtl {
        caret.x
    } else {
        caret.x + caret.width.max(1)
    }
}

fn font_px(caret: Rect, style: &TextStyle, scale: Scale) -> i32 {
    let preferred = style
        .px
        .unwrap_or(caret.height as f32 * FONT_TO_CARET_HEIGHT)
        .round() as i32;
    preferred.clamp(scale.px(MIN_FONT_PX), scale.px(MAX_FONT_PX))
}

pub fn face(screen: &ScreenDc, monitor: Monitor, caret: Rect, style: &TextStyle) -> Face {
    Face::new(
        screen,
        style.family.as_deref().unwrap_or(FALLBACK_FAMILY),
        font_px(caret, style, monitor.scale),
    )
}

pub fn compose<'a>(
    frame: &Frame<'a>,
    caret: Rect,
    style: &TextStyle,
    backdrop: Backdrop,
) -> Option<Scene<'a>> {
    let face = frame.face;
    let top = caret.y + (caret.height - face.line_height) / 2;
    let bounds = flow_bounds(frame.field, caret, frame.monitor.work);
    let flow = Flow {
        caret,
        top,
        line_height: face.line_height,
        bounds,
        rtl: frame.rtl,
        max_lines: lines_that_fit(bounds, top, face.line_height),
    };
    let lines = layout(frame.text, &flow, &|segment| face.measure(segment));
    let left = lines.iter().map(|line| line.x).min()?;
    let right = lines.iter().map(|line| line.x + line.width).max()?;
    let (first, last) = (lines.first()?.y, lines.last()?.y);
    let origin = POINT { x: left, y: first };
    let placed = Placed {
        lines,
        origin,
        destination: origin,
        size: SIZE {
            cx: (right - left).max(1),
            cy: last - first + face.line_height,
        },
    };
    let paint = ink(style.color, backdrop.color, backdrop.opacity);
    let shading = Shading::Ink {
        ink: paint,
        strong_alpha: next_word_alpha(paint.alpha),
    };
    Some(Scene::new(frame, placed, shading))
}
