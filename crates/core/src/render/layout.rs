use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::types::Rect;

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

pub fn fits(text: &str, lines: &[Line]) -> bool {
    lines
        .last()
        .is_some_and(|line| line.end >= text.trim_end().len())
}

pub fn strong_span(
    text: &str,
    line: &Line,
    strong: Range<usize>,
    rtl: bool,
    measure: &dyn Fn(&str) -> i32,
) -> Option<(i32, i32)> {
    let (from, to) = (strong.start.max(line.start), strong.end.min(line.end));
    if from >= to {
        return None;
    }
    let reach = |end: usize| -> Option<i32> {
        if end >= line.end {
            return Some(line.width);
        }
        Some(measure(text.get(line.start..end)?).clamp(0, line.width))
    };
    let end = reach(to)?;
    let start = reach(from)?.min(end);
    Some(if rtl {
        (line.width - end, line.width - start)
    } else {
        (start, end)
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

    fn rows<'a>(text: &'a str, flow: &Flow) -> Vec<(&'a str, i32, i32)> {
        layout(text, flow, &measure)
            .into_iter()
            .map(|line| (&text[line.start..line.end], line.x, line.y))
            .collect()
    }

    #[test]
    fn layout_places_and_wraps_lines_by_the_documented_rules() {
        type Placed<'a> = &'a [(&'a str, i32, i32)];
        let table: [(&str, i32, i32, bool, Placed); 7] = [
            (" over there", 50, 400, false, &[(" over there", 51, 100)]),
            (
                " alpha beta gamma delta",
                0,
                100,
                false,
                &[
                    (" alpha", 1, 100),
                    ("beta gamma", 0, 120),
                    ("delta", 0, 140),
                ],
            ),
            (" word", 70, 100, false, &[("word", 0, 120)]),
            (
                "abcdefghij",
                0,
                40,
                false,
                &[("abcd", 0, 120), ("efgh", 0, 140), ("ij", 0, 160)],
            ),
            ("שלום", 200, 400, true, &[("שלום", 160, 100)]),
            (
                "aaaa bbbb cccc",
                60,
                100,
                true,
                &[("aaaa", 20, 100), ("bbbb cccc", 10, 120)],
            ),
            ("   ", 0, 100, false, &[]),
        ];
        for row in table {
            let (text, caret_x, width, rtl, expected) = row;
            assert_eq!(
                rows(text, &flow(caret_x, 0, width, rtl)),
                expected,
                "{row:?}"
            );
        }
        assert!(layout("", &flow(0, 0, 100, false), &measure).is_empty());
    }

    #[test]
    fn rows_beyond_the_field_height_are_dropped() {
        let mut flow = flow(0, 0, 100, false);
        flow.max_lines = 1;
        assert_eq!(rows(" alpha beta gamma", &flow), [(" alpha", 1, 100)]);
        assert_eq!(lines_that_fit(flow.bounds, 100, 20), 45);
        assert_eq!(
            lines_that_fit(
                Rect {
                    x: 0,
                    y: 90,
                    width: 10,
                    height: 20
                },
                100,
                20
            ),
            1
        );
    }

    #[test]
    fn strong_span_marks_the_next_word_within_each_line() {
        let table = [
            (
                " alpha beta gamma",
                0,
                100,
                false,
                0,
                6,
                false,
                Some((0, 60)),
            ),
            (" alpha beta gamma", 0, 100, false, 1, 6, false, None),
            ("aaaa bbbb", 60, 100, true, 0, 9, true, Some((0, 40))),
            ("aaaa bbbb", 60, 100, true, 1, 4, true, None),
            ("aaaa bbbb", 200, 400, true, 0, 4, true, Some((50, 90))),
            ("aaaa bbbb", 200, 400, true, 0, 4, false, Some((0, 40))),
            (" שלום עולם", 300, 400, true, 0, 2, true, None),
        ];
        for row in table {
            let (text, caret_x, width, flow_rtl, line, strong_end, rtl, expected) = row;
            let lines = layout(text, &flow(caret_x, 0, width, flow_rtl), &measure);
            assert_eq!(
                strong_span(text, &lines[line], 0..strong_end, rtl, &measure),
                expected,
                "{row:?}"
            );
        }
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
    fn flow_bounds_use_the_field_when_it_holds_the_caret_and_the_work_area_otherwise() {
        let work = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let caret = Rect {
            x: 500,
            y: 300,
            width: 1,
            height: 20,
        };
        let field = Rect {
            x: 400,
            y: 280,
            width: 600,
            height: 100,
        };
        assert_eq!(flow_bounds(Some(field), caret, work), field);
        let elsewhere = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        assert_eq!(flow_bounds(Some(elsewhere), caret, work), work);
        assert_eq!(flow_bounds(None, caret, work), work);
        let overhang = Rect {
            x: 1800,
            y: 280,
            width: 600,
            height: 100,
        };
        let near_edge = Rect {
            x: 1850,
            y: 300,
            width: 1,
            height: 20,
        };
        assert_eq!(
            flow_bounds(Some(overhang), near_edge, work),
            Rect {
                x: 1800,
                y: 280,
                width: 120,
                height: 100
            }
        );
    }
}
