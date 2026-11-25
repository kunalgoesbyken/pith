use std::path::PathBuf;

use pith_core::{Placement, Rect, Rgb, TextStyle, is_rtl};
use pith_windows::{Picture, Sample, preview};

use crate::capture::OUT_DIR;
use crate::common::png;

const OVERLAY_WIDTH: i32 = 560;
const OVERLAY_PAD: i32 = 20;
const CARET_HEIGHT: i32 = 20;
const CARET_WIDTH: i32 = 2;
const NARROW_FIELD: i32 = 120;
const FONT_PX: i32 = 15;
const GHOST_OPACITY: u8 = 50;
const LATIN: &str = "the quick brown fox jumps over the lazy dog";
const LONG: &str =
    "the quick brown fox jumps over the lazy dog and then keeps running past the bridge";
const ARABIC: &str = "مرحبا بك في هذا التطبيق الجديد";
const CJK: &str = "こんにちは、今日はいい天気ですね。";
const COMBINING: &str = "cafe\u{301} nai\u{308}ve resume\u{301} a\u{30a}ngstro\u{308}m";

enum Surface {
    Pill(&'static str, Option<&'static str>),
    Wrapped(&'static str, Option<&'static str>),
    Ghost(&'static str),
}

const OVERLAY_ROWS: [Surface; 9] = [
    Surface::Pill(LATIN, Some("Tab")),
    Surface::Pill(LATIN, None),
    Surface::Wrapped(LONG, Some("Tab")),
    Surface::Wrapped(LONG, None),
    Surface::Pill(ARABIC, Some("Tab")),
    Surface::Pill(CJK, Some("Tab")),
    Surface::Pill(COMBINING, None),
    Surface::Ghost(LATIN),
    Surface::Ghost(ARABIC),
];

fn paint_over(canvas: &mut [u8], width: i32, picture: &Picture) {
    for (index, pixel) in picture.pixels.iter().enumerate() {
        let column = picture.x + index as i32 % picture.width;
        let row = picture.y + index as i32 / picture.width;
        let at = ((row * width + column) * 4) as usize;
        let Some(slot) = canvas.get_mut(at..at + 3) else {
            continue;
        };
        let alpha = pixel >> 24;
        let source = [(pixel >> 16) & 0xFF, (pixel >> 8) & 0xFF, pixel & 0xFF];
        for (channel, value) in slot.iter_mut().zip(source) {
            *channel = (value + u32::from(*channel) * (255 - alpha) / 255).min(255) as u8;
        }
    }
}

fn overlay_row(surface: &Surface, dpi: u32, dark: bool, backdrop: [u8; 3]) -> (i32, Vec<u8>) {
    let unit = |logical: i32| logical * dpi as i32 / 96;
    let (text, placement_of, key_hint) = match surface {
        Surface::Pill(text, hint) => (*text, Placement::BelowCaret as fn(Rect) -> Placement, *hint),
        Surface::Wrapped(text, hint) => {
            (*text, Placement::Anchored as fn(Rect) -> Placement, *hint)
        }
        Surface::Ghost(text) => (*text, Placement::Inline as fn(Rect) -> Placement, None),
    };
    let side = if is_rtl(text) {
        OVERLAY_WIDTH - OVERLAY_PAD
    } else {
        OVERLAY_PAD
    };
    let caret = Rect {
        x: unit(side),
        y: unit(OVERLAY_PAD),
        width: unit(CARET_WIDTH),
        height: unit(CARET_HEIGHT),
    };
    let narrow = Rect {
        width: unit(NARROW_FIELD),
        ..caret
    };
    let ink = if dark { [0xE6; 3] } else { [0x1A; 3] };
    let style = TextStyle {
        family: Some("Segoe UI".into()),
        px: Some(unit(FONT_PX) as f32),
        color: Some(Rgb::from_channels(ink.map(u32::from))),
    };
    let placement = placement_of(if matches!(surface, Surface::Wrapped(..)) {
        narrow
    } else {
        caret
    });
    let picture = preview(&Sample {
        text,
        placement,
        style: &style,
        dpi,
        background: Rgb::from_channels(backdrop.map(u32::from)),
        opacity: GHOST_OPACITY,
        key_hint,
    })
    .unwrap();
    let width = unit(OVERLAY_WIDTH);
    let height = picture.y + picture.height + unit(OVERLAY_PAD);
    let mut pixels = vec![255u8; (width * height * 4) as usize];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[..3].copy_from_slice(&backdrop);
    }
    for row in caret.y..caret.y + caret.height {
        for column in caret.x..caret.x + caret.width {
            let at = ((row * width + column) * 4) as usize;
            pixels[at..at + 3].copy_from_slice(&ink);
        }
    }
    paint_over(&mut pixels, width, &picture);
    (height, pixels)
}

pub(crate) fn overlay(name: &str, dark: bool, dpi: u32, backdrop: [u8; 3]) {
    let rows: Vec<(i32, Vec<u8>)> = OVERLAY_ROWS
        .iter()
        .map(|surface| overlay_row(surface, dpi, dark, backdrop))
        .collect();
    let height: i32 = rows.iter().map(|(height, _)| height).sum();
    let pixels: Vec<u8> = rows.into_iter().flat_map(|(_, pixels)| pixels).collect();
    let width = (OVERLAY_WIDTH * dpi as i32 / 96) as u32;
    let path = PathBuf::from(OUT_DIR).join(format!("{name}.png"));
    std::fs::write(&path, png(width, height as u32, &pixels)).unwrap();
    println!("{}", path.display());
}
