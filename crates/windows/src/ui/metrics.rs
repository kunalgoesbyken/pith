use windows::Win32::Foundation::RECT;

pub use pith_core::{BASE_DPI, Scale};

pub const FOCUS_RING: i32 = 2;
pub const FOCUS_GAP: i32 = 1;
pub const STROKE: i32 = 1;
pub const RADIUS_CARD: i32 = 12;
pub const RADIUS_CONTROL: i32 = 8;
pub const RADIUS_KEY: i32 = 6;
pub const RADIUS_BAR: i32 = 2;
pub const SHADOW_OFFSET: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typeface {
    Sans,
    Serif,
    Icons,
}

impl Typeface {
    pub const ALL: [Self; 3] = [Self::Sans, Self::Serif, Self::Icons];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Display,
    Wordmark,
    Stat,
    BodyStrong,
    Body,
    Caption,
    Icon,
}

impl Role {
    pub fn typeface(self) -> Typeface {
        match self {
            Role::Display | Role::Wordmark => Typeface::Serif,
            Role::Icon => Typeface::Icons,
            Role::Stat | Role::BodyStrong | Role::Body | Role::Caption => Typeface::Sans,
        }
    }

    pub fn size(self) -> f32 {
        match self {
            Role::Display => 30.0,
            Role::Wordmark | Role::Stat => 24.0,
            Role::BodyStrong | Role::Body => 14.0,
            Role::Caption => 12.0,
            Role::Icon => 16.0,
        }
    }

    pub fn weight(self) -> i32 {
        match self {
            Role::Stat | Role::BodyStrong => 600,
            Role::Display | Role::Wordmark | Role::Body | Role::Caption | Role::Icon => 400,
        }
    }

    pub fn line(self) -> i32 {
        match self {
            Role::Display => 40,
            Role::Wordmark | Role::Stat => 32,
            Role::BodyStrong | Role::Body => 20,
            Role::Caption | Role::Icon => 16,
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

pub fn square(center: (i32, i32), side: i32) -> RECT {
    rect(center.0 - side / 2, center.1 - side / 2, side, side)
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
