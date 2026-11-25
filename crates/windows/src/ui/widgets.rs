use windows::Win32::Foundation::RECT;

use super::canvas::{Align, Canvas, Text, measure};
use super::control::{
    ChipState, FOCUS_MARGIN, Interaction, Seam, Surface, TOGGLE_HEIGHT, TOGGLE_WIDTH, accent_fill,
    pick,
};
use super::keycap::{key_labels, keycaps, keycaps_width};
use super::metrics::{
    FOCUS_GAP, FOCUS_RING, Role, STROKE, Scale, height, inset, rect, square, width,
};
use super::theme::{Palette, Rgb};

const KNOB: i32 = 12;
const KNOB_PAD: i32 = 4;
const NAV_BAR_WIDTH: i32 = 3;
const NAV_BAR_HEIGHT: i32 = 16;
const NAV_PAD: i32 = 16;
const NAV_HOVER_WASH: u32 = 140;
const CHIP_PAD: i32 = 16;
const TRACK_HEIGHT: i32 = 4;
const THUMB: i32 = 20;
const THUMB_CORE: i32 = 12;
const THUMB_REST_SHRINK: i32 = 1;

fn wash(palette: &Palette, state: Interaction, rest: Rgb) -> Rgb {
    pick(state, rest, palette.control_hover, palette.control_pressed)
}

fn focus_ring(canvas: &Canvas, scale: Scale, bounds: RECT, color: Rgb) {
    canvas.stroke(bounds, color, scale.px(FOCUS_RING));
}

pub fn draw_toggle(canvas: &Canvas, surface: &Surface, bounds: RECT, on: bool, state: Interaction) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    let pill_width = scale.px(TOGGLE_WIDTH);
    let pill_height = scale.px(TOGGLE_HEIGHT);
    let left = bounds.left + scale.px(FOCUS_MARGIN);
    let top = bounds.top + (height(bounds) - pill_height) / 2;
    let pill = rect(left, top, pill_width, pill_height);
    let (knob_color, knob_x) = if on {
        canvas.fill(pill, accent_fill(palette, state));
        (
            palette.accent.on,
            pill.right - scale.px(KNOB_PAD + KNOB / 2),
        )
    } else {
        if state.hot || state.pressed {
            canvas.fill(pill, wash(palette, state, palette.card));
        }
        canvas.stroke(pill, palette.strong_stroke, scale.px(STROKE));
        (
            palette.text_secondary,
            pill.left + scale.px(KNOB_PAD + KNOB / 2),
        )
    };
    let knob = square((knob_x, top + pill_height / 2), scale.px(KNOB));
    canvas.fill(knob, knob_color);
    if state.focused {
        let halo = inset(pill, -scale.px(FOCUS_MARGIN));
        focus_ring(canvas, scale, halo, palette.accent.base);
    }
}

pub fn draw_segment(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    (selected, label, seam): (bool, &str, Seam),
    state: Interaction,
) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    let reach = scale.px(STROKE);
    let shape = RECT {
        left: bounds.left - if seam.joins_before() { reach } else { 0 },
        right: bounds.right + if seam.joins_after() { reach } else { 0 },
        ..bounds
    };
    let (fill, edge, text) = if selected {
        (
            accent_fill(palette, state),
            palette.accent.base,
            palette.accent.on,
        )
    } else {
        let text = if state.disabled {
            palette.text_secondary
        } else {
            palette.text
        };
        let fill = wash(palette, state, palette.control);
        (fill, palette.control_stroke, text)
    };
    canvas.clipped(bounds, || {
        canvas.fill(shape, fill);
        canvas.stroke(shape, edge, scale.px(STROKE));
        if seam.joins_before() && !selected {
            let divider = rect(bounds.left, bounds.top, scale.px(STROKE), height(bounds));
            canvas.fill(divider, palette.control_stroke);
        }
    });
    let style = Text::new(Role::Body, text).aligned(Align::Center);
    canvas.text(bounds, label, style);
    if state.focused {
        let color = if selected {
            palette.accent.on
        } else {
            palette.accent.base
        };
        let inner = inset(bounds, scale.px(FOCUS_RING));
        focus_ring(canvas, scale, inner, color);
    }
}

