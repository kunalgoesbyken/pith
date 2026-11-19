use std::sync::atomic::Ordering;
use std::time::Duration;

use pith_core::{Action, Correction};
use pith_windows::KeyAction;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::EM_UNDO;

use super::common::{screenshot, window_rect};
use super::{
    EM_SETSEL, Field, Rig, THROUGH_CLIPBOARD, get_text, is_hide, is_request, is_show, requests,
    select_range, send, send_text, tail, text_length,
};

const KEY_GAP_MS: u64 = 25;
const SNAPSHOT_GAP: Duration = Duration::from_millis(70);
const BOUNDARY: &str = " ";

fn offered(seen: &[Action]) -> Option<&Correction> {
    seen.iter().find_map(|action| match action {
        Action::Offer { correction, .. } => Some(correction),
        _ => None,
    })
}

fn replaced(seen: &[Action]) -> Option<&Correction> {
    seen.iter().find_map(|action| match action {
        Action::Replace { correction, .. } => Some(correction),
        _ => None,
    })
}

fn is_offer(action: &Action) -> bool {
    matches!(action, Action::Offer { .. })
}

fn finish_word(rig: &Rig, field: Field, text: &str) {
    rig.fresh(field, "");
    rig.type_slowly(field.edit, text, KEY_GAP_MS);
    std::thread::sleep(SNAPSHOT_GAP);
    send_text(field.edit, BOUNDARY);
}

fn wait_for_offer(rig: &Rig) -> Option<Correction> {
    let (action, _) = rig.wait_for(Duration::from_secs(5), is_offer)?;
    match action {
        Action::Offer { correction, .. } => Some(correction),
        _ => None,
    }
}

pub fn offer_and_apply(rig: &mut Rig) {
    let field = rig.a;
    for (mode, through_clipboard) in [("selection insert", false), ("clipboard paste", true)] {
        THROUGH_CLIPBOARD.store(through_clipboard, Ordering::SeqCst);
        finish_word(rig, field, "I think teh");
        let offer = wait_for_offer(rig);
        rig.check(
            &format!("typo ({mode}): a misspelled word is offered its correction"),
            offer.as_ref().is_some_and(|c| {
                c.word == "teh" && c.replacement == "the" && c.boundary == BOUNDARY
            }),
            format!("{offer:?}"),
        );
        let seen = rig.settle(Duration::from_millis(800));
        rig.check(
            &format!("typo ({mode}): the completion waits while the correction shows"),
            !seen.iter().any(is_request) && !seen.iter().any(is_show),
            format!("{seen:?}"),
        );
        if !through_clipboard {
            screenshot(
                window_rect(HWND(field.host as _)),
                &rig.out_dir.join("e2e-typo.png"),
            );
        }
        let before = get_text(field.edit);
        rig.key(KeyAction::AcceptAll);
        let seen = rig.settle(Duration::from_millis(1500));
        let text = get_text(field.edit);
        rig.check(
            &format!("typo ({mode}): accept replaces exactly the word and keeps the boundary"),
            replaced(&seen).is_some_and(|c| c.word == "teh")
                && text == "I think the "
                && before == "I think teh ",
            format!("before {before:?}, after {text:?}"),
        );
        let snapshot = rig.read(field);
        rig.check(
            &format!("typo ({mode}): the caret ends after the boundary with nothing selected"),
            snapshot
                .as_ref()
                .is_some_and(|s| s.before == "I think the " && !s.has_selection),
            format!("{:?}", snapshot.map(|s| (s.before, s.has_selection))),
        );
        rig.check(
            &format!("typo ({mode}): the correction hides"),
            seen.iter().any(is_hide),
            format!("{seen:?}"),
        );
        rig.check(
            &format!("typo ({mode}): completions resume after the fix"),
            requests(&seen)
                .first()
                .is_some_and(|request| request.prompt.ends_with("I think the")),
            format!("{:?}", requests(&seen)),
        );
        let undone = send(field.edit, EM_UNDO, 0, 0) != 0;
        let after_undo = get_text(field.edit);
        rig.check(
            &format!("typo ({mode}): one undo restores the typo"),
            undone && after_undo == before,
            format!("undone {undone}, after {:?}", tail(&after_undo)),
        );
    }
    THROUGH_CLIPBOARD.store(false, Ordering::SeqCst);
    finish_word(rig, field, "Teh");
    let offer = wait_for_offer(rig);
    rig.check(
        "typo: a capitalised word at the start of a sentence is offered",
        offer
            .as_ref()
            .is_some_and(|c| c.word == "Teh" && c.replacement.eq_ignore_ascii_case("the")),
        format!("{offer:?}"),
    );
}

