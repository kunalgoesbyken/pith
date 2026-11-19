use pith_core::{Label, Placement, Rect, Rgb, TextStyle};

use super::compose::{Request, compose, render};
use super::inline::Backdrop;
use super::screen::{Monitor, ScreenDc};
use crate::ui::{Scale, Theme};

const WORK_REACH: i32 = 4096;

pub struct Sample<'a> {
    pub text: &'a str,
    pub placement: Placement,
    pub style: &'a TextStyle,
    pub dpi: u32,
    pub background: Rgb,
    pub opacity: u8,
    pub key_hint: Option<&'a str>,
}

pub struct Picture {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u32>,
}

pub fn preview(sample: &Sample) -> Option<Picture> {
    let labels = [Label::cut(sample.text)];
    let request = Request {
        labels: &labels,
        placement: sample.placement,
        text: sample.style,
        theme: Theme::detect(),
        key_hint: sample.key_hint,
    };
    let monitor = Monitor {
        work: Rect {
            x: 0,
            y: 0,
            width: WORK_REACH,
            height: WORK_REACH,
        },
        scale: Scale(sample.dpi),
    };
    let backdrop = Backdrop {
        color: sample.background,
        opacity: sample.opacity,
    };
    let screen = ScreenDc::get();
    compose(&request, monitor, &screen, None, |frame, placed, _| {
        let (raster, destination) = render(frame, &request, placed, backdrop)?;
        Some(Picture {
            x: destination.x,
            y: destination.y,
            width: raster.size.cx,
            height: raster.size.cy,
            pixels: raster.pixels(),
        })
    })
    .flatten()
}
