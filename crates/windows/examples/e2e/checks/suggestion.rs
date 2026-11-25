use std::time::{Duration, Instant};

use pith_windows::KeyAction;
use windows::Win32::Foundation::HWND;

use crate::common;
use crate::common::{overlay_visible, overlay_window, window_rect};
use crate::harness::actions::{inserted, is_hide, is_request, is_show, last_show, requests};
use crate::harness::edit::{get_text, select, select_end, select_range, text_length};
use crate::harness::fixtures::PROSE;
use crate::harness::rig::Rig;
use crate::harness::screen::{caret_screen, move_host};

pub(crate) fn basic_suggestions(rig: &mut Rig) {
    let field = rig.a;
    for (index, sentence) in PROSE.iter().take(3).enumerate() {
        let shown = rig.type_and_wait(field, sentence, 25);
        rig.check(
            &format!("basic {index} shows a suggestion"),
            shown.is_some(),
            format!("{shown:?}"),
        );
        if index == 0 && shown.is_some() {
            std::thread::sleep(Duration::from_millis(150));
            common::screenshot(
                window_rect(HWND(field.host as _)),
                &rig.out_dir.join("e2e-ghost.png"),
            );
        }
    }
    let started = Instant::now();
    rig.fresh(field, "");
    rig.type_slowly(field.edit, PROSE[1], 25);
    let typed = started.elapsed();
    let shown = rig.wait_for(Duration::from_secs(4), is_show);
    if let Some((_, latency)) = shown {
        rig.latencies.push(latency.as_millis());
    }
    println!(
        "[INFO] typed in {typed:?}, ghost after {:?}",
        shown.map(|s| s.1)
    );
}

pub(crate) fn typing_through_and_escape(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check(
            "typing-through flow",
            false,
            "no multi-char suggestion".into(),
        );
    };
    let typed: String = suggestion.chars().take(2).collect();
    rig.type_slowly(field.edit, &typed, 30);
    let seen = rig.settle(Duration::from_millis(500));
    let expected = suggestion.strip_prefix(typed.as_str()).map(str::to_string);
    rig.check(
        "typing through shrinks the suggestion without a new request",
        !seen.iter().any(is_request) && last_show(&seen) == expected,
        format!(
            "suggestion {suggestion:?}, typed {typed:?}, last {:?}",
            last_show(&seen)
        ),
    );
    rig.key(KeyAction::Dismiss);
    let seen = rig.settle(Duration::from_millis(900));
    rig.check(
        "escape dismisses and stays dismissed",
        seen.iter().any(is_hide) && !seen.iter().any(is_show) && !overlay_visible(),
        format!("{seen:?}"),
    );
}

pub(crate) fn multi_line(rig: &mut Rig) {
    let field = rig.a;
    rig.fresh(field, "");
    rig.type_slowly(
        field.edit,
        "Dear team,\r\rThe quarterly numbers are in and the headline is that we",
        10,
    );
    let seen = rig.settle(Duration::from_millis(1200));
    let asked = requests(&seen)
        .last()
        .map(|r| r.prompt.clone())
        .unwrap_or_default();
    rig.check(
        "multi-line prompt carries earlier lines",
        asked.contains("Dear team,\n\nThe quarterly numbers"),
        format!("{asked:?}"),
    );
    println!("[INFO] multi-line suggestion: {:?}", last_show(&seen));
    rig.fresh(field, "First line is long enough.\rOk");
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "short fresh line does not request",
        !seen.iter().any(is_request),
        format!("{seen:?}"),
    );
    rig.type_slowly(field.edit, " then", 20);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "line grows past the minimum and requests",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
}

