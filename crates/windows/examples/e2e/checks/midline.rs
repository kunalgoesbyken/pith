use std::time::Duration;

use pith_core::{Action, Config, Placement, SkipReason, Suffix, check};
use pith_windows::KeyAction;
use windows::Win32::Foundation::HWND;

use crate::common::{overlay_visible, window_rect};
use crate::harness::actions::{inserted, is_request, is_show, last_show, requests, visible_text};
use crate::harness::edit::{get_text, select, send_text, set_text};
use crate::harness::fixtures::PROSE;
use crate::harness::rig::Rig;
use crate::harness::screen::{caret_screen, monitor_of, monitor_rect, move_host, pill_body};
use crate::harness::text::{tail, utf16_len, word_count};

const MIN_GLYPH_PX: i32 = 4;

const FIT_SUFFIX: &str = ".";

const FIT_EDGE_PX: i32 = 42;

const CROWDED_EDGE_PX: i32 = 24;

const FIT_MIN_WORDS: usize = 5;

const MIDLINE_SUFFIXES: [&str; 3] = [".", "?", "."];

const CLOSING_SUFFIXES: [&str; 8] = [")", "」", "）。", "”", ". ", "?!", "؟", "\""];

const TEXT_SUFFIXES: [&str; 9] = [
    " world",
    ", and then",
    "x",
    "(",
    "$5",
    "😀",
    " @alice",
    "%",
    " #release",
];

pub(crate) fn text_after_caret(rig: &mut Rig) {
    let field = rig.a;
    rig.fresh_at(field, "We should schedule the review for next week", 10);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "no suggestion when text follows the caret",
        !seen.iter().any(is_show) && !seen.iter().any(is_request),
        format!("{seen:?}"),
    );
    rig.place(field, "We should schedule the review for next week   ", 43);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "caret before trailing spaces still suggests",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
    rig.place(
        field,
        "We should schedule the review for\rnext week is fine",
        33,
    );
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "caret at the end of a line with text on the next line still suggests",
        requests(&seen)
            .first()
            .is_some_and(|r| r.prompt.ends_with("review") && r.forced_prefix == " for"),
        format!(
            "{:?}",
            requests(&seen)
                .iter()
                .map(|r| r.prompt.clone())
                .collect::<Vec<_>>()
        ),
    );
}

fn last_placement(seen: &[Action]) -> Option<Placement> {
    seen.iter().rev().find_map(|action| match action {
        Action::Show { placement, .. } => Some(*placement),
        _ => None,
    })
}

fn midline_reader(rig: &mut Rig) {
    let field = rig.a;
    let prefix = PROSE[0];
    let config = Config::default();
    for suffix in ["", "   "].into_iter().chain(CLOSING_SUFFIXES) {
        rig.fresh_at(field, &format!("{prefix}{suffix}"), utf16_len(prefix));
        let snapshot = rig.read(field);
        let found = snapshot.as_ref().map(|s| s.after.suffix().clone());
        let expected = if suffix.trim().is_empty() {
            Suffix::Blank
        } else {
            Suffix::Closing(suffix.trim().to_string())
        };
        rig.check(
            &format!("reader classifies {suffix:?} after the caret"),
            found.as_ref() == Some(&expected)
                && snapshot.as_ref().is_some_and(|s| check(s, &config).is_ok()),
            format!("{found:?}"),
        );
    }
    for suffix in TEXT_SUFFIXES {
        rig.fresh_at(field, &format!("{prefix}{suffix}"), utf16_len(prefix));
        let verdict = rig.read(field).map(|s| check(&s, &config));
        rig.check(
            &format!("reader skips {suffix:?} after the caret"),
            verdict == Some(Err(SkipReason::TextAfterCaret)),
            format!("{verdict:?}"),
        );
    }
}

fn midline_cases() -> Vec<(&'static str, &'static str)> {
    PROSE
        .into_iter()
        .zip(MIDLINE_SUFFIXES.into_iter().cycle())
        .collect()
}

fn midline_suggestion<'a>(
    rig: &mut Rig,
    cases: &[(&'a str, &'a str)],
    min_words: usize,
) -> Option<(&'a str, &'a str, Vec<Action>)> {
    let field = rig.a;
    for &(prefix, suffix) in cases {
        rig.fresh(field, "");
        rig.place(field, &format!("{prefix}{suffix}"), utf16_len(prefix));
        if let Some((first, _)) = rig.wait_for(Duration::from_secs(5), is_show) {
            let mut seen = vec![first];
            seen.extend(rig.settle(Duration::from_millis(500)));
            if last_show(&seen).is_some_and(|text| word_count(&text) >= min_words) {
                return Some((prefix, suffix, seen));
            }
        }
    }
    None
}

