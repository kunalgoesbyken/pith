use std::sync::atomic::Ordering;
use std::time::Duration;

use pith_windows::KeyAction;
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::UI::Controls::EM_UNDO;

use crate::harness::actions::{inserted, is_hide, is_request, last_show};
use crate::harness::clipboard::clipboard_text;
use crate::harness::edit::{get_text, select_range, send, send_text, set_text};
use crate::harness::fixtures::PROSE;
use crate::harness::insert::THROUGH_CLIPBOARD;
use crate::harness::rig::Rig;
use crate::harness::text::{tail, word_count};

pub(crate) fn accept_word_then_rest(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("accept flow", false, "no suggestion".into());
    };
    let before = get_text(field.edit);
    rig.key(KeyAction::AcceptWord);
    let seen = rig.settle(Duration::from_millis(500));
    let words = inserted(&seen);
    let word = words.first().cloned().unwrap_or_default();
    let text = get_text(field.edit);
    rig.check(
        "accept word inserts the next word",
        words.len() == 1 && text == format!("{before}{word}") && suggestion.starts_with(&word),
        format!(
            "suggestion {suggestion:?}, inserted {words:?}, tail {:?}",
            tail(&text)
        ),
    );
    let rest = suggestion[word.len().min(suggestion.len())..].to_string();
    if rest.trim().is_empty() {
        rig.check(
            "single-word suggestion is fully consumed by accept word",
            seen.iter().any(is_hide),
            format!("{seen:?}"),
        );
        return;
    }
    rig.check(
        "rest stays visible after accept word",
        last_show(&seen).as_deref() == Some(rest.as_str()) && !seen.iter().any(is_request),
        format!("rest {rest:?}, shown {:?}", last_show(&seen)),
    );
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(500));
    let text = get_text(field.edit);
    rig.check(
        "accept all inserts the rest",
        inserted(&seen).first() == Some(&rest) && text == format!("{before}{suggestion}"),
        format!("rest {rest:?}, tail {:?}", tail(&text)),
    );
}

pub(crate) fn accept_is_one_undo_step(rig: &mut Rig) {
    let field = rig.a;
    for (mode, through_clipboard) in [("selection insert", false), ("clipboard paste", true)] {
        THROUGH_CLIPBOARD.store(through_clipboard, Ordering::SeqCst);
        let Some(_) = rig.suggestion_for(field, &PROSE, 1) else {
            rig.check(
                &format!("undo ({mode}) setup"),
                false,
                "no suggestion".into(),
            );
            continue;
        };
        std::thread::sleep(Duration::from_millis(300));
        let clipboard_before = clipboard_text();
        let sequence_before = unsafe { GetClipboardSequenceNumber() };
        let before = get_text(field.edit);
        rig.key(KeyAction::AcceptWord);
        let seen = rig.settle(Duration::from_millis(1500));
        let words = inserted(&seen);
        let landed = get_text(field.edit);
        rig.check(
            &format!("undo ({mode}): accept inserted the word"),
            words.len() == 1 && landed == format!("{before}{}", words[0]),
            format!("inserted {words:?}, tail {:?}", tail(&landed)),
        );
        let undone = send(field.edit, EM_UNDO, 0, 0) != 0;
        let after_undo = get_text(field.edit);
        rig.check(
            &format!("undo ({mode}): one undo removes exactly the inserted text"),
            undone && after_undo == before,
            format!(
                "undone {undone}, before {:?}, after {:?}",
                tail(&before),
                tail(&after_undo)
            ),
        );
        let clipboard_after = clipboard_text();
        let touched = unsafe { GetClipboardSequenceNumber() } != sequence_before;
        rig.check(
            &format!("undo ({mode}): clipboard text is restored"),
            clipboard_after == clipboard_before,
            format!(
                "before {:?}, after {:?}, sequence changed {touched}",
                clipboard_before.as_deref().map(tail),
                clipboard_after.as_deref().map(tail)
            ),
        );
        rig.check(
            &format!("undo ({mode}): only the clipboard path touches the clipboard"),
            touched == through_clipboard,
            format!("sequence changed {touched}"),
        );
    }
    THROUGH_CLIPBOARD.store(false, Ordering::SeqCst);
}

pub(crate) fn rapid_accept_chain(rig: &mut Rig) {
    let field = rig.a;
    let Some((typed, suggestion)) = rig.suggestion_for(field, &PROSE, 3) else {
        return rig.check(
            "accept chain setup",
            false,
            "no suggestion with 3+ words".into(),
        );
    };
    let base = get_text(field.edit);
    let words = {
        let mut rest = suggestion.as_str();
        let mut count = 0;
        while !rest.trim().is_empty() {
            let word = pith_core::next_word(rest);
            rest = &rest[word.len()..];
            count += 1;
        }
        count
    };
    rig.drain();
    let gap = std::env::var("PITH_E2E_KEY_GAP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for _ in 0..words + 3 {
        rig.key(KeyAction::AcceptWord);
        std::thread::sleep(Duration::from_millis(gap));
    }
    let seen = rig.settle(Duration::from_millis(700));
    let text = get_text(field.edit);
    rig.check(
        "rapid accept-word chain inserts the suggestion once, in order",
        text == format!("{base}{suggestion}") && inserted(&seen).concat() == suggestion,
        format!(
            "typed {typed:?}, suggestion {suggestion:?}, inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&text)
        ),
    );
}

