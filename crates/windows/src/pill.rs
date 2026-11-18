use crate::ui::{Rgb, Theme};

const CHANNEL_SHIFTS: [u32; 3] = [16, 8, 0];
const FULL: f32 = 255.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pill {
    pub width: i32,
    pub height: i32,
    pub edge: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Style {
    fill: Rgb,
    stroke: Rgb,
    text: Rgb,
    strong: Rgb,
}

impl Style {
    pub fn of(theme: &Theme) -> Self {
        let palette = theme.palette;
        Self {
            fill: palette.card,
            stroke: palette.accent.base,
            text: palette.text_secondary,
            strong: palette.text,
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
        let [red, green, blue] = CHANNEL_SHIFTS.map(|shift| {
            let channel = |color: Rgb| ((color.0 >> shift) & 0xFF) as f32;
            let body = channel(self.stroke) + (channel(self.fill) - channel(self.stroke)) * inner;
            let shown = body + (channel(ink) - body) * ink_weight;
            (shown * shape).round() as u32
        });
        (((shape * FULL).round() as u32) << 24) | (red << 16) | (green << 8) | blue
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

    fn style() -> Style {
        Style {
            fill: Rgb(0x202020),
            stroke: Rgb(0x0067C0),
            text: Rgb(0x909090),
            strong: Rgb(0xFFFFFF),
        }
    }

    #[test]
    fn the_inside_is_opaque_fill() {
        assert_eq!(style().pixel(PILL, 50, 14, 0, false), 0xFF20_2020);
    }

    #[test]
    fn the_corners_are_transparent() {
        assert_eq!(style().pixel(PILL, 0, 0, 0, false) >> 24, 0);
        assert_eq!(style().pixel(PILL, 99, 27, 0, false) >> 24, 0);
    }

    #[test]
    fn the_rim_is_the_stroke_color() {
        assert_eq!(style().pixel(PILL, 50, 0, 0, false), 0xFF00_67C0);
        assert_eq!(style().pixel(PILL, 50, 27, 0, false), 0xFF00_67C0);
    }

    #[test]
    fn covered_pixels_take_the_text_color_and_the_next_word_the_strong_one() {
        assert_eq!(style().pixel(PILL, 50, 14, 255, false), 0xFF90_9090);
        assert_eq!(style().pixel(PILL, 50, 14, 255, true), 0xFFFF_FFFF);
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
