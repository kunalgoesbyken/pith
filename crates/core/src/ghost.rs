use unicode_segmentation::UnicodeSegmentation;

use crate::types::Rect;

pub const CONTRAST_FLOOR: f32 = 3.0;
pub const MAX_ALPHA: u32 = 230;
pub const FADE_FRAME_MS: u32 = 16;
pub const FADE_ALPHAS: [u8; 4] = [112, 176, 232, 255];
pub const MAX_FADE_MS: u32 = 80;

const WHITE: u32 = 0x00FF_FFFF;
const BLACK: u32 = 0x0000_0000;
const NEXT_WORD_BOOST_PERCENT: u32 = 135;

const RTL_RANGES: [(u32, u32); 8] = [
    (0x0590, 0x08FF),
    (0xFB1D, 0xFDFF),
    (0xFE70, 0xFEFF),
    (0x10800, 0x10FFF),
    (0x1E800, 0x1EFFF),
    (0x200F, 0x200F),
    (0x202B, 0x202B),
    (0x202E, 0x202E),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ink {
    pub color: u32,
    pub alpha: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Flow {
    pub caret: Rect,
    pub top: i32,
    pub line_height: i32,
    pub bounds: Rect,
    pub rtl: bool,
    pub max_lines: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub x: i32,
    pub y: i32,
    pub width: i32,
}

pub fn opacity_alpha(percent: u8) -> u32 {
    (u32::from(percent.min(100)) * 255 / 100).clamp(1, MAX_ALPHA)
}

pub fn is_rtl(text: &str) -> bool {
    text.chars()
        .find(|c| c.is_alphabetic() || is_rtl_char(*c))
        .is_some_and(is_rtl_char)
}

fn is_rtl_char(c: char) -> bool {
    let code = c as u32;
    RTL_RANGES
        .iter()
        .any(|(low, high)| (*low..=*high).contains(&code))
}

fn channels(color: u32) -> [f32; 3] {
    [color & 0xFF, (color >> 8) & 0xFF, (color >> 16) & 0xFF].map(|c| c as f32)
}

pub fn luminance(color: u32) -> f32 {
    let [red, green, blue] = channels(color).map(|c| {
        let c = c / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

pub fn contrast(a: u32, b: u32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

pub fn blend(foreground: u32, background: u32, alpha: u32) -> u32 {
    let mixed = channels(foreground)
        .into_iter()
        .zip(channels(background))
        .map(|(f, b)| {
            let alpha = alpha as f32 / 255.0;
            (f * alpha + b * (1.0 - alpha)).round() as u32
        })
        .collect::<Vec<_>>();
    mixed[0] | (mixed[1] << 8) | (mixed[2] << 16)
}

pub fn best_plain(background: u32) -> u32 {
    if contrast(WHITE, background) > contrast(BLACK, background) {
        WHITE
    } else {
        BLACK
    }
}

fn raise_to_floor(foreground: u32, background: u32, start: u32) -> Option<u32> {
    (start..=MAX_ALPHA).find(|alpha| contrast(blend(foreground, background, *alpha), background) >= CONTRAST_FLOOR)
}

pub fn ink(text_color: Option<u32>, background: u32, opacity_percent: u8) -> Ink {
    let start = opacity_alpha(opacity_percent);
    let preferred = text_color.unwrap_or_else(|| best_plain(background));
    if let Some(alpha) = raise_to_floor(preferred, background, start) {
        return Ink {
            color: preferred,
            alpha: alpha as u8,
        };
    }
    let plain = best_plain(background);
    Ink {
        color: plain,
        alpha: raise_to_floor(plain, background, start).unwrap_or(MAX_ALPHA) as u8,
    }
}

pub fn next_word_alpha(alpha: u8) -> u8 {
    (u32::from(alpha) * NEXT_WORD_BOOST_PERCENT / 100)
        .min(MAX_ALPHA)
        .max(u32::from(alpha)) as u8
}

pub fn median_by_luminance(mut colors: Vec<u32>) -> Option<u32> {
    colors.sort_by(|a, b| luminance(*a).total_cmp(&luminance(*b)));
    colors.get(colors.len() / 2).copied()
}

pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    (right > left && bottom > top).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub fn flow_bounds(field: Option<Rect>, caret: Rect, work: Rect) -> Rect {
    let centre = Rect {
        x: caret.x,
        y: caret.y + caret.height / 2,
        width: 1,
        height: 1,
    };
    field
        .filter(|field| intersect(*field, centre).is_some())
        .and_then(|field| intersect(field, work))
        .unwrap_or(work)
}

pub fn lines_that_fit(bounds: Rect, top: i32, line_height: i32) -> usize {
    ((bounds.y + bounds.height - top) / line_height.max(1)).max(1) as usize
}

fn right(rect: Rect) -> i32 {
    rect.x + rect.width
}

struct Cursor {
    start: usize,
    end: usize,
    row: usize,
    room: i32,
}

pub fn strong_span(
    text: &str,
    line: &Line,
    strong_end: usize,
    rtl: bool,
    measure: &dyn Fn(&str) -> i32,
) -> Option<(i32, i32)> {
    if strong_end <= line.start {
        return None;
    }
    if strong_end >= line.end {
        return Some((0, line.width));
    }
    let covered = measure(text.get(line.start..strong_end)?).clamp(0, line.width);
    Some(if rtl {
        (line.width - covered, line.width)
    } else {
        (0, covered)
    })
}

pub fn layout(text: &str, flow: &Flow, measure: &dyn Fn(&str) -> i32) -> Vec<Line> {
    let anchor = if flow.rtl {
        flow.caret.x
    } else {
        flow.caret.x + flow.caret.width.max(1)
    };
    let first_room = if flow.rtl {
        anchor - flow.bounds.x
    } else {
        right(flow.bounds) - anchor
    };
    let mut lines = Vec::new();
    let mut cursor = Cursor {
        start: 0,
        end: 0,
        row: 0,
        room: first_room,
    };
    let flush = |cursor: &Cursor, lines: &mut Vec<Line>| {
        let content = text[cursor.start..cursor.end].trim_end();
        if content.is_empty() || cursor.row >= flow.max_lines {
            return;
        }
        let width = measure(content);
        let x = match (flow.rtl, cursor.row) {
            (false, 0) => anchor,
            (false, _) => flow.bounds.x,
            (true, 0) => anchor - width,
            (true, _) => right(flow.bounds) - width,
        };
        let highest = (right(flow.bounds) - width).max(flow.bounds.x);
        lines.push(Line {
            start: cursor.start,
            end: cursor.start + content.len(),
            x: x.clamp(flow.bounds.x, highest),
            y: flow.top + cursor.row as i32 * flow.line_height,
            width,
        });
    };
    for (index, segment) in text.split_word_bound_indices() {
        let end = index + segment.len();
        if segment.chars().all(char::is_whitespace) {
            if cursor.row > 0 && text[cursor.start..cursor.end].is_empty() {
                cursor.start = end;
            }
            cursor.end = end;
            continue;
        }
        if measure(&text[cursor.start..end]) <= cursor.room {
            cursor.end = end;
            continue;
        }
        let has_content = !text[cursor.start..cursor.end].trim().is_empty();
        if cursor.row == 0 || has_content {
            flush(&cursor, &mut lines);
            cursor.row += 1;
            cursor.start = index;
            cursor.end = index;
            cursor.room = flow.bounds.width;
        }
        if cursor.row >= flow.max_lines {
            return lines;
        }
        if measure(segment) <= cursor.room {
            cursor.end = end;
            continue;
        }
        for (offset, grapheme) in segment.grapheme_indices(true) {
            let from = index + offset;
            let to = from + grapheme.len();
            if from > cursor.start && measure(&text[cursor.start..to]) > cursor.room {
                flush(&cursor, &mut lines);
                cursor.row += 1;
                cursor.start = from;
                cursor.room = flow.bounds.width;
                if cursor.row >= flow.max_lines {
                    return lines;
                }
            }
            cursor.end = to;
        }
    }
    flush(&cursor, &mut lines);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measure(text: &str) -> i32 {
        text.graphemes(true).count() as i32 * 10
    }

    fn flow(caret_x: i32, bounds_x: i32, bounds_width: i32, rtl: bool) -> Flow {
        Flow {
            caret: Rect {
                x: caret_x,
                y: 100,
                width: 1,
                height: 20,
            },
            top: 100,
            line_height: 20,
            bounds: Rect {
                x: bounds_x,
                y: 0,
                width: bounds_width,
                height: 1000,
            },
            rtl,
            max_lines: 5,
        }
    }

    fn rows(text: &str, flow: &Flow) -> Vec<(String, i32, i32)> {
        layout(text, flow, &measure)
            .into_iter()
            .map(|line| (text[line.start..line.end].to_string(), line.x, line.y))
            .collect()
    }

    #[test]
    fn short_text_stays_on_the_caret_line_right_of_the_caret() {
        let flow = flow(50, 0, 400, false);
        assert_eq!(rows(" over there", &flow), [(" over there".into(), 51, 100)]);
    }

    #[test]
    fn long_text_wraps_at_word_boundaries_and_restarts_at_the_left_edge() {
        let flow = flow(0, 0, 100, false);
        assert_eq!(
            rows(" alpha beta gamma delta", &flow),
            [
                (" alpha".into(), 1, 100),
                ("beta gamma".into(), 0, 120),
                ("delta".into(), 0, 140),
            ]
        );
    }

    #[test]
    fn a_word_that_does_not_fit_after_the_caret_moves_down_whole() {
        let flow = flow(70, 0, 100, false);
        assert_eq!(
            rows(" word", &flow),
            [("word".into(), 0, 120)]
        );
    }

    #[test]
    fn a_word_wider_than_the_field_splits_between_graphemes() {
        let flow = flow(0, 0, 40, false);
        let rows = rows("abcdefghij", &flow);
        assert_eq!(
            rows,
            [
                ("abcd".into(), 0, 120),
                ("efgh".into(), 0, 140),
                ("ij".into(), 0, 160),
            ]
        );
    }

    #[test]
    fn combining_marks_and_emoji_never_split_from_their_base() {
        let flow = flow(0, 0, 30, false);
        let text = "e\u{301}e\u{301}e\u{301}e\u{301}";
        for (line, _, _) in rows(text, &flow) {
            assert!(!line.starts_with('\u{301}'), "{line:?}");
        }
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}".repeat(4);
        let rows = rows(&family, &flow);
        let rejoined: String = rows.iter().map(|(line, _, _)| line.as_str()).collect();
        assert_eq!(rejoined, family);
        for (line, _, _) in &rows {
            assert_eq!(line.matches('\u{200D}').count() % 2, 0, "{line:?}");
        }
    }

    #[test]
    fn rows_beyond_the_field_height_are_dropped() {
        let mut flow = flow(0, 0, 100, false);
        flow.max_lines = 1;
        assert_eq!(rows(" alpha beta gamma", &flow), [(" alpha".into(), 1, 100)]);
        assert_eq!(lines_that_fit(flow.bounds, 100, 20), 45);
        assert_eq!(
            lines_that_fit(Rect { x: 0, y: 90, width: 10, height: 20 }, 100, 20),
            1
        );
    }

    #[test]
    fn a_line_never_leaves_the_bounds_even_for_one_wide_grapheme() {
        let flow = flow(0, 0, 5, false);
        for line in layout("abc", &flow, &measure) {
            assert!(line.x >= 0);
        }
    }

    #[test]
    fn rtl_text_extends_left_from_the_caret() {
        let flow = flow(200, 0, 400, true);
        assert_eq!(rows("שלום", &flow), [("שלום".into(), 160, 100)]);
    }

    #[test]
    fn rtl_text_wraps_back_to_the_right_edge() {
        let flow = flow(60, 0, 100, true);
        assert_eq!(
            rows("aaaa bbbb cccc", &flow),
            [
                ("aaaa".into(), 20, 100),
                ("bbbb cccc".into(), 10, 120),
            ]
        );
    }

    #[test]
    fn caret_at_the_far_edge_never_places_text_off_the_bounds() {
        let ltr = flow(400, 0, 400, false);
        for line in layout(" next", &ltr, &measure) {
            assert!(line.x >= 0 && line.x + line.width <= 400);
        }
        let rtl = flow(0, 0, 400, true);
        for line in layout("next", &rtl, &measure) {
            assert!(line.x >= 0 && line.x + line.width <= 400);
        }
    }

    #[test]
    fn strong_span_covers_the_next_word_from_the_logical_start() {
        let text = " alpha beta gamma";
        let flow = flow(0, 0, 100, false);
        let lines = layout(text, &flow, &measure);
        let strong_end = " alpha".len();
        assert_eq!(strong_span(text, &lines[0], strong_end, false, &measure), Some((0, 60)));
        assert_eq!(strong_span(text, &lines[1], strong_end, false, &measure), None);
    }

    #[test]
    fn strong_span_is_the_whole_line_when_the_word_ends_after_it() {
        let text = "aaaa bbbb";
        let flow = flow(60, 0, 100, true);
        let lines = layout(text, &flow, &measure);
        assert_eq!(strong_span(text, &lines[0], 9, true, &measure), Some((0, 40)));
        assert_eq!(strong_span(text, &lines[1], 4, true, &measure), None);
    }

    #[test]
    fn strong_span_sits_at_the_right_end_of_an_rtl_line() {
        let text = "aaaa bbbb";
        let flow = flow(200, 0, 400, true);
        let lines = layout(text, &flow, &measure);
        assert_eq!(strong_span(text, &lines[0], 4, true, &measure), Some((50, 90)));
        assert_eq!(strong_span(text, &lines[0], 4, false, &measure), Some((0, 40)));
    }

    #[test]
    fn strong_span_ignores_a_boundary_inside_a_character() {
        let text = " שלום עולם";
        let flow = flow(300, 0, 400, true);
        let lines = layout(text, &flow, &measure);
        assert_eq!(strong_span(text, &lines[0], 2, true, &measure), None);
    }

    #[test]
    fn empty_and_blank_text_produce_no_lines() {
        let flow = flow(0, 0, 100, false);
        assert!(layout("", &flow, &measure).is_empty());
        assert!(layout("   ", &flow, &measure).is_empty());
    }

    #[test]
    fn direction_follows_the_first_strong_character() {
        for (text, expected) in [
            ("hello", false),
            ("שלום world", true),
            ("hello שלום", false),
            ("  123 مرحبا", true),
            ("... 42", false),
            ("", false),
            ("  ", false),
            ("\u{202E}abc", true),
            ("ܐܒܓ", true),
            ("日本語", false),
        ] {
            assert_eq!(is_rtl(text), expected, "{text:?}");
        }
    }

    #[test]
    fn dark_ghost_on_white_stays_at_the_configured_opacity_when_contrast_allows() {
        let result = ink(Some(BLACK), WHITE, 70);
        assert_eq!(result.color, BLACK);
        assert_eq!(u32::from(result.alpha), opacity_alpha(70));
        assert!(contrast(blend(BLACK, WHITE, u32::from(result.alpha)), WHITE) >= CONTRAST_FLOOR);
    }

    #[test]
    fn faint_opacity_is_raised_until_the_floor_is_met() {
        for (foreground, background) in [(BLACK, WHITE), (WHITE, BLACK), (0x00AA_5500, 0x00F0_F0F0)] {
            let result = ink(Some(foreground), background, 10);
            assert!(u32::from(result.alpha) > opacity_alpha(10));
            let shown = blend(result.color, background, u32::from(result.alpha));
            assert!(contrast(shown, background) >= CONTRAST_FLOOR, "{foreground:x} on {background:x}");
        }
    }

    #[test]
    fn text_colour_that_cannot_reach_the_floor_is_replaced_by_black_or_white() {
        let grey_on_grey = ink(Some(0x0080_8080), 0x0088_8888, 50);
        assert!(grey_on_grey.color == BLACK || grey_on_grey.color == WHITE);
        let shown = blend(grey_on_grey.color, 0x0088_8888, u32::from(grey_on_grey.alpha));
        assert!(contrast(shown, 0x0088_8888) >= CONTRAST_FLOOR);
    }

    #[test]
    fn missing_text_colour_picks_the_better_of_black_and_white() {
        assert_eq!(ink(None, WHITE, 50).color, BLACK);
        assert_eq!(ink(None, 0x0010_1010, 50).color, WHITE);
        assert_eq!(ink(None, 0x00FF_0000, 50).color, WHITE);
        assert_eq!(ink(None, 0x0000_FF00, 50).color, BLACK);
    }

    #[test]
    fn every_background_and_opacity_ends_at_or_above_the_floor_or_the_alpha_cap() {
        for background in [BLACK, WHITE, 0x0080_8080, 0x00FF_00FF, 0x0000_FFFF, 0x0010_2030] {
            for percent in [1u8, 10, 35, 50, 65, 100, 255] {
                let result = ink(None, background, percent);
                assert!(u32::from(result.alpha) <= MAX_ALPHA);
                let shown = blend(result.color, background, u32::from(result.alpha));
                assert!(contrast(shown, background) >= CONTRAST_FLOOR, "{background:x} {percent}");
            }
        }
    }

    #[test]
    fn next_word_is_drawn_stronger_but_never_past_the_cap() {
        for alpha in [1u8, 60, 120, 200, 230] {
            let boosted = next_word_alpha(alpha);
            assert!(boosted >= alpha);
            assert!(u32::from(boosted) <= MAX_ALPHA.max(u32::from(alpha)));
        }
        assert!(next_word_alpha(100) > 100);
        assert_eq!(next_word_alpha(230), 230);
    }

    #[test]
    fn luminance_and_contrast_match_wcag_reference_values() {
        assert!((contrast(BLACK, WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 0.001);
        assert!((contrast(0x0077_7777, WHITE) - 4.48).abs() < 0.05);
    }

    #[test]
    fn median_ignores_a_single_glyph_outlier() {
        assert_eq!(median_by_luminance(vec![WHITE, BLACK, WHITE]), Some(WHITE));
        assert_eq!(median_by_luminance(vec![]), None);
    }

    #[test]
    fn flow_bounds_use_the_field_when_it_holds_the_caret_and_the_work_area_otherwise() {
        let work = Rect { x: 0, y: 0, width: 1920, height: 1040 };
        let caret = Rect { x: 500, y: 300, width: 1, height: 20 };
        let field = Rect { x: 400, y: 280, width: 600, height: 100 };
        assert_eq!(flow_bounds(Some(field), caret, work), field);
        let elsewhere = Rect { x: 0, y: 0, width: 100, height: 100 };
        assert_eq!(flow_bounds(Some(elsewhere), caret, work), work);
        assert_eq!(flow_bounds(None, caret, work), work);
        let overhang = Rect { x: 1800, y: 280, width: 600, height: 100 };
        let near_edge = Rect { x: 1850, y: 300, width: 1, height: 20 };
        assert_eq!(
            flow_bounds(Some(overhang), near_edge, work),
            Rect { x: 1800, y: 280, width: 120, height: 100 }
        );
    }

    #[test]
    fn fade_finishes_inside_the_budget_and_ends_opaque() {
        assert!((FADE_ALPHAS.len() as u32 - 1) * FADE_FRAME_MS <= MAX_FADE_MS);
        assert_eq!(FADE_ALPHAS.last(), Some(&255));
        assert!(FADE_ALPHAS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn opacity_percent_maps_into_the_alpha_range() {
        assert_eq!(opacity_alpha(0), 1);
        assert_eq!(opacity_alpha(50), 127);
        assert_eq!(opacity_alpha(100), MAX_ALPHA);
        assert_eq!(opacity_alpha(200), MAX_ALPHA);
    }
}
