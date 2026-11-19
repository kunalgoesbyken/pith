use super::*;

#[test]
fn counts_follow_shows_and_accepts_and_drain_once() {
    let mut engine = Engine::new(Config::default());
    suggest(&mut engine, "Please sign the", " contract by Friday.");
    for typed in [
        "Please sign the c",
        "Please sign the co",
        "Please sign the con",
    ] {
        engine.handle(Event::Field(Some(field(typed))));
    }
    engine.handle(Event::AcceptWord);
    engine.handle(Event::AcceptAll);
    let counts = engine.take_counts();
    assert_eq!(counts.shown, 1);
    assert_eq!((counts.accepted, counts.accepted_words), (1, 1));
    assert_eq!(counts.words, 3);
    assert_eq!(
        counts.chars,
        "tract".len() as u64 + " by Friday.".len() as u64
    );
    assert!(engine.take_counts().is_empty());

    let mut engine = Engine::new(Config {
        smart_space: true,
        ..Config::default()
    });
    suggest(&mut engine, "Please sign the", " contract");
    engine.handle(Event::AcceptAll);
    let counts = engine.take_counts();
    assert_eq!(counts.chars, " contract ".len() as u64);
    assert_eq!(counts.words, 1);
}

#[test]
fn dismissing_refused_and_typed_through_suggestions_insert_nothing() {
    let mut engine = Engine::new(Config::default());
    suggest(&mut engine, "Please sign the", " contract");
    engine.handle(Event::Dismiss);
    engine.handle(Event::AcceptAll);
    suggest(&mut engine, "Please sign the d", " contract");
    engine.handle(Event::Field(Some(field("Please sign the d contract"))));
    let counts = engine.take_counts();
    assert_eq!(counts.shown, 2);
    assert_eq!(
        (counts.accepted, counts.accepted_words, counts.words),
        (0, 0, 0)
    );
}

#[test]
fn a_dismiss_holds_until_the_text_changes() {
    let at = |before: &str| Event::Field(Some(field(before)));
    let moved = FieldSnapshot {
        placement: Some(Placement::Inline(Rect {
            x: 50,
            y: 10,
            width: 1,
            height: 16,
        })),
        ..field("Please sign the")
    };
    let rows: [(Vec<Event>, Event, &[&str]); 5] = [
        (vec![], Event::Field(Some(moved)), &[]),
        (vec![], at("Please sign the d"), &["debounce"]),
        (vec![Event::Field(None)], at("Please sign the"), &[]),
        (
            vec![Event::Field(None), at("Please sign the")],
            at("Please sign the c"),
            &["debounce"],
        ),
        (
            vec![at("Please sign the d")],
            at("Please sign the"),
            &["debounce"],
        ),
    ];
    for (before, probe, expected) in rows {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let out = engine.handle(Event::Dismiss);
        assert_eq!(out, vec![Action::Hide, Action::DisarmKeys]);
        for event in before.clone() {
            engine.handle(event);
        }
        let out = engine.handle(probe.clone());
        assert_eq!(kinds(&out), expected, "{before:?} then {probe:?}: {out:?}");
    }
}

fn anchored(before: &str) -> FieldSnapshot {
    FieldSnapshot {
        placement: Some(Placement::Anchored(Rect {
            x: 0,
            y: 0,
            width: 300,
            height: 40,
        })),
        ..field(before)
    }
}

#[test]
fn the_ghost_is_anchored_without_a_trusted_caret_and_follows_placement_switches() {
    let mut engine = Engine::new(Config::default());
    let out = suggest_field(&mut engine, anchored("Please sign the"), " contract");
    let expected = anchored("").placement;
    assert!(
        matches!(&out[0], Action::Show { placement, .. } if Some(*placement) == expected),
        "{out:?}"
    );
    let out = engine.handle(Event::Field(Some(field("Please sign the"))));
    assert!(
        matches!(
            out.as_slice(),
            [Action::Show {
                placement: Placement::Inline(_),
                ..
            }]
        ),
        "{out:?}"
    );
    let out = engine.handle(Event::Field(Some(anchored("Please sign the"))));
    assert!(
        matches!(out.as_slice(), [Action::Show { placement, .. }] if Some(*placement) == expected),
        "{out:?}"
    );
}

