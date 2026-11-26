use windows::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_AUTORADIOBUTTON, BS_CHECKBOX, BS_NOTIFY, BS_OWNERDRAW, CreateWindowExW, HMENU,
    WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_GROUP, WS_TABSTOP, WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

use super::canvas::{Align, Canvas, Text, measure, paint};
use super::hover;
use super::metrics::{FOCUS_GAP, FOCUS_RING, Role, STROKE, Scale, height, inset, origin, width};
use super::shape::Corner;
use super::skin;
use super::theme::{Palette, Rgb, Theme};
use super::widgets;
use crate::system::util::wide;

pub const TOGGLE_WIDTH: i32 = 40;
pub const TOGGLE_HEIGHT: i32 = 22;
pub const BUTTON_HEIGHT: i32 = 32;
pub const SLIDER_WIDTH: i32 = 200;
pub const FOCUS_MARGIN: i32 = FOCUS_RING + FOCUS_GAP;
const BUTTON_PAD: i32 = 16;
const BUTTON_MIN: i32 = 88;
const SEGMENT_MIN: i32 = 72;
const SS_OWNERDRAW: u32 = 0x0D;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Standard,
    Accent,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seam {
    Only,
    First,
    Middle,
    Last,
}

impl Seam {
    pub fn of(index: usize, count: usize) -> Self {
        match (index, count) {
            (_, 0 | 1) => Self::Only,
            (0, _) => Self::First,
            (last, count) if last + 1 == count => Self::Last,
            _ => Self::Middle,
        }
    }

    pub fn joins_before(self) -> bool {
        matches!(self, Self::Middle | Self::Last)
    }

    pub fn joins_after(self) -> bool {
        matches!(self, Self::First | Self::Middle)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipState {
    Keys,
    Empty,
    Recording,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look<'a> {
    Toggle {
        on: bool,
    },
    Segment {
        selected: bool,
        label: &'a str,
        seam: Seam,
    },
    Button {
        tone: Tone,
        label: &'a str,
    },
    Nav {
        selected: bool,
        label: &'a str,
        icon: char,
    },
    Chip {
        state: ChipState,
        label: &'a str,
    },
    Slider {
        fraction: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub theme: Theme,
    pub scale: Scale,
    pub backdrop: Rgb,
}

#[derive(Clone, Copy)]
pub(super) struct Interaction {
    pub hot: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
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
    raised: bool,
    text: Rgb,
}

pub(super) fn pick(state: Interaction, rest: Rgb, hot: Rgb, pressed: Rgb) -> Rgb {
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
        raised: true,
        text: match (state.disabled, engaged) {
            (true, _) => palette.text_secondary,
            (false, true) => palette.accent.on,
            (false, false) => text,
        },
    }
}

pub(super) fn accent_fill(palette: &Palette, state: Interaction) -> Rgb {
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
        raised: false,
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
    let scale = surface.scale;
    let body = inset(bounds, scale.px(FOCUS_MARGIN));
    let shape = canvas.shape(body, Corner::Control);
    let look = appearance(&surface.theme, tone, state);
    if look.raised {
        canvas.shadow(shape, surface.theme.shadow_over(surface.backdrop));
    }
    canvas.fill_shape(shape, look.fill);
    if let Some(stroke) = look.stroke {
        canvas.stroke_shape(shape, stroke, scale.px(STROKE));
    }
    let style = Text::new(Role::Body, look.text).aligned(Align::Center);
    canvas.text(body, label, style);
    if state.focused {
        let accent = surface.theme.palette.accent.base;
        widgets::focus_ring(canvas, scale, shape, accent);
    }
}

pub fn draw(item: &DRAWITEMSTRUCT, surface: &Surface, look: Look) {
    let state = Interaction::of(item);
    let local = origin(item.rcItem);
    paint(item.hDC, item.rcItem, surface.scale, |canvas| {
        canvas.clear(surface.backdrop);
        match look {
            Look::Toggle { on } => widgets::draw_toggle(canvas, surface, local, on, state),
            Look::Segment {
                selected,
                label,
                seam,
            } => widgets::draw_segment(canvas, surface, local, (selected, label, seam), state),
            Look::Button { tone, label } => {
                draw_button(canvas, surface, local, (tone, label), state);
            }
            Look::Nav {
                selected,
                label,
                icon,
            } => widgets::draw_nav(canvas, surface, local, (selected, label, icon), state),
            Look::Chip { state: chip, label } => {
                widgets::draw_chip(canvas, surface, local, (chip, label), state);
            }
            Look::Slider { fraction } => {
                widgets::draw_slider(canvas, surface, local, fraction, state);
            }
        }
    });
}

pub fn paint_backdrop(item: &DRAWITEMSTRUCT, surface: &Surface, draw: impl FnOnce(&Canvas, RECT)) {
    let local = origin(item.rcItem);
    paint(item.hDC, item.rcItem, surface.scale, |canvas| {
        canvas.clear(surface.backdrop);
        draw(canvas, local);
    });
}

pub fn toggle_width(scale: Scale) -> i32 {
    scale.px(TOGGLE_WIDTH)
}

pub fn button_width(scale: Scale, label: &str) -> i32 {
    let style = Text::new(Role::Body, Rgb::BLACK);
    let wanted = measure(scale, label, style, 0).0 + 2 * scale.px(BUTTON_PAD);
    wanted.max(scale.px(BUTTON_MIN))
}

pub fn segment_width<'a>(scale: Scale, labels: impl Iterator<Item = &'a str>) -> i32 {
    let style = Text::new(Role::BodyStrong, Rgb::BLACK);
    labels
        .map(|label| measure(scale, label, style, 0).0 + 2 * scale.px(BUTTON_PAD))
        .fold(scale.px(SEGMENT_MIN), i32::max)
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

    fn native(&self, id: i32, name: &str, style: u32, bounds: RECT) -> windows::core::Result<HWND> {
        let button = self.child(
            w!("BUTTON"),
            name,
            style | BS_NOTIFY as u32 | WS_TABSTOP.0,
            id,
            bounds,
        )?;
        hover::attach(button);
        Ok(button)
    }

    pub fn button(&self, id: i32, name: &str, bounds: RECT) -> windows::core::Result<HWND> {
        self.native(id, name, BS_OWNERDRAW as u32 | WS_GROUP.0, bounds)
    }

    pub fn check(&self, id: i32, name: &str, bounds: RECT) -> windows::core::Result<HWND> {
        let button = self.native(id, name, BS_CHECKBOX as u32 | WS_GROUP.0, bounds)?;
        skin::attach(button);
        Ok(button)
    }

    pub fn radio(
        &self,
        id: i32,
        name: &str,
        bounds: RECT,
        first_in_group: bool,
    ) -> windows::core::Result<HWND> {
        let group = if first_in_group { WS_GROUP.0 } else { 0 };
        let button = self.native(id, name, BS_AUTORADIOBUTTON as u32 | group, bounds)?;
        skin::attach(button);
        Ok(button)
    }

    pub fn label(&self, id: i32, text: &str, bounds: RECT) -> windows::core::Result<HWND> {
        self.child(w!("STATIC"), text, SS_OWNERDRAW, id, bounds)
    }
}
