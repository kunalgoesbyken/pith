use pith_core::blend;
use windows::Win32::Foundation::RECT;

use super::canvas::{Align, Canvas, Text, measure};
use super::control::{
    ChipState, FOCUS_MARGIN, Interaction, Seam, Surface, TOGGLE_HEIGHT, TOGGLE_WIDTH, accent_fill,
    pick,
};
use super::keycap::{key_labels, keycaps, keycaps_width};
use super::metrics::{
    FOCUS_GAP, FOCUS_RING, RADIUS_CONTROL, Role, STROKE, Scale, height, inset, rect, square, width,
};
use super::shape::{Corner, Shape};
use super::theme::{Palette, Rgb, Theme};

const KNOB: i32 = 16;
const KNOB_PAD: i32 = 3;
const TOGGLE_SHADE: u32 = 30;
const SEGMENT_PAD: i32 = 3;
pub const NAV_PAD: i32 = 12;
const NAV_ICON: i32 = 16;
const NAV_ICON_GAP: i32 = 10;
const NAV_HOVER_WASH: u32 = 150;
const CHIP_PAD: i32 = 16;
const TRACK_HEIGHT: i32 = 4;
const THUMB: i32 = 18;
const THUMB_GROW: i32 = 2;

fn wash(palette: &Palette, state: Interaction, rest: Rgb) -> Rgb {
    pick(state, rest, palette.control_hover, palette.control_pressed)
}

pub(super) fn focus_ring(canvas: &Canvas, scale: Scale, shape: Shape, color: Rgb) {
    let halo = shape.inset(-scale.px(FOCUS_MARGIN));
    canvas.stroke_shape(halo, color, scale.px(FOCUS_RING));
}

fn nav_hover(surface: &Surface) -> Rgb {
    let palette = &surface.theme.palette;
    if surface.theme.is_contrast() {
        return palette.control_hover;
    }
    blend(palette.selected, surface.backdrop, NAV_HOVER_WASH)
}

fn toggle_fill(palette: &Palette, contrast: bool, on: bool, state: Interaction) -> Rgb {
    if state.disabled {
        return if contrast {
            palette.track
        } else {
            palette.control_stroke
        };
    }
    if on {
        return accent_fill(palette, state);
    }
    let shade = |alpha| blend(palette.text, palette.track, alpha);
    pick(
        state,
        palette.track,
        shade(TOGGLE_SHADE),
        shade(2 * TOGGLE_SHADE),
    )
}

pub fn draw_toggle(canvas: &Canvas, surface: &Surface, bounds: RECT, on: bool, state: Interaction) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    let pill_height = scale.px(TOGGLE_HEIGHT);
    let left = bounds.left + scale.px(FOCUS_MARGIN);
    let top = bounds.top + (height(bounds) - pill_height) / 2;
    let body = rect(left, top, scale.px(TOGGLE_WIDTH), pill_height);
    let pill = canvas.shape(body, Corner::Pill);
    let contrast = surface.theme.is_contrast();
    canvas.fill_shape(pill, toggle_fill(palette, contrast, on, state));
    if contrast && state.disabled {
        canvas.stroke_shape(pill, palette.text_secondary, scale.px(STROKE));
    } else if contrast && !on {
        canvas.stroke_shape(pill, palette.strong_stroke, scale.px(STROKE));
    }
    let lit = on && !state.disabled;
    let travel = scale.px(KNOB_PAD + KNOB / 2);
    let knob_x = if on {
        body.right - travel
    } else {
        body.left + travel
    };
    let knob = square((knob_x, top + pill_height / 2), scale.px(KNOB));
    let knob_color = if lit {
        palette.accent.on
    } else if contrast && state.disabled {
        palette.text_secondary
    } else {
        palette.knob
    };
    canvas.fill_shape(canvas.shape(knob, Corner::Pill), knob_color);
    if state.focused {
        focus_ring(canvas, scale, pill, palette.accent.base);
    }
}

struct SegmentLook {
    fill: Option<Rgb>,
    text: Rgb,
    role: Role,
}

fn segment_look(theme: &Theme, selected: bool, state: Interaction) -> SegmentLook {
    let palette = &theme.palette;
    let role = if selected {
        Role::BodyStrong
    } else {
        Role::Body
    };
    if state.disabled {
        return SegmentLook {
            fill: selected.then_some(palette.card),
            text: palette.text_secondary,
            role,
        };
    }
    let engaged = selected || state.hot || state.pressed;
    if theme.is_contrast() {
        return SegmentLook {
            fill: engaged.then_some(palette.accent.base),
            text: if engaged {
                palette.accent.on
            } else {
                palette.text
            },
            role,
        };
    }
    SegmentLook {
        fill: (selected || state.pressed).then_some(palette.card),
        text: if engaged {
            palette.text
        } else {
            palette.text_secondary
        },
        role,
    }
}

