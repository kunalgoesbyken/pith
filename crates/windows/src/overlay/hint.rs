use pith_core::{KEYCAP_FONT_PX, KeycapLabel, Rect, keycap_label_origin};
use windows::Win32::Graphics::Gdi::{ETO_OPTIONS, HDC};

use super::face::{FALLBACK_FAMILY, Face};
use super::screen::{Monitor, ScreenDc};
use crate::ui::Scale;

pub struct Hint {
    label: String,
    face: Face,
    scale: Scale,
}

impl Hint {
    pub fn new(screen: &ScreenDc, monitor: Monitor, label: &str) -> Self {
        let size = monitor.scale.size(KEYCAP_FONT_PX).round() as i32;
        Self {
            label: label.to_string(),
            face: Face::new(screen, FALLBACK_FAMILY, size),
            scale: monitor.scale,
        }
    }

    pub fn keycap(&self) -> KeycapLabel {
        KeycapLabel {
            width: self.face.measure(&self.label),
            line_height: self.face.line_height,
        }
    }

    pub fn paint(&self, target: HDC, keycap: Rect) {
        let origin = keycap_label_origin(keycap, self.scale);
        self.face
            .paint(target, &self.label, (origin.x, origin.y), ETO_OPTIONS(0));
    }
}
