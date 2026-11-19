use windows::Win32::Foundation::RECT;

pub use pith_core::{BASE_DPI, Scale};

pub const CARD_RADIUS: i32 = 8;
pub const CONTROL_RADIUS: i32 = 4;
pub const FOCUS_RING: i32 = 2;
pub const FOCUS_GAP: i32 = 1;
pub const STROKE: i32 = 1;

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
