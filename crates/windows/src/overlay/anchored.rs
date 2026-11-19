use pith_core::PILL_FONT_PX;

use super::face::{FALLBACK_FAMILY, Face};
use super::screen::{Monitor, ScreenDc};

pub fn face(screen: &ScreenDc, monitor: Monitor) -> Face {
    Face::new(
        screen,
        FALLBACK_FAMILY,
        monitor.scale.size(PILL_FONT_PX).round() as i32,
    )
}
