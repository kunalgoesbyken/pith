use crate::ghost::{best_plain, ink, next_word_alpha};
use crate::scene::{PILL_EDGE, Placed};
use crate::{Ink, Rgb, Scale, argb};

const FULL: f32 = 255.0;
const FULL_COVERAGE: u32 = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub card: Rgb,
    pub accent: Rgb,
    pub text: Rgb,
    pub text_secondary: Rgb,
}

impl Scheme {
    pub const LIGHT: Self = Self {
        card: Rgb::WHITE,
        accent: Rgb(0xCC4C08),
        text: Rgb(0x1A1A1A),
        text_secondary: Rgb(0x5D5D5D),
    };

    pub const DARK: Self = Self {
        card: Rgb(0x2B2B2B),
        accent: Rgb(0xF5A04A),
        text: Rgb::WHITE,
        text_secondary: Rgb(0xC5C5C5),
    };

    pub fn over(background: Rgb) -> Self {
        if best_plain(background) == Rgb::WHITE {
            Self::DARK
        } else {
            Self::LIGHT
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pill {
    pub width: i32,
    pub height: i32,
    pub edge: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PillStyle {
    pub fill: Rgb,
    pub stroke: Rgb,
    pub text: Rgb,
    pub strong: Rgb,
}

impl PillStyle {
    pub fn of(scheme: Scheme) -> Self {
        Self {
            fill: scheme.card,
            stroke: scheme.accent,
            text: scheme.text_secondary,
            strong: scheme.text,
        }
    }

    pub fn pixel(&self, pill: Pill, column: i32, row: i32, coverage: u32, strong: bool) -> u32 {
        let (half_width, half_height) = (pill.width as f32 / 2.0, pill.height as f32 / 2.0);
        let radius = half_width.min(half_height);
        let x = (column as f32 + 0.5 - half_width).abs() - (half_width - radius);
        let y = (row as f32 + 0.5 - half_height).abs() - (half_height - radius);
        let distance = x.max(0.0).hypot(y.max(0.0)) + x.max(y).min(0.0) - radius;
        let shape = (0.5 - distance).clamp(0.0, 1.0);
        let inner = (0.5 - distance - pill.edge as f32).clamp(0.0, 1.0);
        let ink = if strong { self.strong } else { self.text };
        let ink_weight = coverage.min(255) as f32 / FULL;
        let [stroke, fill, ink] = [self.stroke, self.fill, ink].map(Rgb::channels);
        let shown = std::array::from_fn(|index| {
            let channel = |values: [u32; 3]| values[index] as f32;
            let body = channel(stroke) + (channel(fill) - channel(stroke)) * inner;
            let shown = body + (channel(ink) - body) * ink_weight;
            (shown * shape).round() as u32
        });
        argb((shape * FULL).round() as u32, shown)
    }
}

pub enum Shading {
    Ink { ink: Ink, strong_alpha: u8 },
    Pill { style: PillStyle, shape: Pill },
}

impl Shading {
    pub fn inline(text_color: Option<Rgb>, background: Rgb, opacity: u8) -> Self {
        let paint = ink(text_color, background, opacity);
        Self::Ink {
            ink: paint,
            strong_alpha: next_word_alpha(paint.alpha),
        }
    }

    pub fn pill(style: PillStyle, placed: &Placed, scale: Scale) -> Self {
        Self::Pill {
            style,
            shape: Pill {
                width: placed.size.width,
                height: placed.size.height,
                edge: scale.px(PILL_EDGE).max(1),
            },
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    const PILL: Pill = Pill {
        width: 100,
        height: 28,
        edge: 1,
    };

    fn style() -> PillStyle {
        PillStyle {
            fill: Rgb(0x202020),
            stroke: Rgb(0x0067C0),
            text: Rgb(0x909090),
            strong: Rgb(0xFFFFFF),
        }
    }

    #[test]
    fn pixels_are_fill_inside_transparent_at_the_corners_stroke_on_the_rim_and_text_when_covered() {
        let all = u32::MAX;
        let alpha = 0xFF00_0000;
        let rows = [
            (50, 14, 0, false, all, 0xFF20_2020),
            (0, 0, 0, false, alpha, 0),
            (99, 27, 0, false, alpha, 0),
            (50, 0, 0, false, all, 0xFF00_67C0),
            (50, 27, 0, false, all, 0xFF00_67C0),
            (50, 14, 255, false, all, 0xFF90_9090),
            (50, 14, 255, true, all, 0xFFFF_FFFF),
        ];
        for row in rows {
            let (x, y, coverage, strong, mask, expected) = row;
            assert_eq!(
                style().pixel(PILL, x, y, coverage, strong) & mask,
                expected,
                "{row:?}"
            );
        }
    }

    #[test]
    fn partial_coverage_blends_between_fill_and_text() {
        let pixel = style().pixel(PILL, 50, 14, 128, false);
        let red = (pixel >> 16) & 0xFF;
        assert!((0x20..0x90).contains(&red));
        assert_eq!(pixel >> 24, 0xFF);
    }

    #[test]
    fn pixels_are_premultiplied_at_the_antialiased_edge() {
        let pixel = style().pixel(PILL, 3, 4, 0, false);
        let alpha = pixel >> 24;
        assert!(alpha > 0 && alpha < 0xFF, "{alpha}");
        assert!(((pixel >> 16) & 0xFF) <= alpha);
        assert!((pixel & 0xFF) <= alpha);
    }
}
