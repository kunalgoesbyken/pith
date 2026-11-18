use crate::types::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Inline(Rect),
    Anchored(Rect),
}

fn left(rect: Rect) -> i64 {
    i64::from(rect.x)
}

fn top(rect: Rect) -> i64 {
    i64::from(rect.y)
}

fn right(rect: Rect) -> i64 {
    left(rect) + i64::from(rect.width)
}

fn bottom(rect: Rect) -> i64 {
    top(rect) + i64::from(rect.height)
}

fn contains(rect: Rect, x: i64, y: i64) -> bool {
    (left(rect)..=right(rect)).contains(&x) && (top(rect)..=bottom(rect)).contains(&y)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    left(a).max(left(b)) < right(a).min(right(b)) && top(a).max(top(b)) < bottom(a).min(bottom(b))
}

fn visible_element(element: Option<Rect>, works: &[Rect]) -> Option<Rect> {
    element.filter(|e| e.width > 0 && e.height > 0 && works.iter().any(|w| overlaps(*e, *w)))
}

fn trusted(caret: Rect, element: Option<Rect>, works: &[Rect]) -> bool {
    let (x, y) = (left(caret), top(caret) + i64::from(caret.height) / 2);
    caret.height > 0
        && caret.width >= 0
        && works
            .iter()
            .any(|w| caret.height <= w.height && contains(*w, x, y))
        && element.is_none_or(|e| contains(e, x, y))
}

pub fn place(caret: Option<Rect>, element: Option<Rect>, works: &[Rect]) -> Option<Placement> {
    if works.is_empty() {
        return caret
            .filter(|c| c.height > 0 && c.width >= 0)
            .map(Placement::Inline);
    }
    let element = visible_element(element, works);
    caret
        .filter(|c| trusted(*c, element, works))
        .map(Placement::Inline)
        .or(element.map(Placement::Anchored))
}

