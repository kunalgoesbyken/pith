use super::ink::{best_plain, ink, next_word_alpha};
use super::scene::{KEYCAP_RADIUS, PILL_EDGE, PILL_RADIUS, PILL_WRAPPED_RADIUS, Placed};
use crate::{CHANNEL_MAX, Ink, Rect, Rgb, Scale, argb};

const FULL: f32 = CHANNEL_MAX as f32;
const PIXEL_CENTRE: f32 = 0.5;
const KEYCAP_INK_THRESHOLD: f32 = 0.5;
const KEY_ALPHA: f32 = 0.16;
const AMBIENT_ALPHA: f32 = 0.12;
const KEY_LIFT_SHARE: f32 = 3.0;
const AMBIENT_BLUR_SHARE: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub page: Rgb,
    pub card: Rgb,
    pub stroke: Rgb,
    pub accent: Rgb,
    pub text: Rgb,
    pub text_secondary: Rgb,
}

impl Scheme {
    pub const LIGHT: Self = Self {
        page: Rgb(0xF3F3F3),
        card: Rgb::WHITE,
        stroke: Rgb(0xE5E5E5),
        accent: Rgb(0xCC4C08),
        text: Rgb(0x1A1A1A),
        text_secondary: Rgb(0x5D5D5D),
    };

    pub const DARK: Self = Self {
        page: Rgb(0x202020),
        card: Rgb(0x2B2B2B),
        stroke: Rgb(0x3A3A3A),
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
pub struct Keycap {
    pub rect: Rect,
    pub radius: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pill {
    pub width: i32,
    pub height: i32,
    pub margin: i32,
    pub radius: i32,
    pub edge: i32,
    pub keycap: Option<Keycap>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PillStyle {
    pub fill: Rgb,
    pub stroke: Rgb,
    pub text: Rgb,
    pub strong: Rgb,
    pub keycap_fill: Rgb,
    pub keycap_stroke: Rgb,
    pub keycap_text: Rgb,
}

fn distance(point: (f32, f32), origin: (f32, f32), size: (f32, f32), radius: f32) -> f32 {
    let half = (size.0 / 2.0, size.1 / 2.0);
    let radius = radius.min(half.0).min(half.1);
    let x = (point.0 - origin.0 - half.0).abs() - (half.0 - radius);
    let y = (point.1 - origin.1 - half.1).abs() - (half.1 - radius);
    x.max(0.0).hypot(y.max(0.0)) + x.max(y).min(0.0) - radius
}

fn edge_weight(distance: f32) -> f32 {
    (PIXEL_CENTRE - distance).clamp(0.0, 1.0)
}

fn soft_edge(distance: f32, blur: f32) -> f32 {
    let t = (PIXEL_CENTRE - distance / blur).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(from: [f32; 3], to: [f32; 3], weight: f32) -> [f32; 3] {
    std::array::from_fn(|index| from[index] + (to[index] - from[index]) * weight)
}

impl Pill {
    fn body_distance(&self, point: (f32, f32), lift: f32) -> f32 {
        let origin = (self.margin as f32, self.margin as f32 + lift);
        let size = (self.width as f32, self.height as f32);
        distance(point, origin, size, self.radius as f32)
    }

    fn keycap_weights(&self, point: (f32, f32)) -> (f32, f32) {
        self.keycap.map_or((0.0, 0.0), |keycap| {
            let Rect {
                x,
                y,
                width,
                height,
            } = keycap.rect;
            let origin = (x as f32, y as f32);
            let size = (width as f32, height as f32);
            let outline = distance(point, origin, size, keycap.radius as f32);
            (
                edge_weight(outline),
                edge_weight(outline + self.edge as f32),
            )
        })
    }

    fn shadow(&self, point: (f32, f32)) -> f32 {
        if self.margin == 0 {
            return 0.0;
        }
        let reach = self.margin as f32;
        let lifted = self.body_distance(point, reach / KEY_LIFT_SHARE);
        let key = KEY_ALPHA * soft_edge(lifted, reach);
        let near = self.body_distance(point, 0.0);
        let ambient = AMBIENT_ALPHA * soft_edge(near, reach / AMBIENT_BLUR_SHARE);
        1.0 - (1.0 - key) * (1.0 - ambient)
    }
}

impl PillStyle {
    pub fn of(scheme: Scheme) -> Self {
        Self {
            fill: scheme.card,
            stroke: scheme.stroke,
            text: scheme.text_secondary,
            strong: scheme.text,
            keycap_fill: scheme.page,
            keycap_stroke: scheme.page,
            keycap_text: scheme.text_secondary,
        }
    }

    pub fn pixel(&self, pill: Pill, column: i32, row: i32, coverage: u32, strong: bool) -> u32 {
        let point = (column as f32 + PIXEL_CENTRE, row as f32 + PIXEL_CENTRE);
        let outline = pill.body_distance(point, 0.0);
        let shape = edge_weight(outline);
        let inner = edge_weight(outline + pill.edge as f32);
        let (keycap, keycap_inner) = pill.keycap_weights(point);
        let ink = match (keycap > KEYCAP_INK_THRESHOLD, strong) {
            (true, _) => self.keycap_text,
            (false, true) => self.strong,
            (false, false) => self.text,
        };
        let ink_weight = coverage.min(CHANNEL_MAX) as f32 / FULL;
        let [stroke, fill, keycap_fill, keycap_stroke, ink] = [
            self.stroke,
            self.fill,
            self.keycap_fill,
            self.keycap_stroke,
            ink,
        ]
        .map(Rgb::unit_channels);
        let keycap_body = mix(keycap_stroke, keycap_fill, keycap_inner);
        let body = mix(stroke, mix(fill, keycap_body, keycap), inner);
        let shown = mix(body, ink, ink_weight).map(|channel| channel * shape * FULL);
        let alpha = shape + (1.0 - shape) * pill.shadow(point);
        argb(
            (alpha * FULL).round() as u32,
            shown.map(|channel| channel.round() as u32),
        )
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
        let radius = if placed.lines.len() > 1 {
            PILL_WRAPPED_RADIUS
        } else {
            PILL_RADIUS
        };
        Self::Pill {
            style,
            shape: Pill {
                width: placed.size.width - 2 * placed.margin,
                height: placed.size.height - 2 * placed.margin,
                margin: placed.margin,
                radius: scale.px(radius),
                edge: scale.px(PILL_EDGE).max(1),
                keycap: placed.keycap.map(|rect| Keycap {
                    rect,
                    radius: scale.px(KEYCAP_RADIUS),
                }),
            },
        }
    }

    pub fn pixel(&self, column: i32, row: i32, coverage: u32, strong: bool) -> u32 {
        match self {
            Self::Ink { ink, strong_alpha } => {
                let ceiling = u32::from(if strong { *strong_alpha } else { ink.alpha });
                ink.color.premultiplied(coverage * ceiling / CHANNEL_MAX)
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
        margin: 0,
        radius: 14,
        edge: 1,
        keycap: None,
    };

    fn style() -> PillStyle {
        PillStyle {
            fill: Rgb(0x202020),
            stroke: Rgb(0x0067C0),
            text: Rgb(0x909090),
            strong: Rgb(0xFFFFFF),
            keycap_fill: Rgb(0x303030),
            keycap_stroke: Rgb(0x303030),
            keycap_text: Rgb(0xC0C0C0),
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