pub(crate) fn escape_then_flicker(rig: &mut Rig) {
    let field = rig.a;
    let Some((typed, _)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("escape setup", false, "no suggestion".into());
    };
    rig.key(KeyAction::Dismiss);
    rig.settle(Duration::from_millis(300));
    select(field.edit, 0);
    std::thread::sleep(Duration::from_millis(250));
    select_end(field.edit);
    std::thread::sleep(Duration::from_millis(250));
    select(field.edit, 3);
    std::thread::sleep(Duration::from_millis(150));
    select_end(field.edit);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "escape holds through caret flicker",
        !seen.iter().any(is_show) && !seen.iter().any(is_request) && !overlay_visible(),
        format!("typed {typed:?}, actions {seen:?}"),
    );
    rig.type_slowly(field.edit, " and", 30);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "new typing after escape and flicker suggests again",
        seen.iter().any(is_request),
        format!(
            "{} requests, shown {:?}",
            requests(&seen).len(),
            last_show(&seen)
        ),
    );
}

pub(crate) fn selection_while_visible(rig: &mut Rig) {
    let field = rig.a;
    let Some(_) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("selection setup", false, "no suggestion".into());
    };
    let length = text_length(field.edit);
    select_range(field.edit, length - 5, length);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "selecting text hides the ghost and requests nothing",
        seen.iter().any(is_hide)
            && !seen.iter().any(is_show)
            && !seen.iter().any(is_request)
            && !overlay_visible(),
        format!("{seen:?}"),
    );
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "tab with a selection inserts nothing",
        inserted(&seen).is_empty(),
        format!("{:?}", inserted(&seen)),
    );
    select_end(field.edit);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "collapsing the selection suggests again",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
}

pub(crate) fn caret_moves(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, _)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("caret setup", false, "no suggestion".into());
    };
    let length = text_length(field.edit);
    select(field.edit, length - 4);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "caret moved into the text hides the ghost",
        seen.iter().any(is_hide) && !overlay_visible() && !seen.iter().any(is_show),
        format!("{seen:?}"),
    );
    select_end(field.edit);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "caret back at the end suggests again",
        seen.iter().any(is_request) && last_show(&seen).is_some(),
        format!(
            "{} requests, shown {:?}",
            requests(&seen).len(),
            last_show(&seen)
        ),
    );
    let rect = window_rect(HWND(field.host as _));
    move_host(field, rect.left + 40, rect.top + 20);
    let seen = rig.settle(Duration::from_millis(900));
    let moved_caret = caret_screen(field.edit, text_length(field.edit));
    let ghost = overlay_window().map(window_rect);
    move_host(field, rect.left, rect.top);
    let settled = rig.settle(Duration::from_millis(600));
    rig.check(
        "ghost follows the field when its window moves",
        ghost.is_some_and(|g| (g.left - moved_caret.0).abs() <= 8) && !seen.iter().any(is_request),
        format!(
            "caret {moved_caret:?}, ghost {:?}, back {:?}",
            ghost.map(|g| (g.left, g.top)),
            last_show(&settled)
        ),
    );
}

pub(crate) fn switching_fields(rig: &mut Rig) {
    let (a, b) = (rig.a, rig.b);
    let text = PROSE[2];
    rig.fresh(b, text);
    rig.fresh(a, "");
    rig.type_slowly(a.edit, text, 10);
    let Some(suggestion) = rig.wait_show(Duration::from_secs(5)) else {
        return rig.check("switch setup", false, "no suggestion".into());
    };
    rig.drain();
    rig.use_field(b);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(900));
    rig.check(
        "switching to a field with identical text never pastes the old suggestion",
        inserted(&seen).is_empty() && get_text(b.edit) == text && get_text(a.edit) == text,
        format!("suggestion {suggestion:?}, actions {seen:?}"),
    );
    rig.check(
        "switching to a field with identical text asks the model for that field",
        seen.iter().any(is_hide) || seen.iter().any(is_request),
        format!("{seen:?}"),
    );
    rig.drain();
    rig.use_field(a);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "switching back suggests again",
        seen.iter().any(is_request) || seen.iter().any(is_show),
        format!("{} actions", seen.len()),
    );
}
