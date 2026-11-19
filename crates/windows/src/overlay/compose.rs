use pith_core::{
    Form, Label, Placed, Placement, Rect, Shading, Surface, TextStyle, fit_cuts, is_rtl,
};
use windows::Win32::Foundation::POINT;

use super::anchored;
use super::hint::Hint;
use super::inline::{self, Backdrop};
use super::pill;
use super::raster::Raster;
use super::scene::{Frame, Scene};
use super::screen::{Monitor, ScreenDc};
use crate::ui::Theme;

pub struct Request<'a> {
    pub labels: &'a [Label],
    pub placement: Placement,
    pub text: &'a TextStyle,
    pub theme: Theme,
    pub key_hint: Option<&'a str>,
}

pub fn compose<R>(
    request: &Request,
    monitor: Monitor,
    screen: &ScreenDc,
    field: Option<Rect>,
    finish: impl FnOnce(&Frame, Placed, usize) -> R,
) -> Option<R> {
    let form = Form::of(request.placement, true);
    let hint = match form {
        Form::Ghost(_) => None,
        Form::UnderCaret(_) | Form::UnderField(_) => {
            request.key_hint.map(|key| Hint::new(screen, monitor, key))
        }
    };
    let face = match form {
        Form::Ghost(caret) => inline::face(screen, monitor, caret, request.text),
        Form::UnderCaret(_) | Form::UnderField(_) => anchored::face(screen, monitor),
    };
    let measure = |segment: &str| face.measure(segment);
    let surface = Surface {
        work: monitor.work,
        scale: monitor.scale,
        field,
        line_height: face.line_height,
        measure: &measure,
        elevated: !request.theme.is_contrast(),
        keycap: hint.as_ref().map(Hint::keycap),
    };
    let (index, placed) = fit_cuts(request.labels, form, &surface)?;
    let Label { text, strong } = &request.labels[index];
    let frame = Frame {
        text,
        strong,
        rtl: is_rtl(text),
        screen,
        monitor,
        face: &face,
        hint: hint.as_ref(),
    };
    Some(finish(&frame, placed, index))
}

pub fn render<'a>(
    frame: &Frame<'a>,
    request: &Request,
    placed: Placed,
    backdrop: Backdrop,
) -> Option<(Raster<'a>, POINT)> {
    let shading = match Form::of(request.placement, true) {
        Form::Ghost(_) => inline::shading(request.text, backdrop),
        Form::UnderCaret(_) | Form::UnderField(_) => {
            Shading::pill(pill::style(&request.theme), &placed, frame.monitor.scale)
        }
    };
    let destination = POINT {
        x: placed.destination.x,
        y: placed.destination.y,
    };
    let scene = Scene::new(frame, placed, shading);
    Some((Raster::render(&scene)?, destination))
}
