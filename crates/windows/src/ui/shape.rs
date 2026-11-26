use windows::Win32::Foundation::RECT;

use super::metrics::{
    RADIUS_BAR, RADIUS_CARD, RADIUS_CONTROL, RADIUS_KEY, Scale, height, inset, offset, width,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    Card,
    Control,
    Key,
    Bar,
    Pill,
}

impl Corner {
    fn logical(self) -> Option<i32> {
        match self {
            Corner::Card => Some(RADIUS_CARD),
            Corner::Control => Some(RADIUS_CONTROL),
            Corner::Key => Some(RADIUS_KEY),
            Corner::Bar => Some(RADIUS_BAR),
            Corner::Pill => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub bounds: RECT,
    pub radius: f32,
}

impl Shape {
    pub fn new(scale: Scale, bounds: RECT, corner: Corner) -> Self {
        let limit = (width(bounds).min(height(bounds)) as f32 / 2.0).max(0.0);
        let radius = corner
            .logical()
            .map_or(limit, |logical| (scale.px(logical) as f32).min(limit));
        Self { bounds, radius }
    }

    pub fn inset(self, by: i32) -> Self {
        Self {
            bounds: inset(self.bounds, by),
            radius: (self.radius - by as f32).max(0.0),
        }
    }

    pub fn offset(self, dx: i32, dy: i32) -> Self {
        Self {
            bounds: offset(self.bounds, dx, dy),
            ..self
        }
    }
}