pub(crate) fn forced_trigger(rig: &mut Rig) {
    let field = rig.a;
    rig.fresh(field, "");
    for text in ["Hi", "Hello", "Re"] {
        rig.fresh(field, "");
        rig.type_slowly(field.edit, text, 10);
        let auto = rig.settle(Duration::from_millis(900));
        rig.check(
            "short text asks nothing on its own",
            !auto.iter().any(is_request) && last_show(&auto).is_none(),
            format!("{text:?}"),
        );
        rig.key(KeyAction::Trigger);
        let Some(first) = rig.wait_show(Duration::from_secs(20)) else {
            rig.check(
                "force trigger shows a suggestion",
                false,
                format!("{text:?}"),
            );
            continue;
        };
        let seen = rig.settle(Duration::from_millis(500));
        let suggestion = last_show(&seen).unwrap_or(first);
        rig.check(
            "force trigger shows a suggestion in a field too short for auto",
            !suggestion.trim().is_empty(),
            format!("{text:?} -> {suggestion:?}"),
        );
        rig.key(KeyAction::Trigger);
        let again = rig.settle(Duration::from_millis(500));
        rig.check(
            "force trigger while a suggestion is visible does nothing",
            !again.iter().any(|a| is_request(a) || is_hide(a)),
            format!("{again:?}"),
        );
        if word_count(&suggestion) < 2 {
            continue;
        }
        let before = get_text(field.edit);
        rig.key(KeyAction::AcceptWord);
        let seen = rig.settle(Duration::from_millis(600));
        let words = inserted(&seen);
        let word = words.first().cloned().unwrap_or_default();
        let rest = suggestion[word.len().min(suggestion.len())..].to_string();
        rig.check(
            "word accept on a forced suggestion keeps the rest with no new request",
            words.len() == 1
                && get_text(field.edit) == format!("{before}{word}")
                && last_show(&seen).as_deref() == Some(rest.as_str())
                && !seen.iter().any(is_request),
            format!("rest {rest:?}, shown {:?}, seen {seen:?}", last_show(&seen)),
        );
        rig.key(KeyAction::AcceptAll);
        let seen = rig.settle(Duration::from_millis(600));
        rig.check(
            "accept all finishes the forced suggestion",
            get_text(field.edit) == format!("{before}{suggestion}") && inserted(&seen).len() == 1,
            format!("tail {:?}", tail(&get_text(field.edit))),
        );
        return;
    }
}

pub(crate) fn keys_without_a_suggestion(rig: &mut Rig) {
    let field = rig.a;
    rig.fresh(field, "");
    let base = get_text(field.edit);
    for key in [
        KeyAction::AcceptAll,
        KeyAction::AcceptWord,
        KeyAction::Dismiss,
    ] {
        rig.key(key);
    }
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "accept and dismiss keys do nothing without a suggestion",
        inserted(&seen).is_empty() && get_text(field.edit) == base,
        format!("{seen:?}"),
    );
}

pub(crate) fn stale_keys(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    let base = get_text(field.edit);
    send_text(field.edit, "z");
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after a diverging keystroke never pastes the stale suggestion",
        inserted(&seen).is_empty() && get_text(field.edit) == format!("{base}z"),
        format!(
            "suggestion {suggestion:?}, inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&get_text(field.edit))
        ),
    );

    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    let base = get_text(field.edit);
    let first: String = suggestion.chars().take(1).collect();
    send_text(field.edit, &first);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after typing the first suggested character completes without doubling",
        get_text(field.edit) == format!("{base}{suggestion}"),
        format!(
            "suggestion {suggestion:?}, inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&get_text(field.edit))
        ),
    );

    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    set_text(
        field.edit,
        "Completely different text that the model never saw",
    );
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after the text was replaced never pastes the stale suggestion",
        inserted(&seen).is_empty()
            && get_text(field.edit) == "Completely different text that the model never saw",
        format!("suggestion {suggestion:?}, inserts {:?}", inserted(&seen)),
    );

    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    let base = get_text(field.edit);
    select_range(field.edit, 0, 5);
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after selecting text never pastes over the selection",
        inserted(&seen).is_empty() && get_text(field.edit) == base,
        format!("suggestion {suggestion:?}, inserts {:?}", inserted(&seen)),
    );
}