#[test]
fn leaving_a_visible_suggestion_hides_it_and_reschedules_only_in_a_usable_field() {
    let same = || field("Please sign the");
    let rows: [(Event, &[&str]); 8] = [
        (
            Event::Field(Some(field("Please sign the x"))),
            &["hide", "disarm", "debounce"],
        ),
        (
            Event::Field(Some(blind("Please sign the"))),
            &["hide", "disarm"],
        ),
        (Event::Field(None), &["hide", "disarm"]),
        (
            Event::Field(Some(FieldSnapshot {
                field_id: 2,
                ..same()
            })),
            &["hide", "disarm", "debounce"],
        ),
        (
            Event::Field(Some(FieldSnapshot {
                app: "wordpad".into(),
                ..same()
            })),
            &["hide", "disarm", "debounce"],
        ),
        (
            Event::Field(Some(FieldSnapshot {
                has_selection: true,
                ..same()
            })),
            &["hide", "disarm"],
        ),
        (
            Event::Field(Some(FieldSnapshot {
                composing: true,
                ..same()
            })),
            &["hide", "disarm"],
        ),
        (
            Event::Field(Some(FieldSnapshot {
                is_password: true,
                ..same()
            })),
            &["hide", "disarm"],
        ),
    ];
    for (event, expected) in rows {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let out = engine.handle(event.clone());
        assert_eq!(kinds(&out), expected, "{event:?}: {out:?}");
        assert!(engine.suggestion().is_none(), "{event:?}");
    }
}

#[test]
fn clearing_a_blocker_reschedules_exactly_once() {
    let blocked = Config {
        blocked_apps: vec!["notepad".into()],
        ..Config::default()
    };
    let selected = FieldSnapshot {
        has_selection: true,
        ..field("Please sign the")
    };
    let rows: [(bool, Event, &[&str], Event); 4] = [
        (
            false,
            Event::Field(Some(selected)),
            &[],
            Event::Field(Some(field("Please sign the"))),
        ),
        (
            false,
            Event::Field(Some(blind("Please sign the"))),
            &[],
            Event::Field(Some(field("Please sign the"))),
        ),
        (
            true,
            Event::SetConfig(Config {
                enabled: false,
                ..Config::default()
            }),
            &["hide", "disarm"],
            Event::SetConfig(Config::default()),
        ),
        (
            true,
            Event::SetConfig(blocked),
            &["hide", "disarm"],
            Event::SetConfig(Config::default()),
        ),
    ];
    for (visible, block, expected, clear) in rows {
        let mut engine = Engine::new(Config::default());
        if visible {
            suggest(&mut engine, "Please sign the", " contract");
        }
        let out = engine.handle(block.clone());
        assert_eq!(kinds(&out), expected, "{block:?}: {out:?}");
        if let Event::SetConfig(config) = &block {
            assert_eq!(engine.config().blocked_apps, config.blocked_apps);
        }
        let out = engine.handle(clear.clone());
        assert_eq!(kinds(&out), ["debounce"], "{block:?}: {out:?}");
    }
}

#[test]
fn low_confidence_drops_cut_word() {
    let mut engine = Engine::new(Config::default());
    let out = engine.handle(Event::Field(Some(field("Please sign the"))));
    let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
        panic!("{out:?}")
    };
    engine.handle(Event::DebounceElapsed(generation));
    let mut cut = completion(" contract by Fri");
    cut.stop = StopReason::LowConfidence;
    let out = engine.handle(Event::Completed {
        generation,
        result: Ok(cut),
    });
    assert!(
        matches!(&out[0], Action::Show { cuts, .. } if cuts[0] == " contract by"),
        "{out:?}"
    );
}

#[test]
fn typing_inside_a_grapheme_requests_again() {
    let mut engine = Engine::new(Config::default());
    suggest(
        &mut engine,
        "Hello \u{928}\u{92e}",
        "\u{938}\u{94d}\u{924}\u{947}",
    );
    let out = engine.handle(Event::Field(Some(field("Hello \u{928}\u{92e}\u{938}"))));
    assert!(
        !out.iter().any(|a| matches!(a, Action::Show { .. })),
        "{out:?}"
    );
}

