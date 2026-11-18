use windows::Win32::Foundation::RECT;

pub const BASE_DPI: u32 = 96;
pub const CARD_RADIUS: i32 = 8;
pub const CONTROL_RADIUS: i32 = 4;
pub const FOCUS_RING: i32 = 2;
pub const FOCUS_GAP: i32 = 1;
pub const STROKE: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale(pub u32);

impl Scale {
    pub fn px(self, logical: i32) -> i32 {
        (logical * self.0 as i32 + BASE_DPI as i32 / 2) / BASE_DPI as i32
    }

    pub fn size(self, logical: f32) -> f32 {
        logical * self.0 as f32 / BASE_DPI as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Title,
    Subtitle,
    BodyStrong,
    Body,
    Caption,
}

impl Role {
    pub fn size(self) -> f32 {
        match self {
            Role::Title => 28.0,
            Role::Subtitle => 20.0,
            Role::BodyStrong | Role::Body => 14.0,
            Role::Caption => 12.0,
        }
    }

    pub fn weight(self) -> i32 {
        match self {
            Role::Title | Role::Subtitle | Role::BodyStrong => 600,
            Role::Body | Role::Caption => 400,
        }
    }

    pub fn line(self) -> i32 {
        match self {
            Role::Title => 36,
            Role::Subtitle => 28,
            Role::BodyStrong | Role::Body => 20,
            Role::Caption => 16,
        }
    }
}

pub fn rect(left: i32, top: i32, width: i32, height: i32) -> RECT {
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

pub fn width(bounds: RECT) -> i32 {
    bounds.right - bounds.left
}

pub fn height(bounds: RECT) -> i32 {
    bounds.bottom - bounds.top
}

pub fn inset(bounds: RECT, by: i32) -> RECT {
    RECT {
        left: bounds.left + by,
        top: bounds.top + by,
        right: bounds.right - by,
        bottom: bounds.bottom - by,
    }
}

pub fn offset(bounds: RECT, dx: i32, dy: i32) -> RECT {
    RECT {
        left: bounds.left + dx,
        top: bounds.top + dy,
        right: bounds.right + dx,
        bottom: bounds.bottom + dy,
    }
}

pub fn origin(bounds: RECT) -> RECT {
    offset(bounds, -bounds.left, -bounds.top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_rounds_to_the_nearest_pixel() {
        assert_eq!(Scale(96).px(100), 100);
        assert_eq!(Scale(144).px(100), 150);
        assert_eq!(Scale(192).px(100), 200);
        assert_eq!(Scale(120).px(15), 19);
        assert_eq!(Scale(144).size(12.0), 18.0);
    }

    #[test]
    fn rectangle_helpers_keep_size_and_move_edges() {
        let bounds = rect(10, 20, 100, 40);
        assert_eq!((width(bounds), height(bounds)), (100, 40));
        assert_eq!(inset(bounds, 5), rect(15, 25, 90, 30));
        assert_eq!(offset(bounds, -10, -20), rect(0, 0, 100, 40));
        assert_eq!(origin(bounds), rect(0, 0, 100, 40));
    }
}
