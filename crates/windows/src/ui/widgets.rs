use windows::Win32::Foundation::RECT;

use super::canvas::{Align, Canvas, Text, measure};
use super::control::{
    ChipState, FOCUS_MARGIN, Interaction, Seam, Surface, TOGGLE_HEIGHT, TOGGLE_WIDTH, accent_fill,
    pick,
};
use super::keycap::{key_labels, keycaps, keycaps_width};
use super::metrics::{
    CONTROL_RADIUS, FOCUS_GAP, FOCUS_RING, Role, STROKE, Scale, height, inset, rect, width,
};
use super::theme::Rgb;

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

pub fn draw_toggle(canvas: &Canvas, surface: &Surface, bounds: RECT, on: bool, state: Interaction) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    let pill_width = scale.px(TOGGLE_WIDTH);
    let pill_height = scale.px(TOGGLE_HEIGHT);
    let left = bounds.left + scale.px(FOCUS_MARGIN);
    let top = bounds.top + (height(bounds) - pill_height) / 2;
    let pill = rect(left, top, pill_width, pill_height);
    let radius = pill_height / 2;
    let (knob_color, knob_x) = if on {
        canvas.fill_round(pill, radius, accent_fill(palette, state));
        (
            palette.accent.on,
            pill.right - scale.px(KNOB_PAD + KNOB / 2),
        )
    } else {
        if state.hot || state.pressed {
            let wash = pick(
                state,
                palette.card,
                palette.control_hover,
                palette.control_pressed,
            );
            canvas.fill_round(pill, radius, wash);
        }
        canvas.stroke_round(pill, radius, palette.strong_stroke, scale.px(STROKE));
        (
            palette.text_secondary,
            pill.left + scale.px(KNOB_PAD + KNOB / 2),
        )
    };
    canvas.disc(
        (knob_x as f32, (top + pill_height / 2) as f32),
        scale.size(KNOB as f32 / 2.0),
        knob_color,
    );
    if state.focused {
        let margin = scale.px(FOCUS_MARGIN);
        let ring = scale.px(FOCUS_RING);
        canvas.stroke_round(
            inset(pill, -margin),
            radius + margin,
            palette.accent.base,
            ring,
        );
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
    let radius = scale.px(CONTROL_RADIUS);
    let reach = 2 * radius;
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
        let fill = pick(
            state,
            palette.control,
            palette.control_hover,
            palette.control_pressed,
        );
        (fill, palette.control_stroke, text)
    };
    canvas.clipped(bounds, || {
        canvas.fill_round(shape, radius, fill);
        canvas.stroke_round(shape, radius, edge, scale.px(STROKE));
        if seam.joins_before() && !selected {
            let divider = rect(bounds.left, bounds.top, scale.px(STROKE), height(bounds));
            canvas.fill_round(divider, 0, palette.control_stroke);
        }
    });
    let style = Text::new(Role::Body, text).aligned(Align::Center);
    canvas.text(bounds, label, style);
    if state.focused {
        let ring = scale.px(FOCUS_RING);
        let color = if selected {
            palette.accent.on
        } else {
            palette.accent.base
        };
        canvas.stroke_round(inset(bounds, ring), radius, color, ring);
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
    let radius = scale.px(CONTROL_RADIUS);
    if selected {
        canvas.card(bounds, radius, palette.card, palette.card_stroke);
        let bar = rect(
            bounds.left + scale.px(STROKE),
            bounds.top + (height(bounds) - scale.px(NAV_BAR_HEIGHT)) / 2,
            scale.px(NAV_BAR_WIDTH),
            scale.px(NAV_BAR_HEIGHT),
        );
        canvas.fill_round(bar, scale.px(NAV_BAR_WIDTH) / 2, palette.accent.base);
    } else if state.hot || state.pressed {
        let wash = pith_core::blend(palette.card, surface.backdrop, NAV_HOVER_WASH);
        canvas.fill_round(bounds, radius, wash);
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
        canvas.stroke_round(bounds, radius, palette.accent.base, scale.px(FOCUS_RING));
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
    let radius = scale.px(CONTROL_RADIUS);
    let margin = scale.px(FOCUS_MARGIN);
    let shape = inset(bounds, margin);
    let wash = pick(
        state,
        palette.card,
        palette.control_hover,
        palette.control_pressed,
    );
    match chip {
        ChipState::Keys => {
            if state.hot || state.pressed {
                canvas.fill_round(inset(shape, -scale.px(FOCUS_GAP)), radius, wash);
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
            canvas.fill_round(shape, radius, wash);
            canvas.stroke_round(shape, radius, edge, thickness);
            let style = Text::new(Role::Body, color).aligned(Align::Center);
            canvas.text(shape, label, style);
        }
    }
    if state.focused && chip != ChipState::Recording {
        canvas.stroke_round(
            bounds,
            radius + margin,
            palette.accent.base,
            scale.px(FOCUS_RING),
        );
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
    canvas.fill_round(track, track_height / 2, palette.track);
    let center = (
        track.left as f32 + fraction * width(track) as f32,
        middle as f32,
    );
    let filled = RECT {
        right: center.0.round() as i32,
        ..track
    };
    canvas.fill_round(filled, track_height / 2, palette.accent.base);
    let outer = thumb_radius(scale) as f32;
    canvas.disc(center, outer, palette.control_stroke);
    canvas.disc(center, outer - scale.size(STROKE as f32), palette.card);
    let core = scale.size(THUMB_CORE as f32 / 2.0);
    let shrink = if state.hot || state.pressed {
        0.0
    } else {
        scale.size(1.0)
    };
    canvas.disc(center, core - shrink, accent_fill(palette, state));
    if state.focused {
        let gap = scale.size(FOCUS_GAP as f32 + FOCUS_RING as f32 / 2.0);
        canvas.ring(
            center,
            outer + gap,
            scale.size(FOCUS_RING as f32),
            palette.accent.base,
        );
    }
}
