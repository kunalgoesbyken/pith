use pith_core::{Emphasis, Placed, Rgb, Shading, argb};

use super::typeset::{Font, Mask, Typesetter};

const OPAQUE: u32 = 255;

pub(crate) struct Subject<'a> {
    pub(crate) text: &'a str,
    pub(crate) font: Font<'a>,
    pub(crate) placed: &'a Placed,
    pub(crate) emphasis: &'a Emphasis,
    pub(crate) shading: &'a Shading,
}

fn flatten(pixel: u32, background: Rgb) -> u32 {
    let alpha = pixel >> 24;
    let blend = |shift: u32, behind: u32| {
        (((pixel >> shift) & 0xFF) + behind * (OPAQUE - alpha) / OPAQUE).min(OPAQUE)
    };
    let [red, green, blue] = background.channels();
    argb(OPAQUE, [blend(16, red), blend(8, green), blend(0, blue)])
}

pub(crate) fn render(
    typesetter: &Typesetter,
    subject: &Subject,
    opaque_over: Option<Rgb>,
) -> Vec<u32> {
    let Subject {
        text,
        font,
        placed,
        emphasis,
        shading,
    } = subject;
    let mut mask = Mask::new(placed.size.width, placed.size.height);
    for line in &placed.lines {
        typesetter.draw(
            font,
            &text[line.start..line.end],
            &mut mask,
            line.x - placed.origin.x,
            line.y - placed.origin.y,
        );
    }
    let mut pixels = Vec::with_capacity(mask.coverage.len());
    for row in 0..mask.height {
        for column in 0..mask.width {
            let pixel = shading.pixel(
                column,
                row,
                u32::from(mask.at(column, row)),
                emphasis.covers(column, row),
            );
            pixels.push(match opaque_over {
                Some(background) => flatten(pixel, background),
                None => pixel,
            });
        }
    }
    pixels
}
