use super::*;
use crate::TriggerMode;

fn trigger_now(engine: &mut Engine) -> u64 {
    let out = engine.handle(Event::Trigger);
    match out.as_slice() {
        [Action::StartCompletion { generation, .. }] => *generation,
        _ => panic!("{out:?}"),
    }
}

#[test]
fn trigger_suggests_in_a_field_that_is_too_short_and_keeps_it() {
    let mut engine = Engine::new(Config::default());
    let out = engine.handle(Event::Field(Some(field("Hi"))));
    assert!(out.is_empty(), "{out:?}");
    let generation = trigger_now(&mut engine);
    let out = engine.handle(Event::Completed {
        generation,
        result: Ok(completion(" there, how are you?")),
    });
    assert!(matches!(out.as_slice(), [Action::Show { .. }]), "{out:?}");
    let out = engine.handle(Event::Drawn(Drawn::Shown(0)));
    assert_eq!(out, [Action::ArmKeys]);
    for _ in 0..3 {
        let out = engine.handle(Event::Field(Some(field("Hi"))));
        assert!(out.is_empty(), "{out:?}");
    }
    assert_eq!(engine.suggestion(), Some(" there, how are you?"));
    let out = engine.handle(Event::Field(Some(field("Hi t"))));
    assert!(
        matches!(out.as_slice(), [Action::Show { cuts, .. }] if cuts[0] == "here, how are you?"),
        "{out:?}"
    );
}

#[test]
fn trigger_starts_immediately_and_supersedes_a_pending_debounce() {
    let mut engine = Engine::new(Config::default());
    let out = engine.handle(Event::Field(Some(field("Please sign the"))));
    let Some(Action::ScheduleDebounce {
        generation: old, ..
    }) = out.last().cloned()
    else {
        panic!("{out:?}")
    };
    let generation = trigger_now(&mut engine);
    assert_ne!(generation, old);
    assert!(engine.handle(Event::DebounceElapsed(old)).is_empty());
}

#[test]
fn trigger_is_ignored_when_it_cannot_help() {
    let mut engine = Engine::new(Config::default());
    assert!(engine.handle(Event::Trigger).is_empty());
    assert!(
        engine
            .handle(Event::Field(Some(blind("Please sign the"))))
            .is_empty()
    );
    assert!(engine.handle(Event::Trigger).is_empty());
    for change in [
        (|f: &mut FieldSnapshot| f.is_password = true) as fn(&mut FieldSnapshot),
        |f| f.composing = true,
        |f| f.has_selection = true,
        |f| f.app = crate::apps::sample(crate::apps::Surface::Terminal).into(),
        |f| f.after = " tail".into(),
        |f| f.before = "  ".into(),
    ] {
        let mut blocked = field("Hi");
        change(&mut blocked);
        engine.handle(Event::Field(Some(blocked)));
        assert!(engine.handle(Event::Trigger).is_empty());
    }
    let mut engine = Engine::new(Config {
        enabled: false,
        ..Config::default()
    });
    engine.handle(Event::Field(Some(field("Please sign the"))));
    assert!(engine.handle(Event::Trigger).is_empty());
}

#[test]
fn trigger_does_nothing_while_a_suggestion_is_visible_or_in_flight_but_overrides_a_dismiss() {
    let mut engine = Engine::new(Config::default());
    let out = engine.handle(Event::Field(Some(field("Please sign the"))));
    let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
        panic!("{out:?}")
    };
    engine.handle(Event::DebounceElapsed(generation));
    assert!(engine.handle(Event::Trigger).is_empty());
    engine.handle(Event::Completed {
        generation,
        result: Ok(completion(" contract")),
    });
    assert!(engine.handle(Event::Trigger).is_empty());
    assert_eq!(engine.suggestion(), Some(" contract"));

    engine.handle(Event::Dismiss);
    let generation = trigger_now(&mut engine);
    let out = engine.handle(Event::Completed {
        generation,
        result: Ok(completion(" contract")),
    });
    assert!(matches!(&out[0], Action::Show { .. }), "{out:?}");
}

fn strict() -> Engine {
    Engine::new(Config {
        min_chars_before: 20,
        ..Config::default()
    })
}

#[test]
fn forcing_ends_when_the_suggestion_is_used_up_the_text_diverges_or_the_field_changes() {
    let mut engine = strict();
    engine.handle(Event::Field(Some(field("Hi"))));
    let generation = trigger_now(&mut engine);
    engine.handle(Event::Completed {
        generation,
        result: Ok(completion(" there")),
    });
    engine.handle(Event::AcceptAll);
    let out = engine.handle(Event::Field(Some(field("Hi there"))));
    assert!(out.is_empty(), "{out:?}");
    let out = engine.handle(Event::Field(Some(field("Hi there!"))));
    assert!(no_model_work(&out), "{out:?}");

    let mut engine = strict();
    engine.handle(Event::Field(Some(field("Hi"))));
    let generation = trigger_now(&mut engine);
    engine.handle(Event::Completed {
        generation,
        result: Ok(completion(" there")),
    });
    let out = engine.handle(Event::Field(Some(field("Hi x"))));
    assert!(no_model_work(&out), "{out:?}");
    assert!(engine.suggestion().is_none());
    trigger_now(&mut engine);
    let mut other = field("Hi");
    other.field_id = 2;
    engine.handle(Event::Field(Some(other)));
    assert!(engine.suggestion().is_none());
}

fn manual() -> Engine {
    Engine::new(Config {
        trigger_mode: TriggerMode::Manual,
        ..Config::default()
    })
}

#[test]
fn manual_mode_never_asks_on_its_own_and_stays_manual_after_a_trigger() {
    let mut engine = manual();
    for text in ["Please sign the", "Please sign the c", "Please sign the co"] {
        let out = engine.handle(Event::Field(Some(field(text))));
        assert!(out.is_empty(), "{out:?}");
    }
    assert!(engine.handle(Event::DebounceElapsed(0)).is_empty());
    let unchanged = engine.config().clone();
    let out = engine.handle(Event::SetConfig(unchanged));
    assert!(no_model_work(&out), "{out:?}");
    let out = engine.handle(Event::Field(None));
    assert!(out.is_empty(), "{out:?}");

    let mut engine = manual();
    engine.handle(Event::Field(Some(field("Please sign the"))));
    let generation = trigger_now(&mut engine);
    let out = engine.handle(Event::Completed {
        generation,
        result: Ok(completion(" contract by Friday.")),
    });
    assert!(matches!(&out[0], Action::Show { .. }), "{out:?}");
    let out = engine.handle(Event::AcceptAll);
    assert_eq!(out[0], Action::Insert(" contract by Friday.".into()));
    assert!(no_model_work(&out), "{out:?}");
    for text in [
        "Please sign the contract by Friday.",
        "Please sign the contract by Friday. T",
    ] {
        let out = engine.handle(Event::Field(Some(field(text))));
        assert!(no_model_work(&out), "{out:?}");
    }
    engine.handle(Event::Dismiss);
    trigger_now(&mut engine);
}