pub fn draw_nav(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    (selected, label): (bool, &str),
    state: Interaction,
) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    if selected {
        canvas.card(bounds, palette.card, palette.card_stroke);
        let bar = rect(
            bounds.left + scale.px(STROKE),
            bounds.top + (height(bounds) - scale.px(NAV_BAR_HEIGHT)) / 2,
            scale.px(NAV_BAR_WIDTH),
            scale.px(NAV_BAR_HEIGHT),
        );
        canvas.fill(bar, palette.accent.base);
    } else if state.hot || state.pressed {
        let wash = pith_core::blend(palette.card, surface.backdrop, NAV_HOVER_WASH);
        canvas.fill(bounds, wash);
    }
    let role = if selected {
        Role::BodyStrong
    } else {
        Role::Body
    };
    let text_area = RECT {
        left: bounds.left + scale.px(NAV_PAD),
        right: bounds.right - scale.px(NAV_PAD / 2),
        ..bounds
    };
    canvas.text(text_area, label, Text::new(role, palette.text));
    if state.focused {
        focus_ring(canvas, scale, bounds, palette.accent.base);
    }
}

pub fn chip_width(scale: Scale, state: ChipState, label: &str) -> i32 {
    match state {
        ChipState::Keys => keycaps_width(scale, key_labels(label)),
        ChipState::Empty | ChipState::Recording => {
            let style = Text::new(Role::Body, Rgb::BLACK);
            measure(scale, label, style, 0).0 + 2 * scale.px(CHIP_PAD)
        }
    }
}

pub fn draw_chip(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    (chip, label): (ChipState, &str),
    state: Interaction,
) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    let shape = inset(bounds, scale.px(FOCUS_MARGIN));
    let rest = wash(palette, state, palette.card);
    match chip {
        ChipState::Keys => {
            if state.hot || state.pressed {
                canvas.fill(inset(shape, -scale.px(FOCUS_GAP)), rest);
            }
            let origin = (shape.left, shape.top);
            keycaps(canvas, (scale, palette), origin, key_labels(label));
        }
        ChipState::Empty | ChipState::Recording => {
            let recording = chip == ChipState::Recording;
            let (edge, thickness, color) = if recording {
                (palette.accent.base, scale.px(FOCUS_RING), palette.text)
            } else {
                (
                    palette.control_stroke,
                    scale.px(STROKE),
                    palette.text_secondary,
                )
            };
            canvas.fill(shape, rest);
            canvas.stroke(shape, edge, thickness);
            let style = Text::new(Role::Body, color).aligned(Align::Center);
            canvas.text(shape, label, style);
        }
    }
    if state.focused && chip != ChipState::Recording {
        focus_ring(canvas, scale, bounds, palette.accent.base);
    }
}

fn thumb_radius(scale: Scale) -> i32 {
    scale.px(THUMB) / 2
}

fn slider_reach(scale: Scale) -> i32 {
    thumb_radius(scale) + scale.px(FOCUS_MARGIN)
}

pub fn slider_fraction_at(scale: Scale, span: i32, x: i32) -> f32 {
    let reach = slider_reach(scale);
    let travel = (span - 2 * reach).max(1);
    ((x - reach) as f32 / travel as f32).clamp(0.0, 1.0)
}

pub fn draw_slider(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    fraction: f32,
    state: Interaction,
) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    let reach = slider_reach(scale);
    let middle = bounds.top + height(bounds) / 2;
    let track_height = scale.px(TRACK_HEIGHT);
    let track = RECT {
        left: bounds.left + reach,
        right: bounds.right - reach,
        top: middle - track_height / 2,
        bottom: middle - track_height / 2 + track_height,
    };
    canvas.fill(track, palette.track);
    let center = (
        track.left + (fraction * width(track) as f32).round() as i32,
        middle,
    );
    let filled = RECT {
        right: center.0,
        ..track
    };
    canvas.fill(filled, palette.accent.base);
    let outer = thumb_radius(scale);
    canvas.fill(square(center, 2 * outer), palette.control_stroke);
    canvas.fill(square(center, 2 * (outer - scale.px(STROKE))), palette.card);
    let core = scale.px(THUMB_CORE) / 2;
    let shrink = if state.hot || state.pressed {
        0
    } else {
        scale.px(THUMB_REST_SHRINK)
    };
    canvas.fill(
        square(center, 2 * (core - shrink)),
        accent_fill(palette, state),
    );
    if state.focused {
        let extent = outer + scale.px(FOCUS_GAP) + scale.px(FOCUS_RING);
        focus_ring(
            canvas,
            scale,
            square(center, 2 * extent),
            palette.accent.base,
        );
    }
}
