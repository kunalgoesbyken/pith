use std::time::{Duration, Instant};

use super::*;

fn accepted_insert(config: Config, before: &str, text: &str, event: Event) -> String {
    let mut engine = Engine::new(config);
    suggest(&mut engine, before, text);
    let out = engine.handle(event);
    out.into_iter()
        .find_map(|action| match action {
            Action::Insert(text) => Some(text),
            _ => None,
        })
        .unwrap_or_default()
}

fn spaced() -> Config {
    Config {
        smart_space: true,
        ..Config::default()
    }
}

#[test]
fn trailing_space_only_closes_a_whole_final_word() {
    let cases = [
        (
            "Please sign the",
            " contract",
            Event::AcceptAll,
            " contract ",
        ),
        ("Please sign the contr", "act", Event::AcceptAll, "act "),
        (
            "Please sign the",
            " contract by Friday",
            Event::AcceptWord,
            " contract",
        ),
        (
            "Please sign the",
            " contract by Friday.",
            Event::AcceptAll,
            " contract by Friday.",
        ),
        (
            "Please sign the",
            " contract,",
            Event::AcceptAll,
            " contract,",
        ),
        ("我喜欢", "苹果", Event::AcceptAll, "苹果"),
    ];
    for (before, text, event, expected) in cases {
        let inserted = accepted_insert(spaced(), before, text, event.clone());
        assert_eq!(inserted, expected, "{before:?} + {text:?}");
    }
}

#[test]
fn trailing_space_is_off_by_default_never_doubles_and_does_not_reset_the_engine() {
    let off = accepted_insert(
        Config::default(),
        "Please sign the",
        " contract",
        Event::AcceptAll,
    );
    assert_eq!(off, " contract");

    let mut engine = Engine::new(spaced());
    let snapshot = FieldSnapshot {
        after: " ".into(),
        ..field("Please sign the")
    };
    suggest_field(&mut engine, snapshot, " contract");
    let out = engine.handle(Event::AcceptAll);
    assert!(out.contains(&Action::Insert(" contract".into())), "{out:?}");

    let mut engine = Engine::new(spaced());
    suggest(&mut engine, "Please sign the", " contract");
    engine.handle(Event::AcceptAll);
    let out = engine.handle(Event::Field(Some(field("Please sign the contract "))));
    assert!(
        !out.iter().any(|a| matches!(a, Action::CancelCompletion)),
        "{out:?}"
    );
}

#[test]
fn word_accepts_keep_the_rest_without_any_model_work() {
    let mut engine = Engine::new(Config::default());
    suggest(
        &mut engine,
        "Please sign the",
        " contract by Friday afternoon.",
    );
    let mut typed = String::from("Please sign the");
    for (word, rest) in [
        (" contract", " by Friday afternoon."),
        (" by", " Friday afternoon."),
        (" Friday", " afternoon."),
    ] {
        let out = engine.handle(Event::AcceptWord);
        assert_eq!(out[0], Action::Insert(word.into()));
        assert!(
            matches!(&out[1], Action::Show { cuts, .. } if cuts[0] == rest),
            "{out:?}"
        );
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(no_model_work(&out), "{out:?}");
        typed.push_str(word);
        let out = engine.handle(Event::Field(Some(field(&typed))));
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(engine.suggestion(), Some(rest));
    }
    let out = engine.handle(Event::DebounceElapsed(0));
    assert!(out.is_empty(), "{out:?}");
}

fn racing_engine() -> Engine {
    Engine::new(Config {
        accept_race_ms: 1_000,
        ..Config::default()
    })
}

#[test]
fn an_accept_right_after_the_suggestion_appears_is_refused_until_the_race_window_passes() {
    let later = Instant::now() + Duration::from_secs(5);
    assert!(!Engine::new(Config::default()).can_accept(later, false));

    let mut open = Engine::new(Config {
        accept_race_ms: 0,
        ..Config::default()
    });
    suggest(&mut open, "Please sign the", " contract by Friday.");
    assert!(open.can_accept(Instant::now(), false));

    let mut engine = racing_engine();
    suggest(&mut engine, "Please sign the", " contract by Friday.");
    assert!(!engine.can_accept(Instant::now(), false));
    assert!(!engine.can_accept(Instant::now(), true));
    assert!(engine.can_accept(later, false));
    engine.handle(Event::Field(Some(field("Please sign the con"))));
    assert!(!engine.can_accept(Instant::now(), false));
    assert!(engine.can_accept(later, false));

    let mut engine = racing_engine();
    suggest(&mut engine, "Please sign the", " contract by Friday.");
    engine.handle(Event::AcceptWord);
    assert!(engine.can_accept(Instant::now(), false));
}

#[test]
fn caret_keys_only_accept_at_the_end_of_the_text() {
    for (after, at_end) in [
        ("", true),
        ("\r\n  ", true),
        ("\nSecond line", false),
        ("\n\n  x", false),
    ] {
        let mut engine = Engine::new(Config {
            accept_race_ms: 0,
            ..Config::default()
        });
        let mut inner = field("Please sign the");
        inner.after = after.into();
        let out = engine.handle(Event::Field(Some(inner)));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        engine.handle(Event::DebounceElapsed(generation));
        engine.handle(Event::Completed {
            generation,
            result: Ok(completion(" contract")),
        });
        assert_eq!(engine.suggestion(), Some(" contract"), "{after:?}");
        assert!(engine.can_accept(Instant::now(), false), "{after:?}");
        assert_eq!(engine.can_accept(Instant::now(), true), at_end, "{after:?}");
    }
}
