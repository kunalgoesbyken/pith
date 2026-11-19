use windows::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_OWNERDRAW, CreateWindowExW, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_TABSTOP,
    WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

use super::canvas::{Align, Canvas, Text, paint};
use super::hover;
use super::metrics::{
    CONTROL_RADIUS, FOCUS_GAP, FOCUS_RING, Role, STROKE, Scale, height, inset, origin, rect, width,
};
use super::theme::{Palette, Rgb, Theme};
use crate::util::wide;

pub const TOGGLE_WIDTH: i32 = 40;
pub const TOGGLE_HEIGHT: i32 = 20;
pub const BUTTON_HEIGHT: i32 = 32;
pub const FOCUS_MARGIN: i32 = FOCUS_RING + FOCUS_GAP;
const KNOB: i32 = 12;
const KNOB_PAD: i32 = 4;
const TOGGLE_LABEL_GAP: i32 = 12;
const SS_OWNERDRAW: u32 = 0x0D;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Standard,
    Accent,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look<'a> {
    Toggle { on: bool, label: &'a str },
    Segment { selected: bool, label: &'a str },
    Button { tone: Tone, label: &'a str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub theme: Theme,
    pub scale: Scale,
    pub backdrop: Rgb,
}

#[derive(Clone, Copy)]
struct Interaction {
    hot: bool,
    pressed: bool,
    focused: bool,
    disabled: bool,
}

impl Interaction {
    fn of(item: &DRAWITEMSTRUCT) -> Self {
        Self {
            hot: hover::is_hot(item.hwndItem),
            pressed: item.itemState.0 & ODS_SELECTED.0 != 0,
            focused: item.itemState.0 & ODS_FOCUS.0 != 0
                && item.itemState.0 & ODS_NOFOCUSRECT.0 == 0,
            disabled: item.itemState.0 & ODS_DISABLED.0 != 0,
        }
    }
}

struct Fill {
    fill: Rgb,
    stroke: Option<Rgb>,
    text: Rgb,
}

fn pick(state: Interaction, rest: Rgb, hot: Rgb, pressed: Rgb) -> Rgb {
    match (state.pressed, state.hot) {
        (true, _) => pressed,
        (false, true) => hot,
        (false, false) => rest,
    }
}

fn standard(theme: &Theme, state: Interaction, text: Rgb) -> Fill {
    let palette = &theme.palette;
    let engaged = theme.is_contrast() && (state.hot || state.pressed);
    Fill {
        fill: pick(
            state,
            palette.control,
            palette.control_hover,
            palette.control_pressed,
        ),
        stroke: Some(palette.control_stroke),
        text: match (state.disabled, engaged) {
            (true, _) => palette.text_secondary,
            (false, true) => palette.accent.on,
            (false, false) => text,
        },
    }
}

fn accent_fill(palette: &Palette, state: Interaction) -> Rgb {
    let accent = &palette.accent;
    pick(state, accent.base, accent.hover, accent.pressed)
}

fn accent(theme: &Theme, state: Interaction) -> Fill {
    let palette = &theme.palette;
    if state.disabled {
        return standard(theme, state, palette.text_secondary);
    }
    Fill {
        fill: accent_fill(palette, state),
        stroke: None,
        text: palette.accent.on,
    }
}

fn appearance(theme: &Theme, tone: Tone, state: Interaction) -> Fill {
    match tone {
        Tone::Standard => standard(theme, state, theme.palette.text),
        Tone::Danger => standard(theme, state, theme.palette.danger),
        Tone::Accent => accent(theme, state),
    }
}

fn draw_button(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    (tone, label): (Tone, &str),
    state: Interaction,
) {
    let margin = surface.scale.px(FOCUS_MARGIN);
    let shape = inset(bounds, margin);
    let radius = surface.scale.px(CONTROL_RADIUS);
    let look = appearance(&surface.theme, tone, state);
    canvas.fill_round(shape, radius, look.fill);
    if let Some(stroke) = look.stroke {
        canvas.stroke_round(shape, radius, stroke, surface.scale.px(STROKE));
    }
    let style = Text::new(Role::Body, look.text).aligned(Align::Center);
    canvas.text(shape, label, style);
    if state.focused {
        let ring = surface.scale.px(FOCUS_RING);
        canvas.stroke_round(
            bounds,
            radius + margin,
            surface.theme.palette.accent.base,
            ring,
        );
    }
}

fn draw_toggle(
    canvas: &Canvas,
    surface: &Surface,
    bounds: RECT,
    on: bool,
    label: &str,
    state: Interaction,
) {
    let scale = surface.scale;
    let palette = &surface.theme.palette;
    let pill_width = scale.px(TOGGLE_WIDTH);
    let pill_height = scale.px(TOGGLE_HEIGHT);
    let left = bounds.right - scale.px(FOCUS_MARGIN) - pill_width;
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
            canvas.fill_round(
                pill,
                radius,
                pick(
                    state,
                    palette.card,
                    palette.control_hover,
                    palette.control_pressed,
                ),
            );
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
    let gap = scale.px(TOGGLE_LABEL_GAP);
    let label_area = RECT {
        left: bounds.left,
        top: bounds.top,
        right: pill.left - gap,
        bottom: bounds.bottom,
    };
    canvas.text(
        label_area,
        label,
        Text::new(Role::Body, palette.text).aligned(Align::End),
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

pub fn draw(item: &DRAWITEMSTRUCT, surface: &Surface, look: Look) {
    let state = Interaction::of(item);
    let local = origin(item.rcItem);
    paint(item.hDC, item.rcItem, surface.scale, |canvas| {
        canvas.clear(Some(surface.backdrop));
        match look {
            Look::Toggle { on, label } => draw_toggle(canvas, surface, local, on, label, state),
            Look::Segment { selected, label } => {
                let tone = if selected {
                    Tone::Accent
                } else {
                    Tone::Standard
                };
                draw_button(canvas, surface, local, (tone, label), state);
            }
            Look::Button { tone, label } => {
                draw_button(canvas, surface, local, (tone, label), state);
            }
        }
    });
}

pub fn paint_backdrop(item: &DRAWITEMSTRUCT, surface: &Surface, draw: impl FnOnce(&Canvas, RECT)) {
    let local = origin(item.rcItem);
    paint(item.hDC, item.rcItem, surface.scale, |canvas| {
        canvas.clear(Some(surface.backdrop));
        draw(canvas, local);
    });
}

pub fn toggle_width(scale: Scale, label_width: i32) -> i32 {
    scale.px(TOGGLE_WIDTH + TOGGLE_LABEL_GAP) + label_width
}

pub fn expand(visual: RECT, scale: Scale) -> RECT {
    inset(visual, -scale.px(FOCUS_MARGIN))
}

pub struct Parent {
    pub hwnd: HWND,
    pub instance: HINSTANCE,
}

impl Parent {
    pub fn child(
        &self,
        class: PCWSTR,
        text: &str,
        style: u32,
        id: i32,
        bounds: RECT,
    ) -> windows::core::Result<HWND> {
        let text = wide(text);
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                PCWSTR(text.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                bounds.left,
                bounds.top,
                width(bounds),
                height(bounds),
                Some(self.hwnd),
                Some(HMENU(id as isize as _)),
                Some(self.instance),
                None,
            )
        }
    }

    pub fn button(&self, id: i32, name: &str, bounds: RECT) -> windows::core::Result<HWND> {
        let button = self.child(
            w!("BUTTON"),
            name,
            BS_OWNERDRAW as u32 | WS_TABSTOP.0,
            id,
            bounds,
        )?;
        hover::attach(button);
        Ok(button)
    }

    pub fn label(&self, id: i32, text: &str, bounds: RECT) -> windows::core::Result<HWND> {
        self.child(w!("STATIC"), text, SS_OWNERDRAW, id, bounds)
    }
}
