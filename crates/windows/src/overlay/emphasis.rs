use pith_core::{Line, next_word, strong_span};
use windows::Win32::Foundation::POINT;

use super::face::Face;

pub struct Emphasis(Vec<Option<(i32, i32)>>);

impl Emphasis {
    pub fn of(
        text: &str,
        lines: &[Line],
        rtl: bool,
        face: &Face,
        origin: POINT,
        height: i32,
    ) -> Self {
        let strong_end = next_word(text).len();
        let measure = |segment: &str| face.measure(segment);
        let spans: Vec<_> = lines
            .iter()
            .map(|line| strong_span(text, line, strong_end, rtl, &measure))
            .collect();
        row_spans(lines, &spans, origin, face.line_height, height)
    }

    pub fn covers(&self, column: i32, row: i32) -> bool {
        self.0[row as usize].is_some_and(|(from, to)| (from..to).contains(&column))
    }
}

fn row_spans(
    lines: &[Line],
    spans: &[Option<(i32, i32)>],
    origin: POINT,
    line_height: i32,
    height: i32,
) -> Emphasis {
    let mut rows = vec![None; height as usize];
    for (line, span) in lines.iter().zip(spans) {
        let Some((from, to)) = span else {
            continue;
        };
        let start = line.x - origin.x;
        let top = (line.y - origin.y) as usize;
        for row in rows.iter_mut().skip(top).take(line_height as usize) {
            *row = Some((start + from, start + to));
        }
    }
    Emphasis(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: i32, y: i32, width: i32) -> Line {
        Line {
            start: 0,
            end: 0,
            x,
            y,
            width,
        }
    }

    fn emphasized(
        blank_above: usize,
        span: (i32, i32),
        count: usize,
        height: usize,
    ) -> Vec<Option<(i32, i32)>> {
        std::iter::repeat_n(None, blank_above)
            .chain(std::iter::repeat_n(Some(span), count))
            .chain(std::iter::repeat(None))
            .take(height)
            .collect()
    }

    #[test]
    fn emphasis_rows_are_clipped_and_relative_to_the_picture() {
        let rows = [
            (
                vec![line(10, 100, 50), line(0, 120, 80)],
                vec![Some((0, 30)), None],
                0,
                100,
                20,
                40,
                emphasized(0, (10, 40), 20, 40),
            ),
            (
                vec![line(210, 50, 50)],
                vec![Some((20, 50))],
                200,
                50,
                10,
                10,
                emphasized(0, (30, 60), 10, 10),
            ),
            (
                vec![line(0, 5, 10)],
                vec![Some((0, 10))],
                0,
                0,
                20,
                8,
                emphasized(5, (0, 10), 3, 8),
            ),
        ];
        for (lines, spans, left, first, line_height, height, expected) in rows {
            assert_eq!(
                row_spans(
                    &lines,
                    &spans,
                    POINT { x: left, y: first },
                    line_height,
                    height
                )
                .0,
                expected,
                "{spans:?} {left} {first} {line_height} {height}"
            );
        }
    }
}
