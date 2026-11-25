use super::canvas::{Align, Canvas, Text, measure};
use super::metrics::{Role, Scale, rect};
use super::theme::{Palette, Rgb};

pub const KEYCAP_HEIGHT: i32 = 28;
const KEYCAP_PAD: i32 = 10;
const KEYCAP_MIN: i32 = 28;
const KEYCAP_GAP: i32 = 6;

pub fn key_labels(chord: &str) -> impl Iterator<Item = &str> {
    chord.split('+')
}

fn cap_width(scale: Scale, label: &str) -> i32 {
    let style = Text::new(Role::Body, Rgb::BLACK);
    let label_width = measure(scale, label, style, 0).0;
    (label_width + 2 * scale.px(KEYCAP_PAD)).max(scale.px(KEYCAP_MIN))
}

pub fn keycaps_width<'a>(scale: Scale, labels: impl Iterator<Item = &'a str>) -> i32 {
    let widths: Vec<i32> = labels.map(|label| cap_width(scale, label)).collect();
    let gaps = widths.len().saturating_sub(1) as i32 * scale.px(KEYCAP_GAP);
    widths.iter().sum::<i32>() + gaps
}

pub fn keycap(
    canvas: &Canvas,
    (scale, palette): (Scale, &Palette),
    origin: (i32, i32),
    label: &str,
) -> i32 {
    let style = Text::new(Role::Body, palette.text).aligned(Align::Center);
    let width = cap_width(scale, label);
    let bounds = rect(origin.0, origin.1, width, scale.px(KEYCAP_HEIGHT));
    canvas.card(bounds, palette.control, palette.control_stroke);
    canvas.text(bounds, label, style);
    width
}

pub fn keycaps<'a>(
    canvas: &Canvas,
    look: (Scale, &Palette),
    origin: (i32, i32),
    labels: impl Iterator<Item = &'a str>,
) -> i32 {
    let (scale, _) = look;
    let mut x = origin.0;
    for label in labels {
        x += keycap(canvas, look, (x, origin.1), label) + scale.px(KEYCAP_GAP);
    }
    (x - scale.px(KEYCAP_GAP) - origin.0).max(0)
}