pub fn dismiss_and_decline(rig: &mut Rig) {
    let field = rig.a;
    finish_word(rig, field, "I think teh");
    let offer = wait_for_offer(rig);
    rig.check(
        "typo dismiss: setup offers the correction",
        offer.is_some(),
        format!("{offer:?}"),
    );
    rig.key(KeyAction::Dismiss);
    let seen = rig.settle(Duration::from_millis(1200));
    rig.check(
        "typo dismiss: escape hides it, replaces nothing and holds the completion",
        seen.iter().any(is_hide)
            && replaced(&seen).is_none()
            && !seen.iter().any(is_request)
            && get_text(field.edit) == "I think teh ",
        format!("{seen:?}"),
    );
    let end = text_length(field.edit);
    select_range(field.edit, end - 1, end);
    send_text(field.edit, "");
    std::thread::sleep(SNAPSHOT_GAP);
    send_text(field.edit, BOUNDARY);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "typo dismiss: a dismissed word is not offered again",
        offered(&seen).is_none(),
        format!("{seen:?}"),
    );
    rig.drain();
    rig.type_slowly(field.edit, "and recieve", KEY_GAP_MS);
    std::thread::sleep(SNAPSHOT_GAP);
    send_text(field.edit, BOUNDARY);
    let offer = wait_for_offer(rig);
    rig.check(
        "typo dismiss: another misspelling is still offered",
        offer
            .as_ref()
            .is_some_and(|c| c.word == "recieve" && c.replacement == "receive"),
        format!("{offer:?}"),
    );
}

pub fn stale_accept(rig: &mut Rig) {
    let field = rig.a;
    finish_word(rig, field, "I think recieve");
    rig.check(
        "typo stale: setup offers the correction",
        wait_for_offer(rig).is_some(),
        String::new(),
    );
    send_text(field.edit, "z");
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "typo stale: accept after the text moved on replaces nothing",
        replaced(&seen).is_none() && get_text(field.edit) == "I think recieve z",
        format!("{:?}, {seen:?}", get_text(field.edit)),
    );
    finish_word(rig, field, "I think recieve");
    rig.check(
        "typo stale: setup offers the correction again",
        wait_for_offer(rig).is_some(),
        String::new(),
    );
    send(field.edit, EM_SETSEL, 0, 6);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "typo stale: accept over a selection replaces nothing",
        replaced(&seen).is_none() && get_text(field.edit) == "I think recieve ",
        format!("{:?}", get_text(field.edit)),
    );
}

const SKIPPED: [(&str, &str); 12] = [
    ("an acronym", "Send it to KRTQ"),
    ("camel case", "call myVarr"),
    ("a word with digits", "order abc123"),
    ("a url", "see https://example.com/teh"),
    ("an email address", "write to teh@example"),
    ("a proper noun mid sentence", "my friend Teh"),
    ("a word under three letters", "I think ab"),
    ("a correct word", "I think the"),
    ("an identifier", "set my_teh"),
    ("a hyphenated compound", "a well-knwon"),
    (
        "another script under this layout",
        "\u{92e}\u{941}\u{91d}\u{947} \u{915}\u{932} \u{938}\u{941}\u{92c}\u{939}",
    ),
    (
        "a correct word in a long sentence",
        "Please let me know if you have any",
    ),
];

pub fn skips(rig: &mut Rig) {
    let field = rig.a;
    for (name, text) in SKIPPED {
        rig.drain();
        finish_word(rig, field, text);
        let seen = rig.settle(Duration::from_millis(900));
        rig.check(
            &format!("typo skip: {name} is never offered"),
            offered(&seen).is_none() && replaced(&seen).is_none(),
            format!("{text:?}: {seen:?}"),
        );
    }
    rig.fresh(field, "");
    rig.type_slowly(field.edit, "I think teh", KEY_GAP_MS);
    std::thread::sleep(SNAPSHOT_GAP);
    send_text(field.edit, "\r");
    let seen = rig.settle(Duration::from_millis(900));
    rig.check(
        "typo skip: a line break is not a word boundary",
        offered(&seen).is_none(),
        format!("{seen:?}"),
    );
    rig.fresh_at(field, "I think teh next", 11);
    std::thread::sleep(SNAPSHOT_GAP);
    send_text(field.edit, BOUNDARY);
    let seen = rig.settle(Duration::from_millis(900));
    rig.check(
        "typo skip: text after the caret is never touched",
        offered(&seen).is_none() && requests(&seen).is_empty(),
        format!("{seen:?}"),
    );
    let locked = rig.locked;
    finish_word(rig, locked, "my teh");
    let seen = rig.settle(Duration::from_millis(900));
    rig.check(
        "typo skip: a password field is never checked",
        offered(&seen).is_none(),
        format!("{seen:?}"),
    );
    rig.use_field(rig.a);
}
