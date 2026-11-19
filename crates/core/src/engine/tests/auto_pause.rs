use super::*;
use crate::engine::Phase;
use crate::engine::auto_pause::READ_TIME;

fn pausing(limit: u32) -> Engine {
    Engine::new(Config {
        auto_pause_after: limit,
        ..Config::default()
    })
}

fn in_app(app: &str, before: &str) -> FieldSnapshot {
    FieldSnapshot {
        app: app.into(),
        ..field(before)
    }
}

fn suggest_in(engine: &mut Engine, app: &str, before: &str, text: &str) {
    suggest_field(engine, in_app(app, before), text);
    assert!(engine.suggestion().is_some());
}

fn paused_in(out: &[Action]) -> Option<&Vec<String>> {
    out.iter().find_map(|action| match action {
        Action::AutoPaused(apps) => Some(apps),
        _ => None,
    })
}

fn read_by_user(engine: &mut Engine) {
    if let Phase::Showing(shown) = &mut engine.phase {
        shown.shown_at = shown.shown_at.map(|at| at - READ_TIME);
    }
}

#[test]
fn mismatched_typing_counts_as_ignored_until_the_app_pauses() {
    let mut engine = pausing(3);
    for round in 0..3 {
        suggest_in(&mut engine, "notepad", "Please sign the", " contract");
        read_by_user(&mut engine);
        let out = engine.handle(Event::Field(Some(in_app("notepad", "Please sign thez"))));
        assert_eq!(paused_in(&out).is_some(), round == 2, "{round}: {out:?}");
    }
    let out = engine.handle(Event::Field(Some(in_app("notepad", "Please sign thezz"))));
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn typing_past_a_suggestion_too_brief_to_read_is_not_an_ignore() {
    let mut engine = pausing(3);
    for round in 0..6 {
        suggest_in(&mut engine, "notepad", "Please sign the", " contract");
        let out = engine.handle(Event::Field(Some(in_app("notepad", "Please sign thez"))));
        assert!(paused_in(&out).is_none(), "{round}: {out:?}");
    }
}

#[test]
fn typing_the_suggestion_through_counts_as_accepted() {
    let mut engine = pausing(2);
    for _ in 0..4 {
        suggest_in(&mut engine, "notepad", "Please sign the", " contract");
        engine.handle(Event::Dismiss);
        suggest_in(&mut engine, "notepad", "Please sign the d", " contract");
        let out = engine.handle(Event::Field(Some(in_app(
            "notepad",
            "Please sign the d contract",
        ))));
        assert!(paused_in(&out).is_none(), "{out:?}");
    }
}

#[test]
fn dismissing_after_a_partial_accept_is_not_an_ignore() {
    let mut engine = pausing(1);
    suggest_in(
        &mut engine,
        "notepad",
        "Please sign the",
        " contract by Friday",
    );
    engine.handle(Event::AcceptWord);
    assert!(engine.suggestion().is_some());
    let out = engine.handle(Event::Dismiss);
    assert!(paused_in(&out).is_none(), "{out:?}");
}

#[test]
fn dismissals_pause_one_app_through_config_changes_until_resumed() {
    let mut engine = pausing(2);
    suggest_in(&mut engine, "Slack", "Please sign the", " contract");
    assert!(paused_in(&engine.handle(Event::Dismiss)).is_none());
    suggest_in(&mut engine, "Slack", "Please sign the d", " contract");
    let out = engine.handle(Event::Dismiss);
    assert_eq!(paused_in(&out), Some(&vec!["Slack".to_string()]));

    let config = engine.config().clone();
    engine.handle(Event::SetConfig(config));
    let out = engine.handle(Event::Field(Some(in_app("Slack", "Please sign thex"))));
    assert!(out.is_empty(), "{out:?}");

    let out = engine.handle(Event::Field(Some(in_app("notepad", "Please sign the"))));
    assert_eq!(kinds(&out), ["debounce"], "{out:?}");

    let out = engine.handle(Event::Field(Some(in_app("Slack", "Please sign thexy"))));
    assert!(out.is_empty(), "{out:?}");
    let out = engine.handle(Event::Resume("slack".into()));
    assert_eq!(paused_in(&out), Some(&Vec::new()));
    assert_eq!(kinds(&out), ["other", "debounce"], "{out:?}");
    assert!(engine.handle(Event::Resume("slack".into())).is_empty());
}
