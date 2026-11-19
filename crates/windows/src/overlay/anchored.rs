use pith_core::{Flow, Rect, anchor_pill, flow_bounds, layout};
use windows::Win32::Foundation::{POINT, SIZE};

use super::face::{FALLBACK_FAMILY, Face};
use super::pill::{Pill, Style};
use super::scene::{Frame, Placed, Scene};
use super::screen::{Monitor, ScreenDc};
use super::shading::Shading;
use crate::ui::{Role, STROKE};

const PADDING_X: i32 = 10;
const PADDING_Y: i32 = 5;
const GAP: i32 = 4;
const MIN_WIDTH: i32 = 240;
const MAX_LINES: usize = 4;

pub fn face(screen: &ScreenDc, monitor: Monitor) -> Face {
    Face::new(
        screen,
        FALLBACK_FAMILY,
        monitor.scale.size(Role::Body.size()).round() as i32,
    )
}

pub fn under_field<'a>(frame: &Frame<'a>, element: Rect, style: Style) -> Option<Scene<'a>> {
    let monitor = frame.monitor;
    let room = element
        .width
        .max(monitor.scale.px(MIN_WIDTH))
        .min(monitor.work.width);
    compose(frame, element, room, style)
}

pub fn under_caret<'a>(frame: &Frame<'a>, caret: Rect, style: Style) -> Option<Scene<'a>> {
    let bounds = flow_bounds(frame.field, caret, frame.monitor.work);
    let room = if frame.rtl {
        caret.x + caret.width - bounds.x
    } else {
        bounds.x + bounds.width - caret.x
    };
    compose(frame, caret, room, style)
}

fn compose<'a>(frame: &Frame<'a>, anchor: Rect, room: i32, style: Style) -> Option<Scene<'a>> {
    let Frame {
        text, rtl, monitor, ..
    } = *frame;
    let (work, scale) = (monitor.work, monitor.scale);
    let (padding_x, padding_y) = (scale.px(PADDING_X), scale.px(PADDING_Y));
    let room = room - 2 * padding_x;
    if room < 1 {
        return None;
    }
    let face = frame.face;
    let flow = Flow {
        caret: Rect {
            x: if rtl { room } else { -1 },
            y: 0,
            width: 0,
            height: face.line_height,
        },
        top: 0,
        line_height: face.line_height,
        bounds: Rect {
            x: 0,
            y: 0,
            width: room,
            height: face.line_height * MAX_LINES as i32,
        },
        rtl,
        max_lines: MAX_LINES,
    };
    let lines = layout(text, &flow, &|segment| face.measure(segment));
    if lines.first()?.y != flow.top {
        return None;
    }
    let left = lines.iter().map(|line| line.x).min()?;
    let right = lines.iter().map(|line| line.x + line.width).max()?;
    let height = lines.len() as i32 * face.line_height + 2 * padding_y;
    let pill = anchor_pill(
        anchor,
        right - left + 2 * padding_x,
        height,
        scale.px(GAP),
        work,
        rtl,
    )?;
    let placed = Placed {
        lines,
        origin: POINT {
            x: left - padding_x,
            y: -padding_y,
        },
        destination: POINT {
            x: pill.x,
            y: pill.y,
        },
        size: SIZE {
            cx: pill.width,
            cy: pill.height,
        },
    };
    let shape = Pill {
        width: pill.width,
        height: pill.height,
        edge: scale.px(STROKE).max(1),
    };
    Some(Scene::new(frame, placed, Shading::Pill { style, shape }))
}