pub fn anchor_pill(
    element: Rect,
    width: i32,
    height: i32,
    gap: i32,
    work: Rect,
    rtl: bool,
) -> Option<Rect> {
    if width > work.width || height > work.height {
        return None;
    }
    let wanted = if rtl {
        right(element) - i64::from(width)
    } else {
        left(element)
    };
    let x = wanted.clamp(left(work), right(work) - i64::from(width));
    let fits = |y: &i64| *y >= top(work) && *y + i64::from(height) <= bottom(work);
    let below = bottom(element) + i64::from(gap);
    let above = top(element) - i64::from(gap) - i64::from(height);
    let inside = bottom(element).min(bottom(work)) - i64::from(gap) - i64::from(height);
    let y = [below, above]
        .into_iter()
        .find(fits)
        .or_else(|| (fits(&inside) && inside >= top(element)).then_some(inside))?;
    Some(Rect {
        x: x as i32,
        y: y as i32,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    const PRIMARY: Rect = rect(0, 0, 1920, 1040);
    const LEFT: Rect = rect(-1280, -200, 1280, 984);
    const ABOVE: Rect = rect(0, -1080, 1920, 1040);
    const FIELD: Rect = rect(400, 500, 600, 40);
    const CARET: Rect = rect(500, 508, 1, 24);

    fn on(works: &[Rect], caret: Option<Rect>, element: Option<Rect>) -> Option<Placement> {
        place(caret, element, works)
    }

    #[test]
    fn a_caret_inside_its_field_and_monitor_stays_inline() {
        assert_eq!(
            on(&[PRIMARY], Some(CARET), Some(FIELD)),
            Some(Placement::Inline(CARET))
        );
    }

    #[test]
    fn a_caret_without_known_field_bounds_stays_inline() {
        assert_eq!(
            on(&[PRIMARY], Some(CARET), None),
            Some(Placement::Inline(CARET))
        );
    }

    #[test]
    fn a_zero_width_caret_stays_inline() {
        let caret = rect(500, 508, 0, 24);
        assert_eq!(
            on(&[PRIMARY], Some(caret), Some(FIELD)),
            Some(Placement::Inline(caret))
        );
    }

    #[test]
    fn a_caret_on_the_field_edge_stays_inline() {
        for x in [FIELD.x, FIELD.x + FIELD.width] {
            let caret = rect(x, 508, 1, 24);
            assert_eq!(
                on(&[PRIMARY], Some(caret), Some(FIELD)),
                Some(Placement::Inline(caret))
            );
        }
    }

    #[test]
    fn a_missing_caret_anchors_to_the_field() {
        assert_eq!(
            on(&[PRIMARY], None, Some(FIELD)),
            Some(Placement::Anchored(FIELD))
        );
    }

    #[test]
    fn nothing_known_places_nothing() {
        assert_eq!(on(&[PRIMARY], None, None), None);
    }

    #[test]
    fn untrusted_carets_fall_back_to_the_field() {
        let cases = [
            ("zero height", rect(500, 508, 1, 0)),
            ("negative height", rect(500, 532, 1, -24)),
            ("negative width", rect(500, 508, -1, 24)),
            ("origin placeholder", rect(0, 0, 0, 0)),
            ("left of the field", rect(100, 508, 1, 24)),
            ("right of the field", rect(1200, 508, 1, 24)),
            ("above the field", rect(500, 100, 1, 24)),
            ("below the field", rect(500, 900, 1, 24)),
            ("off every monitor", rect(5000, 508, 1, 24)),
            ("far negative", rect(-90_000, -90_000, 1, 24)),
            ("taller than the monitor", rect(500, 0, 1, 4000)),
            ("saturated", rect(i32::MAX, i32::MAX, i32::MAX, i32::MAX)),
        ];
        for (name, caret) in cases {
            assert_eq!(
                on(&[PRIMARY], Some(caret), Some(FIELD)),
                Some(Placement::Anchored(FIELD)),
                "{name}"
            );
        }
    }

    #[test]
    fn a_caret_off_the_monitor_without_a_field_places_nothing() {
        assert_eq!(on(&[PRIMARY], Some(rect(5000, 508, 1, 24)), None), None);
    }

    #[test]
    fn unusable_field_bounds_are_ignored() {
        let cases = [
            ("empty", rect(0, 0, 0, 0)),
            ("zero width", rect(400, 500, 0, 40)),
            ("negative height", rect(400, 500, 600, -40)),
            ("off every monitor", rect(9000, 9000, 600, 40)),
            ("saturated", rect(i32::MAX, i32::MAX, i32::MAX, i32::MAX)),
        ];
        for (name, element) in cases {
            assert_eq!(
                on(&[PRIMARY], Some(CARET), Some(element)),
                Some(Placement::Inline(CARET)),
                "{name}"
            );
            assert_eq!(on(&[PRIMARY], None, Some(element)), None, "{name}");
        }
    }

    #[test]
    fn a_stale_caret_from_another_window_does_not_beat_the_field() {
        let stale = rect(1500, 700, 1, 24);
        assert_eq!(
            on(&[PRIMARY], Some(stale), Some(FIELD)),
            Some(Placement::Anchored(FIELD))
        );
    }

    #[test]
    fn monitors_at_negative_coordinates_are_valid_homes() {
        let works = [PRIMARY, LEFT, ABOVE];
        let field = rect(-1000, 100, 600, 40);
        let caret = rect(-900, 108, 1, 24);
        assert_eq!(
            on(&works, Some(caret), Some(field)),
            Some(Placement::Inline(caret))
        );
        let upper = rect(500, -600, 600, 40);
        let upper_caret = rect(600, -592, 1, 24);
        assert_eq!(
            on(&works, Some(upper_caret), Some(upper)),
            Some(Placement::Inline(upper_caret))
        );
        let gap = rect(-1500, 900, 1, 24);
        assert_eq!(on(&works, Some(gap), None), None);
    }

    #[test]
    fn a_negative_origin_caret_is_off_a_primary_only_desktop() {
        let caret = rect(-900, 108, 1, 24);
        assert_eq!(on(&[PRIMARY], Some(caret), None), None);
    }

    #[test]
    fn a_field_straddling_monitors_is_visible() {
        let field = rect(-100, 500, 400, 40);
        assert_eq!(
            on(&[PRIMARY, LEFT], None, Some(field)),
            Some(Placement::Anchored(field))
        );
    }

    #[test]
    fn scaled_coordinates_are_judged_in_the_same_pixels_as_the_field() {
        let field = rect(600, 750, 900, 60);
        let caret = rect(750, 762, 2, 36);
        assert_eq!(
            on(&[PRIMARY], Some(caret), Some(field)),
            Some(Placement::Inline(caret))
        );
    }

    #[test]
    fn a_caret_reported_in_unscaled_units_falls_back_to_the_field() {
        let field = rect(600, 750, 900, 60);
        let virtualised = rect(375, 381, 1, 18);
        assert_eq!(
            on(&[PRIMARY], Some(virtualised), Some(field)),
            Some(Placement::Anchored(field))
        );
    }

    #[test]
    fn without_monitor_information_a_real_caret_is_trusted() {
        assert_eq!(
            on(&[], Some(CARET), Some(FIELD)),
            Some(Placement::Inline(CARET))
        );
        assert_eq!(on(&[], Some(rect(5, 5, 1, 0)), Some(FIELD)), None);
        assert_eq!(on(&[], None, Some(FIELD)), None);
    }

    fn pill(element: Rect, width: i32, height: i32, work: Rect, rtl: bool) -> Option<Rect> {
        anchor_pill(element, width, height, 4, work, rtl)
    }

    #[test]
    fn the_pill_sits_below_the_field_at_its_start_edge() {
        assert_eq!(
            pill(FIELD, 200, 28, PRIMARY, false),
            Some(rect(400, 544, 200, 28))
        );
    }

    #[test]
    fn a_right_to_left_pill_aligns_to_the_field_end_edge() {
        assert_eq!(
            pill(FIELD, 200, 28, PRIMARY, true),
            Some(rect(800, 544, 200, 28))
        );
    }

    #[test]
    fn the_pill_moves_above_when_there_is_no_room_below() {
        let field = rect(400, 1000, 600, 30);
        assert_eq!(
            pill(field, 200, 28, PRIMARY, false),
            Some(rect(400, 968, 200, 28))
        );
    }

    #[test]
    fn the_pill_sits_inside_the_bottom_edge_when_the_field_leaves_no_room_outside_it() {
        let tall = rect(0, 0, 1920, 1040);
        assert_eq!(
            pill(tall, 200, 28, PRIMARY, false),
            Some(rect(0, 1008, 200, 28))
        );
        let below_screen = rect(0, 20, 1920, 3000);
        assert_eq!(
            pill(below_screen, 200, 28, PRIMARY, false),
            Some(rect(0, 1008, 200, 28))
        );
        let shallow = rect(0, 1030, 1920, 10);
        assert_eq!(pill(shallow, 200, 28, rect(0, 1020, 1920, 30), false), None);
    }

    #[test]
    fn the_pill_never_overlaps_the_field_or_leaves_the_work_area() {
        let fields = [
            FIELD,
            rect(-50, 500, 300, 40),
            rect(1800, 10, 400, 30),
            rect(1700, 1000, 600, 30),
            rect(100, -500, 400, 400),
            rect(0, 0, 1920, 400),
        ];
        for field in fields {
            for rtl in [false, true] {
                for width in [40, 400, 1920] {
                    let Some(found) = pill(field, width, 28, PRIMARY, rtl) else {
                        continue;
                    };
                    assert!(
                        left(found) >= left(PRIMARY)
                            && right(found) <= right(PRIMARY)
                            && top(found) >= top(PRIMARY)
                            && bottom(found) <= bottom(PRIMARY),
                        "{field:?} {found:?}"
                    );
                    assert!(!overlaps(found, field), "{field:?} {found:?}");
                }
            }
        }
    }

    #[test]
    fn the_pill_is_clamped_into_a_negative_coordinate_work_area() {
        let field = rect(-1300, 100, 600, 40);
        assert_eq!(
            pill(field, 200, 28, LEFT, false),
            Some(rect(-1280, 144, 200, 28))
        );
    }

    #[test]
    fn a_pill_wider_or_taller_than_the_work_area_is_dropped() {
        assert_eq!(pill(FIELD, 2000, 28, PRIMARY, false), None);
        assert_eq!(pill(FIELD, 200, 2000, PRIMARY, false), None);
    }

    #[test]
    fn saturated_field_coordinates_do_not_overflow() {
        let field = rect(i32::MAX, i32::MAX, i32::MAX, i32::MAX);
        assert_eq!(pill(field, 200, 28, PRIMARY, false), None);
    }
}