pub(crate) fn midline_fit(rig: &mut Rig) {
    let field = rig.a;
    let monitor = monitor_rect(monitor_of(HWND(field.host as _)));
    let rect = window_rect(HWND(field.host as _));
    let cases: Vec<(&str, &str)> = PROSE
        .into_iter()
        .map(|prefix| (prefix, FIT_SUFFIX))
        .collect();
    let Some((prefix, suffix, seen)) = midline_suggestion(rig, &cases, FIT_MIN_WORDS) else {
        return rig.check("fit setup", false, "no suggestion".into());
    };
    let full = last_show(&seen).unwrap_or_default();
    rig.key(KeyAction::AcceptWord);
    let seen = rig.settle(Duration::from_millis(500));
    let word = inserted(&seen).concat();
    let remainder = visible_text(&seen).unwrap_or_default();
    rig.check(
        "a word accept leaves the rest of the suggestion in the pill",
        !word.is_empty() && full == format!("{word}{remainder}"),
        format!("full {full:?}, word {word:?}, rest {remainder:?}"),
    );
    let (caret_x, _) = caret_screen(field.edit, utf16_len(prefix) + utf16_len(&word));
    move_host(
        field,
        rect.left + monitor.right - FIT_EDGE_PX - caret_x,
        rect.top,
    );
    let drawn = visible_text(&rig.settle(Duration::from_millis(700)));
    let pill = pill_body();
    rig.check(
        "the cut pill stays on screen inside the room next to the caret",
        pill.is_some_and(|p| p.right <= monitor.right && p.left >= caret_x - 12),
        format!(
            "caret {caret_x}, pill {:?}, monitor right {}",
            pill.map(|p| (p.left, p.right)),
            monitor.right
        ),
    );
    let shown = drawn.clone().unwrap_or_default();
    let rest = remainder.strip_prefix(shown.as_str()).unwrap_or_default();
    let whole_words = shown
        .chars()
        .next_back()
        .zip(rest.chars().next())
        .is_none_or(|(last, next)| !(last.is_alphanumeric() && next.is_alphanumeric()));
    rig.check(
        "a remainder that does not fit is drawn cut to whole words",
        !shown.is_empty() && !rest.is_empty() && whole_words,
        format!("rest {remainder:?}, drawn {drawn:?}"),
    );
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    let landed = inserted(&seen).concat();
    println!("[INFO] pill fit {landed:?} of {remainder:?}");
    rig.check(
        "accepting inserts exactly the text that was drawn",
        landed == shown && get_text(field.edit) == format!("{prefix}{word}{landed}{suffix}"),
        format!(
            "drawn {drawn:?}, inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&get_text(field.edit))
        ),
    );
    move_host(field, rect.left, rect.top);
    std::thread::sleep(Duration::from_millis(300));
    rig.drain();
    send_text(field.edit, "z");
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "suggesting goes on after an accept from a cut pill",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
}

pub(crate) fn midline_crowded(rig: &mut Rig) {
    let field = rig.a;
    let monitor = monitor_rect(monitor_of(HWND(field.host as _)));
    let rect = window_rect(HWND(field.host as _));
    let Some((prefix, _, _)) = midline_suggestion(rig, &midline_cases(), 1) else {
        return rig.check("crowded setup", false, "no suggestion".into());
    };
    let (caret_x, _) = caret_screen(field.edit, utf16_len(prefix));
    move_host(
        field,
        rect.left + monitor.right - CROWDED_EDGE_PX - caret_x,
        rect.top,
    );
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "a pill with no room for one word hides the suggestion",
        visible_text(&seen).is_none() && !overlay_visible(),
        format!("{:?}", visible_text(&seen)),
    );
    let base = get_text(field.edit);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "accept inserts nothing when nothing was drawn",
        inserted(&seen).is_empty() && get_text(field.edit) == base,
        format!("{:?}", inserted(&seen)),
    );
    move_host(field, rect.left, rect.top);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "moving the window gives the suggestion another try",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
}

pub(crate) fn midline(rig: &mut Rig) {
    midline_reader(rig);
    let field = rig.a;
    let Some((prefix, suffix, seen)) = midline_suggestion(rig, &midline_cases(), 1) else {
        return rig.check("mid-line setup", false, "no suggestion".into());
    };
    let suggestion = last_show(&seen).unwrap_or_default();
    rig.check(
        "a suggestion before closing punctuation shows in the pill and does not repeat it",
        matches!(last_placement(&seen), Some(Placement::BelowCaret(_)))
            && !suggestion.trim_start().starts_with(|c| suffix.contains(c))
            && !suggestion.ends_with(|c| suffix.contains(c)),
        format!(
            "{suggestion:?} before {suffix:?}, {:?}",
            last_placement(&seen)
        ),
    );
    std::thread::sleep(Duration::from_millis(200));
    let (caret_x, caret_y) = caret_screen(field.edit, utf16_len(prefix));
    let pill = pill_body();
    rig.check(
        "the pill starts at the caret, stays off the text after it and is wide enough for the suggestion",
        pill.is_some_and(|p| {
            !((p.left..p.right).contains(&(caret_x + 2))
                && (p.top..p.bottom).contains(&(caret_y + 4)))
                && (p.left - caret_x).abs() <= 12
                && p.right - p.left >= suggestion.chars().count() as i32 * MIN_GLYPH_PX
        }),
        format!(
            "caret ({caret_x},{caret_y}), pill {:?}",
            pill.map(|p| (p.left, p.top, p.right, p.bottom))
        ),
    );
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    let landed = inserted(&seen).concat();
    rig.check(
        "accepting inserts at the caret and keeps the text after it",
        !landed.is_empty() && get_text(field.edit) == format!("{prefix}{landed}{suffix}"),
        format!(
            "inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&get_text(field.edit))
        ),
    );
    let Some((prefix, suffix, _)) = midline_suggestion(rig, &midline_cases(), 1) else {
        return rig.check("mid-line stale setup", false, "no suggestion".into());
    };
    let changed = format!("{prefix}!");
    set_text(field.edit, &changed);
    select(field.edit, utf16_len(prefix));
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab after the text beyond the caret changed never pastes the stale suggestion",
        inserted(&seen).is_empty() && get_text(field.edit) == changed,
        format!("was {suffix:?}, inserts {:?}", inserted(&seen)),
    );
}
