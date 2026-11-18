mod common;

use common::{Rng, is_boundary, text};
use pith_core::{Flow, Rect, is_rtl, layout, lines_that_fit, strong_span};
use unicode_segmentation::UnicodeSegmentation;

const CASES: usize = 4000;

fn measure(segment: &str) -> i32 {
    segment
        .chars()
        .map(|c| if (c as u32) < 0x300 { 7 } else { 14 })
        .sum()
}

fn setup(rng: &mut Rng) -> (String, Flow) {
    let input = text(rng, 40);
    let width = 1 + rng.below(400) as i32;
    let bounds = Rect {
        x: rng.below(200) as i32 - 100,
        y: 0,
        width,
        height: 20 + rng.below(300) as i32,
    };
    let rtl = rng.chance(50);
    let caret_x = bounds.x + rng.below(width as usize + 40) as i32 - 20;
    let top = rng.below(bounds.height as usize) as i32;
    let flow = Flow {
        caret: Rect {
            x: caret_x,
            y: top,
            width: 1 + rng.below(3) as i32,
            height: 18,
        },
        top,
        line_height: 18,
        bounds,
        rtl,
        max_lines: lines_that_fit(bounds, top, 18),
    };
    (input, flow)
}

#[test]
fn layout_lines_are_ordered_clean_slices_inside_the_row_budget() {
    let mut rng = Rng::new(31);
    for _ in 0..CASES {
        let (input, flow) = setup(&mut rng);
        let lines = layout(&input, &flow, &measure);
        assert!(lines.len() <= flow.max_lines, "{input:?}");
        let mut previous_end = 0;
        let mut previous_y = i32::MIN;
        for line in &lines {
            assert!(line.start < line.end, "{input:?}");
            assert!(line.start >= previous_end, "{input:?}");
            assert!(line.y > previous_y, "{input:?}");
            assert!(is_boundary(&input, line.start), "{input:?}");
            assert!(is_boundary(&input, line.end), "{input:?}");
            assert_eq!(line.width, measure(&input[line.start..line.end]));
            assert!(line.x >= flow.bounds.x, "{input:?} {line:?}");
            if line.width <= flow.bounds.width {
                assert!(
                    line.x + line.width <= flow.bounds.x + flow.bounds.width,
                    "{input:?} {line:?} {flow:?}"
                );
            }
            previous_end = line.end;
            previous_y = line.y;
        }
    }
}

#[test]
fn layout_with_room_keeps_every_visible_character_in_order() {
    let mut rng = Rng::new(32);
    for _ in 0..CASES {
        let (input, mut flow) = setup(&mut rng);
        flow.max_lines = usize::MAX;
        let lines = layout(&input, &flow, &measure);
        let kept: String = lines
            .iter()
            .flat_map(|line| input[line.start..line.end].chars())
            .filter(|c| !c.is_whitespace())
            .collect();
        let wanted: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(kept, wanted, "{input:?} {flow:?}");
        for line in &lines {
            assert!(
                !input[line.start..line.end]
                    .graphemes(true)
                    .next()
                    .is_some_and(|g| g.is_empty())
            );
        }
    }
}

#[test]
fn strong_span_stays_inside_its_line() {
    let mut rng = Rng::new(33);
    for _ in 0..CASES {
        let (input, flow) = setup(&mut rng);
        let rtl = is_rtl(&input);
        let strong_end = rng.below(input.len() + 2);
        for line in layout(&input, &flow, &measure) {
            if let Some((from, to)) = strong_span(&input, &line, strong_end, rtl, &measure) {
                assert!(0 <= from && from <= to && to <= line.width, "{from} {to}");
            }
        }
    }
}
