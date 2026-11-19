use crate::{
    Flow, Label, Line, Placement, Rect, Scale, TextStyle, anchor_pill, fits, flow_bounds, is_rtl,
    layout, lines_that_fit,
};

pub const PILL_FONT_PX: f32 = 14.0;
pub const PILL_EDGE: i32 = 1;
const FONT_TO_CARET_HEIGHT: f32 = 0.72;
const MIN_FONT_PX: i32 = 9;
const MAX_FONT_PX: i32 = 72;
const PADDING_X: i32 = 10;
const PADDING_Y: i32 = 5;
const GAP: i32 = 4;
const MIN_WIDTH: i32 = 240;
const MAX_LINES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug)]
pub struct Placed {
    pub lines: Vec<Line>,
    pub origin: Point,
    pub destination: Point,
    pub size: Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Ghost(Rect),
    UnderCaret(Rect),
    UnderField(Rect),
}

impl Form {
    pub fn of(placement: Placement, ghost_allowed: bool) -> Self {
        match placement {
            Placement::Inline(caret) if ghost_allowed => Self::Ghost(caret),
            Placement::Inline(caret) | Placement::BelowCaret(caret) => Self::UnderCaret(caret),
            Placement::Anchored(element) => Self::UnderField(element),
        }
    }

    pub fn target(self) -> Rect {
        match self {
            Self::Ghost(rect) | Self::UnderCaret(rect) | Self::UnderField(rect) => rect,
        }
    }

    pub fn anchor(self, rtl: bool) -> i32 {
        match self {
            Self::Ghost(caret) | Self::UnderCaret(caret) => inline_anchor(caret, rtl),
            Self::UnderField(element) => element.x,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Surface<'a> {
    pub work: Rect,
    pub scale: Scale,
    pub field: Option<Rect>,
    pub line_height: i32,
    pub measure: &'a dyn Fn(&str) -> i32,
}

pub fn inline_anchor(caret: Rect, rtl: bool) -> i32 {
    if rtl {
        caret.x
    } else {
        caret.x + caret.width.max(1)
    }
}

pub fn inline_font_px(caret: Rect, style: &TextStyle, scale: Scale) -> i32 {
    let preferred = style
        .px
        .unwrap_or(caret.height as f32 * FONT_TO_CARET_HEIGHT)
        .round() as i32;
    preferred.clamp(scale.px(MIN_FONT_PX), scale.px(MAX_FONT_PX))
}

fn place_inline(surface: &Surface, text: &str, rtl: bool, caret: Rect) -> Option<Placed> {
    let top = caret.y + (caret.height - surface.line_height) / 2;
    let bounds = flow_bounds(surface.field, caret, surface.work);
    let flow = Flow {
        caret,
        top,
        line_height: surface.line_height,
        bounds,
        rtl,
        max_lines: lines_that_fit(bounds, top, surface.line_height),
    };
    let lines = layout(text, &flow, surface.measure);
    let left = lines.iter().map(|line| line.x).min()?;
    let right = lines.iter().map(|line| line.x + line.width).max()?;
    let (first, last) = (lines.first()?.y, lines.last()?.y);
    let origin = Point { x: left, y: first };
    Some(Placed {
        lines,
        origin,
        destination: origin,
        size: Size {
            width: (right - left).max(1),
            height: last - first + surface.line_height,
        },
    })
}

fn place_under_field(surface: &Surface, text: &str, rtl: bool, element: Rect) -> Option<Placed> {
    let room = element
        .width
        .max(surface.scale.px(MIN_WIDTH))
        .min(surface.work.width);
    place_pill(surface, text, rtl, element, room)
}

fn place_under_caret(surface: &Surface, text: &str, rtl: bool, caret: Rect) -> Option<Placed> {
    let bounds = flow_bounds(surface.field, caret, surface.work);
    let room = if rtl {
        caret.x + caret.width - bounds.x
    } else {
        bounds.x + bounds.width - caret.x
    };
    place_pill(surface, text, rtl, caret, room)
}

pub fn fit_cuts(labels: &[Label], form: Form, surface: &Surface) -> Option<(usize, Placed)> {
    labels
        .iter()
        .enumerate()
        .find_map(|(index, Label { text, .. })| {
            let rtl = is_rtl(text);
            let placed = match form {
                Form::Ghost(caret) => place_inline(surface, text, rtl, caret),
                Form::UnderCaret(caret) => place_under_caret(surface, text, rtl, caret),
                Form::UnderField(element) => place_under_field(surface, text, rtl, element),
            };
            placed
                .filter(|placed| fits(text, &placed.lines))
                .map(|placed| (index, placed))
        })
}

fn place_pill(surface: &Surface, text: &str, rtl: bool, anchor: Rect, room: i32) -> Option<Placed> {
    let scale = surface.scale;
    let (padding_x, padding_y) = (scale.px(PADDING_X), scale.px(PADDING_Y));
    let room = room - 2 * padding_x;
    if room < 1 {
        return None;
    }
    let flow = Flow {
        caret: Rect {
            x: if rtl { room } else { -1 },
            y: 0,
            width: 0,
            height: surface.line_height,
        },
        top: 0,
        line_height: surface.line_height,
        bounds: Rect {
            x: 0,
            y: 0,
            width: room,
            height: surface.line_height * MAX_LINES as i32,
        },
        rtl,
        max_lines: MAX_LINES,
    };
    let lines = layout(text, &flow, surface.measure);
    if lines.first()?.y != flow.top {
        return None;
    }
    let left = lines.iter().map(|line| line.x).min()?;
    let right = lines.iter().map(|line| line.x + line.width).max()?;
    let height = lines.len() as i32 * surface.line_height + 2 * padding_y;
    let pill = anchor_pill(
        anchor,
        right - left + 2 * padding_x,
        height,
        scale.px(GAP),
        surface.work,
        rtl,
    )?;
    Some(Placed {
        lines,
        origin: Point {
            x: left - padding_x,
            y: -padding_y,
        },
        destination: Point {
            x: pill.x,
            y: pill.y,
        },
        size: Size {
            width: pill.width,
            height: pill.height,
        },
    })
}
