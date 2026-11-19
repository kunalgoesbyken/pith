use pith_core::{PILL_FONT_PX, PillStyle, Placed, Shading};

use super::face::{FALLBACK_FAMILY, Face};
use super::scene::Frame;
use super::screen::{Monitor, ScreenDc};

pub fn face(screen: &ScreenDc, monitor: Monitor) -> Face {
    Face::new(
        screen,
        FALLBACK_FAMILY,
        monitor.scale.size(PILL_FONT_PX).round() as i32,
    )
}

pub fn shading(frame: &Frame, placed: &Placed, style: PillStyle) -> Shading {
    Shading::pill(style, placed, frame.monitor.scale)
}
