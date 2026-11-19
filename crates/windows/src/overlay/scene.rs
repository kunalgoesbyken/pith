use std::ops::Range;

use pith_core::{Line, Rect};
use windows::Win32::Foundation::{POINT, SIZE};

use super::emphasis::Emphasis;
use super::face::Face;
use super::screen::{Monitor, ScreenDc};
use super::shading::Shading;

#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub text: &'a str,
    pub strong: &'a Range<usize>,
    pub rtl: bool,
    pub screen: &'a ScreenDc,
    pub monitor: Monitor,
    pub face: &'a Face,
    pub field: Option<Rect>,
}

pub struct Placed {
    pub lines: Vec<Line>,
    pub origin: POINT,
    pub destination: POINT,
    pub size: SIZE,
}

pub struct Scene<'a> {
    pub text: &'a str,
    pub rtl: bool,
    pub face: &'a Face,
    pub placed: Placed,
    emphasis: Emphasis,
    shading: Shading,
}

impl<'a> Scene<'a> {
    pub fn new(frame: &Frame<'a>, placed: Placed, shading: Shading) -> Self {
        let face = frame.face;
        let emphasis = Emphasis::of(
            frame.text,
            frame.strong,
            &placed.lines,
            frame.rtl,
            face,
            placed.origin,
            placed.size.cy,
        );
        Self {
            text: frame.text,
            rtl: frame.rtl,
            face,
            placed,
            emphasis,
            shading,
        }
    }

    pub fn shade(&self, column: i32, row: i32, coverage: u32) -> u32 {
        let strong = self.emphasis.covers(column, row);
        self.shading.pixel(column, row, coverage, strong)
    }
}