fn segment_track(canvas: &Canvas, scale: Scale, bounds: RECT, seam: Seam) -> Shape {
    let reach = scale.px(RADIUS_CONTROL);
    let joined = RECT {
        left: bounds.left - if seam.joins_before() { reach } else { 0 },
        right: bounds.right + if seam.joins_after() { reach } else { 0 },
        ..bounds
    };
    canvas.shape(joined, Corner::Control)
}

pub fn draw_segment(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    (selected, label, seam): (bool, &str, Seam),
    state: Interaction,
) {
    let scale = surface.scale;
    let theme = &surface.theme;
    let palette = &theme.palette;
    let track = segment_track(canvas, scale, bounds, seam);
    canvas.clipped(bounds, || {
        canvas.fill_shape(track, palette.well);
        if theme.is_contrast() {
            canvas.stroke_shape(track, palette.control_stroke, scale.px(STROKE));
        }
    });
    let piece = canvas.shape(inset(bounds, scale.px(SEGMENT_PAD)), Corner::Key);
    let look = segment_look(theme, selected, state);
    if let Some(fill) = look.fill {
        if selected {
            canvas.shadow(piece, theme.shadow_over(palette.well));
        }
        canvas.fill_shape(piece, fill);
    }
    let style = Text::new(look.role, look.text).aligned(Align::Center);
    canvas.text(bounds, label, style);
    if state.focused {
        focus_ring(canvas, scale, piece, palette.accent.base);
    }
}

pub fn draw_nav(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    (selected, label, icon): (bool, &str, char),
    state: Interaction,
) {
    let scale = surface.scale;
    let theme = &surface.theme;
    let palette = &theme.palette;
    let shape = canvas.shape(bounds, Corner::Control);
    let hovered = state.hot || state.pressed;
    if selected {
        canvas.fill_shape(shape, palette.selected);
    } else if hovered {
        canvas.fill_shape(shape, nav_hover(surface));
    }
    let engaged = selected || hovered;
    let ink = if engaged && theme.is_contrast() {
        palette.accent.on
    } else {
        palette.text
    };
    let icon_ink = if engaged { ink } else { palette.text_secondary };
    let icon_bounds = rect(
        bounds.left + scale.px(NAV_PAD),
        bounds.top + (height(bounds) - scale.px(NAV_ICON)) / 2,
        scale.px(NAV_ICON),
        scale.px(NAV_ICON),
    );
    canvas.text(
        icon_bounds,
        &icon.to_string(),
        Text::new(Role::Icon, icon_ink).aligned(Align::Center),
    );
    let role = if selected {
        Role::BodyStrong
    } else {
        Role::Body
    };
    let text_area = RECT {
        left: icon_bounds.right + scale.px(NAV_ICON_GAP),
        right: bounds.right - scale.px(NAV_PAD),
        ..bounds
    };
    canvas.text(text_area, label, Text::new(role, ink));
    if state.focused {
        canvas.stroke_shape(shape, palette.accent.base, scale.px(FOCUS_RING));
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
    let body = inset(bounds, scale.px(FOCUS_MARGIN));
    let shape = canvas.shape(body, Corner::Control);
    let rest = wash(palette, state, palette.card);
    match chip {
        ChipState::Keys => {
            if state.hot || state.pressed {
                canvas.fill_shape(shape.inset(-scale.px(FOCUS_GAP)), rest);
            }
            let origin = (body.left, body.top);
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
            canvas.fill_shape(shape, rest);
            canvas.stroke_shape(shape, edge, thickness);
            let style = Text::new(Role::Body, color).aligned(Align::Center);
            canvas.text(body, label, style);
        }
    }
    if state.focused && chip != ChipState::Recording {
        focus_ring(canvas, scale, shape, palette.accent.base);
    }
}

fn thumb_radius(scale: Scale) -> i32 {
    scale.px(THUMB + THUMB_GROW) / 2
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
    canvas.fill_shape(canvas.shape(track, Corner::Pill), palette.track);
    let center = (
        track.left + (fraction * width(track) as f32).round() as i32,
        middle,
    );
    let filled = RECT {
        right: center.0,
        ..track
    };
    canvas.fill_shape(canvas.shape(filled, Corner::Pill), palette.accent.base);
    let grown = if state.hot || state.pressed {
        scale.px(THUMB_GROW)
    } else {
        0
    };
    let thumb = canvas.shape(square(center, scale.px(THUMB) + grown), Corner::Pill);
    canvas.shadow(thumb, surface.theme.shadow_over(surface.backdrop));
    canvas.card(thumb, palette.card, palette.control_stroke);
    if state.focused {
        focus_ring(canvas, scale, thumb, palette.accent.base);
    }
}