#[test]
fn dismiss_holds_while_the_caret_visits_other_spots_in_the_text() {
    let mut engine = Engine::new(Config::default());
    suggest(&mut engine, "Please sign the", " contract");
    engine.handle(Event::Dismiss);
    for (before, after) in [("", "Please sign the"), ("Ple", "ase sign the")] {
        let mut moved = field(before);
        moved.after = after.into();
        engine.handle(Event::Field(Some(moved)));
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        assert!(out.is_empty(), "{before:?}: {out:?}");
    }
}

fn long_text(ending: &str) -> String {
    let mut text: String = (0..6000).map(|n| format!("word{n} ")).collect();
    text.push_str(ending);
    text
}

fn slid(document: &str, end: usize, lookback: usize) -> FieldSnapshot {
    field(&document[end.saturating_sub(lookback)..end])
}

#[test]
fn long_document_prompt_start_stays_put_while_typing() {
    let mut engine = Engine::new(Config::default());
    let lookback = engine.config().lookback_chars();
    let document = long_text("Please sign the contract by Friday and send");
    let mut starts = std::collections::HashSet::new();
    for end in document.len() - 30..document.len() {
        let out = engine.handle(Event::Field(Some(slid(&document, end, lookback))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        let out = engine.handle(Event::DebounceElapsed(generation));
        let Some(Action::StartCompletion { request, .. }) = out.last() else {
            panic!("{out:?}")
        };
        starts.insert(request.prompt.chars().take(40).collect::<String>());
    }
    assert!(starts.len() <= 2, "{starts:?}");
}

#[test]
fn behind_snapshots_while_an_insert_is_pending_keep_the_suggestion() {
    let mut engine = Engine::new(Config::default());
    let ending = " contract by Friday afternoon.";
    suggest(&mut engine, "Please sign the", ending);
    engine.handle(Event::AcceptWord);
    let out = engine.handle(Event::Field(Some(field("Please sign the"))));
    assert!(out.is_empty(), "{out:?}");
    assert_eq!(engine.suggestion(), Some(" by Friday afternoon."));
    let out = engine.handle(Event::AcceptWord);
    assert_eq!(out[0], Action::Insert(" by".into()));
    let out = engine.handle(Event::Field(Some(field("Please sign the"))));
    assert!(out.is_empty(), "{out:?}");
    let out = engine.handle(Event::Field(Some(field("Please sign the contract"))));
    assert!(out.is_empty(), "{out:?}");
    let out = engine.handle(Event::AcceptWord);
    assert_eq!(out[0], Action::Insert(" Friday".into()));
    let landed = field("Please sign the contract by Friday");
    let out = engine.handle(Event::Field(Some(landed)));
    assert!(out.is_empty(), "{out:?}");
    assert_eq!(engine.suggestion(), Some(" afternoon."));
}

#[test]
fn a_pending_insert_ends_when_the_field_diverges_reverts_or_is_blocked() {
    let rows = [
        vec![field("Please sign the contract"), field("Please sign the")],
        vec![field("Please sign the x")],
        vec![FieldSnapshot {
            has_selection: true,
            ..field("Please sign the")
        }],
        vec![FieldSnapshot {
            is_password: true,
            ..field("Please sign the")
        }],
    ];
    for row in rows {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::AcceptWord);
        let (last, landed) = row.split_last().unwrap();
        for snapshot in landed {
            let out = engine.handle(Event::Field(Some(snapshot.clone())));
            assert!(out.is_empty(), "{row:?}: {out:?}");
        }
        let out = engine.handle(Event::Field(Some(last.clone())));
        assert_eq!(out[0], Action::Hide, "{row:?}: {out:?}");
        assert!(engine.suggestion().is_none(), "{row:?}");
    }
}

#[test]
fn dismiss_in_one_field_survives_another_field_with_other_text() {
    let mut engine = Engine::new(Config::default());
    suggest(&mut engine, "Please sign the", " contract");
    engine.handle(Event::Dismiss);
    let mut other = field("Dear John");
    other.field_id = 2;
    engine.handle(Event::Field(Some(other)));
    let out = engine.handle(Event::Field(Some(field("Please sign the"))));
    assert!(out.is_empty(), "{out:?}");
}
