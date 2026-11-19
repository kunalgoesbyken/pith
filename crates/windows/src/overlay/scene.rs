use std::ops::Range;

use pith_core::{Emphasis, Placed, Shading};

use super::face::Face;
use super::hint::Hint;
use super::screen::{Monitor, ScreenDc};

#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub text: &'a str,
    pub strong: &'a Range<usize>,
    pub rtl: bool,
    pub screen: &'a ScreenDc,
    pub monitor: Monitor,
    pub face: &'a Face,
    pub hint: Option<&'a Hint>,
}

pub struct Scene<'a> {
    pub text: &'a str,
    pub rtl: bool,
    pub face: &'a Face,
    pub hint: Option<&'a Hint>,
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
            &placed,
            frame.rtl,
            face.line_height,
            &|segment| face.measure(segment),
        );
        Self {
            text: frame.text,
            rtl: frame.rtl,
            face,
            hint: frame.hint,
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
