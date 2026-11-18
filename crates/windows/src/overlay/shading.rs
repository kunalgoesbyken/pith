use pith_core::Ink;

use super::pill::{Pill, Style};

const FULL_COVERAGE: u32 = 255;

pub enum Shading {
    Ink { ink: Ink, strong_alpha: u8 },
    Pill { style: Style, shape: Pill },
}

impl Shading {
    pub fn pixel(&self, column: i32, row: i32, coverage: u32, strong: bool) -> u32 {
        match self {
            Self::Ink { ink, strong_alpha } => {
                let ceiling = u32::from(if strong { *strong_alpha } else { ink.alpha });
                ink.color.premultiplied(coverage * ceiling / FULL_COVERAGE)
            }
            Self::Pill { style, shape } => style.pixel(*shape, column, row, coverage, strong),
        }
    }
}
