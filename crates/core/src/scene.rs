use crate::{
    Flow, Label, Line, Placement, Rect, Scale, TextStyle, anchor_pill, fits, flow_bounds, is_rtl,
    layout, lines_that_fit,
};

pub const PILL_FONT_PX: f32 = 14.0;
pub const KEYCAP_FONT_PX: f32 = 12.0;
pub const PILL_EDGE: i32 = 1;
pub const PILL_ELEVATION: i32 = 12;
pub const PILL_RADIUS: i32 = 6;
pub const PILL_WRAPPED_RADIUS: i32 = 8;
pub const KEYCAP_RADIUS: i32 = 4;
const FONT_TO_CARET_HEIGHT: f32 = 0.72;
const MIN_FONT_PX: i32 = 9;
const MAX_FONT_PX: i32 = 72;
const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 6;
const GAP: i32 = 6;
const KEYCAP_PADDING_X: i32 = 6;
const KEYCAP_PADDING_Y: i32 = 2;
const KEYCAP_GAP: i32 = 8;
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
    pub margin: i32,
    pub keycap: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeycapLabel {
    pub width: i32,
    pub line_height: i32,
}

impl KeycapLabel {
    fn size(self, scale: Scale) -> Size {
        Size {
            width: self.width + 2 * scale.px(KEYCAP_PADDING_X),
            height: self.line_height + 2 * scale.px(KEYCAP_PADDING_Y),
        }
    }
}

pub fn keycap_label_origin(keycap: Rect, scale: Scale) -> Point {
    Point {
        x: keycap.x + scale.px(KEYCAP_PADDING_X),
        y: keycap.y + scale.px(KEYCAP_PADDING_Y),
    }
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
    pub elevated: bool,
    pub keycap: Option<KeycapLabel>,
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
        margin: 0,
        keycap: None,
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
    let offered = match form {
        Form::Ghost(_) => None,
        Form::UnderCaret(_) | Form::UnderField(_) => surface.keycap,
    };
    labels
        .iter()
        .enumerate()
        .find_map(|(index, Label { text, .. })| {
            let rtl = is_rtl(text);
            offered
                .into_iter()
                .map(Some)
                .chain([None])
                .find_map(|keycap| {
                    let surface = Surface { keycap, ..*surface };
                    let placed = match form {
                        Form::Ghost(caret) => place_inline(&surface, text, rtl, caret),
                        Form::UnderCaret(caret) => place_under_caret(&surface, text, rtl, caret),
                        Form::UnderField(element) => {
                            place_under_field(&surface, text, rtl, element)
                        }
                    };
                    placed
                        .filter(|placed| fits(text, &placed.lines))
                        .map(|placed| (index, placed))
                })
        })
}

struct Metrics {
    padding_x: i32,
    padding_y: i32,
    margin: i32,
    keycap_gap: i32,
    keycap: Option<Size>,
}

impl Metrics {
    fn of(surface: &Surface) -> Self {
        let scale = surface.scale;
        Self {
            padding_x: scale.px(PADDING_X),
            padding_y: scale.px(PADDING_Y),
            margin: if surface.elevated {
                scale.px(PILL_ELEVATION)
            } else {
                0
            },
            keycap_gap: scale.px(KEYCAP_GAP),
            keycap: surface.keycap.map(|keycap| keycap.size(scale)),
        }
    }

    fn keycap_room(&self) -> i32 {
        self.keycap.map_or(0, |size| size.width + self.keycap_gap)
    }

    fn keycap_rect(&self, rtl: bool, text_width: i32, height: i32) -> Option<Rect> {
        let size = self.keycap?;
        let start = self.margin + self.padding_x;
        Some(Rect {
            x: if rtl {
                start
            } else {
                start + text_width + self.keycap_gap
            },
            y: self.margin + (height - size.height) / 2,
            width: size.width,
            height: size.height,
        })
    }
}

fn pill_flow(surface: &Surface, rtl: bool, room: i32) -> Flow {
    Flow {
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
    }
}

fn place_pill(surface: &Surface, text: &str, rtl: bool, anchor: Rect, room: i32) -> Option<Placed> {
    let metrics = Metrics::of(surface);
    let keycap_room = metrics.keycap_room();
    let room = room - 2 * metrics.padding_x - keycap_room;
    if room < 1 {
        return None;
    }
    let flow = pill_flow(surface, rtl, room);
    let lines = layout(text, &flow, surface.measure);
    if lines.first()?.y != flow.top {
        return None;
    }
    let left = lines.iter().map(|line| line.x).min()?;
    let right = lines.iter().map(|line| line.x + line.width).max()?;
    let height = lines.len() as i32 * surface.line_height + 2 * metrics.padding_y;
    let pill = anchor_pill(
        anchor,
        right - left + 2 * metrics.padding_x + keycap_room,
        height,
        surface.scale.px(GAP),
        surface.work,
        rtl,
    )?;
    let margin = metrics.margin;
    let shift = if rtl { keycap_room } else { 0 };
    Some(Placed {
        lines,
        origin: Point {
            x: left - metrics.padding_x - margin - shift,
            y: -metrics.padding_y - margin,
        },
        destination: Point {
            x: pill.x - margin,
            y: pill.y - margin,
        },
        size: Size {
            width: pill.width + 2 * margin,
            height: pill.height + 2 * margin,
        },
        margin,
        keycap: metrics.keycap_rect(rtl, right - left, height),
    })
}
