mod accept;
mod auto_pause;
mod trigger;
mod typing;

use super::{Action, Engine, Event};
use crate::{Completion, Config, Drawn, FieldSnapshot, Placement, Rect, StopReason};

fn field(before: &str) -> FieldSnapshot {
    FieldSnapshot {
        field_id: 1,
        app: "notepad".into(),
        before: before.into(),
        placement: Some(Placement::Inline(Rect {
            x: 10,
            y: 10,
            width: 1,
            height: 16,
        })),
        ..Default::default()
    }
}

fn completion(text: &str) -> Completion {
    Completion {
        text: text.into(),
        mean_logprob: Some(-0.5),
        tokens: 4,
        prompt_tokens_evaluated: 3,
        stop: StopReason::Sentence,
    }
}

fn suggest_field(engine: &mut Engine, snapshot: FieldSnapshot, text: &str) -> Vec<Action> {
    let out = engine.handle(Event::Field(Some(snapshot)));
    let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
        panic!("{out:?}")
    };
    let out = engine.handle(Event::DebounceElapsed(generation));
    assert!(
        matches!(out.as_slice(), [Action::StartCompletion { .. }]),
        "{out:?}"
    );
    let out = engine.handle(Event::Completed {
        generation,
        result: Ok(completion(text)),
    });
    engine.handle(Event::Drawn(Drawn::Shown(0)));
    out
}

fn suggest(engine: &mut Engine, before: &str, text: &str) -> Vec<Action> {
    suggest_field(engine, field(before), text)
}

fn kinds(out: &[Action]) -> Vec<&'static str> {
    out.iter()
        .map(|action| match action {
            Action::Hide => "hide",
            Action::DisarmKeys => "disarm",
            Action::ScheduleDebounce { .. } => "debounce",
            _ => "other",
        })
        .collect()
}

fn blind(before: &str) -> FieldSnapshot {
    FieldSnapshot {
        placement: None,
        ..field(before)
    }
}

fn no_model_work(out: &[Action]) -> bool {
    !out.iter().any(|a| {
        matches!(
            a,
            Action::StartCompletion { .. }
                | Action::ScheduleDebounce { .. }
                | Action::CancelCompletion
        )
    })
}
