use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    Completion, CompletionRequest, Config, FieldSnapshot, Rect, SkipReason, TriggerMode, build,
    check, check_forced, continue_window, next_word, suggestion,
};

fn is_grapheme_boundary(text: &str, index: usize) -> bool {
    index == text.len() || text.grapheme_indices(true).any(|(start, _)| start == index)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Field(Option<FieldSnapshot>),
    DebounceElapsed(u64),
    Completed {
        generation: u64,
        result: Result<Completion, String>,
    },
    AcceptAll,
    AcceptWord,
    Dismiss,
    Trigger,
    SetEnabled(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    ScheduleDebounce {
        generation: u64,
        delay_ms: u64,
    },
    StartCompletion {
        generation: u64,
        request: CompletionRequest,
    },
    CancelCompletion,
    Show {
        text: String,
        caret: Rect,
        font_family: Option<String>,
        font_px: Option<f32>,
        color: Option<u32>,
    },
    Hide,
    Insert(String),
    ArmKeys,
    DisarmKeys,
}

pub struct Engine {
    config: Config,
    field: Option<FieldSnapshot>,
    generation: u64,
    in_flight: bool,
    suggestion: Option<String>,
    dismissed: Option<(u64, String)>,
    forced: Option<(u64, String)>,
    shown_at: Option<Instant>,
    unlanded: Vec<String>,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            field: None,
            generation: 0,
            in_flight: false,
            suggestion: None,
            dismissed: None,
            forced: None,
            shown_at: None,
            unlanded: Vec::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn suggestion(&self) -> Option<&str> {
        self.suggestion.as_deref()
    }

    pub fn can_accept(&self, now: Instant, moves_caret: bool) -> bool {
        let Some(field) = &self.field else {
            return false;
        };
        let racing = self.shown_at.is_some_and(|shown| {
            now.saturating_duration_since(shown) < Duration::from_millis(self.config.accept_race_ms)
        });
        self.suggestion.is_some()
            && !racing
            && (!moves_caret || field.after.trim().is_empty())
    }

    pub fn handle(&mut self, event: Event) -> Vec<Action> {
        let mut out = Vec::new();
        match event {
            Event::Field(None) => {
                self.reset(&mut out);
                self.field = None;
            }
            Event::Field(Some(field)) => self.on_field(field, &mut out),
            Event::DebounceElapsed(generation) => self.on_debounce(generation, &mut out),
            Event::Completed { generation, result } => {
                self.on_completed(generation, result, &mut out)
            }
            Event::AcceptAll => self.on_accept(false, &mut out),
            Event::AcceptWord => self.on_accept(true, &mut out),
            Event::Dismiss => {
                self.reset(&mut out);
                self.dismissed = self
                    .field
                    .as_ref()
                    .map(|field| (field.field_id, field.before.clone()));
            }
            Event::Trigger => self.on_trigger(&mut out),
            Event::SetEnabled(enabled) => {
                self.config.enabled = enabled;
                self.reset(&mut out);
                self.schedule(&mut out);
            }
        }
        out
    }

    fn reset(&mut self, out: &mut Vec<Action>) {
        if self.in_flight {
            out.push(Action::CancelCompletion);
            self.in_flight = false;
        }
        if self.suggestion.take().is_some() {
            out.push(Action::Hide);
            out.push(Action::DisarmKeys);
        }
        self.forced = None;
        self.shown_at = None;
        self.unlanded.clear();
        self.generation += 1;
    }

    fn eligible(&self, field: &FieldSnapshot) -> Result<(), SkipReason> {
        let forced = self
            .forced
            .as_ref()
            .is_some_and(|(id, app)| *id == field.field_id && *app == field.app);
        if forced {
            check_forced(field, &self.config)
        } else {
            check(field, &self.config)
        }
    }

    fn on_trigger(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        if self.suggestion.is_some()
            || self.in_flight
            || field.caret.is_none()
            || check_forced(field, &self.config).is_err()
        {
            return;
        }
        let forced = (field.field_id, field.app.clone());
        self.dismissed = None;
        self.reset(out);
        self.forced = Some(forced);
        self.in_flight = true;
        if let Some(field) = &self.field {
            out.push(Action::StartCompletion {
                generation: self.generation,
                request: build(field, &self.config),
            });
        }
    }


    fn insert_pending(&mut self, field: &FieldSnapshot) -> bool {
        if self.unlanded.is_empty() {
            return false;
        }
        let position = self
            .field
            .as_ref()
            .filter(|previous| previous.field_id == field.field_id && previous.app == field.app)
            .filter(|_| self.eligible(field).is_ok())
            .and_then(|previous| {
                let mut candidates = vec![previous.before.as_str()];
                for accepted in self.unlanded.iter().rev() {
                    let Some(rest) = candidates
                        .last()
                        .and_then(|text| text.strip_suffix(accepted.as_str()))
                    else {
                        break;
                    };
                    candidates.push(rest);
                }
                let window = self.config.lookback_chars() / 2;
                candidates.iter().position(|candidate| {
                    *candidate == field.before
                        || (field.before.chars().count() >= window
                            && candidate.ends_with(field.before.as_str()))
                })
            });
        match position {
            Some(behind) if behind > 0 => {
                let landed = self.unlanded.len() - behind;
                self.unlanded.drain(..landed);
                true
            }
            _ => {
                self.unlanded.clear();
                false
            }
        }
    }

    fn on_field(&mut self, mut field: FieldSnapshot, out: &mut Vec<Action>) {
        if self.insert_pending(&field) {
            return;
        }
        if let Some(previous) = &self.field
            && previous.field_id == field.field_id
            && previous.app == field.app
        {
            field.before = continue_window(
                &previous.before,
                &field.before,
                self.config.lookback_chars(),
            );
        }
        self.dismissed
            .take_if(|(id, before)| *id == field.field_id && *before != field.before);
        if let Some(previous) = &self.field {
            let same_field = previous.field_id == field.field_id
                && previous.app == field.app
                && self.eligible(&field).is_ok();
            if same_field && previous.before == field.before && previous.after == field.after {
                let moved = previous.caret != field.caret;
                let unblocked = self.eligible(previous).is_err()
                    || (previous.caret.is_none() && self.suggestion.is_none() && !self.in_flight);
                self.field = Some(field);
                if unblocked {
                    self.schedule(out);
                } else if moved {
                    self.show_current(out);
                }
                return;
            }
            if same_field && let Some(rest) = self.typed_through(previous, &field) {
                self.field = Some(field);
                if rest.trim().is_empty() {
                    self.suggestion = None;
                    self.forced = None;
                    out.push(Action::Hide);
                    out.push(Action::DisarmKeys);
                    self.schedule(out);
                } else {
                    self.suggestion = Some(rest);
                    self.show_current(out);
                }
                return;
            }
        }
        self.reset(out);
        self.field = Some(field);
        self.schedule(out);
    }

    fn typed_through(&self, previous: &FieldSnapshot, field: &FieldSnapshot) -> Option<String> {
        let suggestion = self.suggestion.as_ref()?;
        let typed = field.before.strip_prefix(previous.before.as_str())?;
        if typed.is_empty() || previous.after != field.after {
            return None;
        }
        let rest = suggestion.strip_prefix(typed)?;
        is_grapheme_boundary(suggestion, typed.len()).then(|| rest.to_string())
    }

    fn schedule(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        let dismissed = self
            .dismissed
            .as_ref()
            .is_some_and(|(id, before)| *id == field.field_id && *before == field.before);
        if dismissed
            || self.config.trigger_mode == TriggerMode::Manual
            || field.caret.is_none()
            || check(field, &self.config).is_err()
        {
            return;
        }
        out.push(Action::ScheduleDebounce {
            generation: self.generation,
            delay_ms: self.config.debounce_ms,
        });
    }

    fn on_debounce(&mut self, generation: u64, out: &mut Vec<Action>) {
        if generation != self.generation || self.in_flight || self.suggestion.is_some() {
            return;
        }
        let Some(field) = &self.field else { return };
        if field.caret.is_none() || self.eligible(field).is_err() {
            return;
        }
        self.in_flight = true;
        out.push(Action::StartCompletion {
            generation,
            request: build(field, &self.config),
        });
    }

    fn on_completed(
        &mut self,
        generation: u64,
        result: Result<Completion, String>,
        out: &mut Vec<Action>,
    ) {
        if generation != self.generation || !self.in_flight {
            return;
        }
        self.in_flight = false;
        let Ok(completion) = result else { return };
        let Some(field) = &self.field else { return };
        let Some(text) = suggestion(&completion, &field.before, &self.config) else {
            return;
        };
        if field.caret.is_none() {
            return;
        }
        self.suggestion = Some(text);
        self.shown_at = Some(Instant::now());
        self.show_current(out);
        out.push(Action::ArmKeys);
    }

    fn on_accept(&mut self, one_word: bool, out: &mut Vec<Action>) {
        let Some(suggestion) = self.suggestion.take() else {
            return;
        };
        let accepted = if one_word {
            next_word(&suggestion).to_string()
        } else {
            suggestion.clone()
        };
        let rest = suggestion[accepted.len()..].to_string();
        self.shown_at = None;
        out.push(Action::Insert(accepted.clone()));
        if let Some(field) = &mut self.field {
            field.before.push_str(&accepted);
        }
        self.unlanded.push(accepted);
        if rest.trim().is_empty() {
            self.forced = None;
            out.push(Action::Hide);
            out.push(Action::DisarmKeys);
            self.generation += 1;
            self.schedule(out);
        } else {
            self.suggestion = Some(rest);
            self.show_current(out);
        }
    }

    fn show_current(&mut self, out: &mut Vec<Action>) {
        let (Some(text), Some(field)) = (&self.suggestion, &self.field) else {
            return;
        };
        match field.caret {
            Some(caret) => out.push(Action::Show {
                text: text.clone(),
                caret,
                font_family: field.font_family.clone(),
                font_px: field.font_px,
                color: field.text_color,
            }),
            None => {
                self.suggestion = None;
                out.push(Action::Hide);
                out.push(Action::DisarmKeys);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StopReason;

    fn field(before: &str) -> FieldSnapshot {
        FieldSnapshot {
            field_id: 1,
            app: "notepad".into(),
            before: before.into(),
            caret: Some(Rect {
                x: 10,
                y: 10,
                width: 1,
                height: 16,
            }),
            ..Default::default()
        }
    }

    fn completion(text: &str) -> Completion {
        Completion {
            text: text.into(),
            mean_logprob: -0.5,
            tokens: 4,
            prompt_tokens_evaluated: 3,
            stop: StopReason::Sentence,
        }
    }

    fn suggest(engine: &mut Engine, before: &str, text: &str) -> Vec<Action> {
        let out = engine.handle(Event::Field(Some(field(before))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        let out = engine.handle(Event::DebounceElapsed(generation));
        assert!(
            matches!(out.as_slice(), [Action::StartCompletion { .. }]),
            "{out:?}"
        );
        engine.handle(Event::Completed {
            generation,
            result: Ok(completion(text)),
        })
    }

    #[test]
    fn shows_and_arms_after_completion() {
        let mut engine = Engine::new(Config::default());
        let out = suggest(&mut engine, "Please sign the", " contract by Friday.");
        assert!(matches!(&out[0], Action::Show { text, .. } if text == " contract by Friday."));
        assert_eq!(out[1], Action::ArmKeys);
    }

    #[test]
    fn typing_through_shrinks_without_new_request() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        let out = engine.handle(Event::Field(Some(field("Please sign the con"))));
        assert!(
            matches!(out.as_slice(), [Action::Show { text, .. }] if text == "tract by Friday."),
            "{out:?}"
        );
    }

    #[test]
    fn diverging_input_hides_and_reschedules() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        let out = engine.handle(Event::Field(Some(field("Please sign the x"))));
        assert_eq!(out[0], Action::Hide);
        assert_eq!(out[1], Action::DisarmKeys);
        assert!(matches!(out[2], Action::ScheduleDebounce { .. }));
    }

    #[test]
    fn accept_word_then_rest() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        let out = engine.handle(Event::AcceptWord);
        assert_eq!(out[0], Action::Insert(" contract".into()));
        assert!(matches!(&out[1], Action::Show { text, .. } if text == " by Friday."));
        let out = engine.handle(Event::Field(Some(field("Please sign the contract"))));
        assert!(out.is_empty(), "{out:?}");
        let out = engine.handle(Event::AcceptAll);
        assert_eq!(out[0], Action::Insert(" by Friday.".into()));
        assert_eq!(out[1], Action::Hide);
    }

    #[test]
    fn double_accept_word_never_reinserts() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        let first = engine.handle(Event::AcceptWord);
        let second = engine.handle(Event::AcceptWord);
        assert_eq!(first[0], Action::Insert(" contract".into()));
        assert_eq!(second[0], Action::Insert(" by".into()));
    }

    #[test]
    fn stale_completion_ignored() {
        let mut engine = Engine::new(Config::default());
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        let Action::ScheduleDebounce { generation, .. } = out[0] else {
            panic!()
        };
        engine.handle(Event::DebounceElapsed(generation));
        let out = engine.handle(Event::Field(Some(field("Please sign them"))));
        assert_eq!(out[0], Action::CancelCompletion);
        let out = engine.handle(Event::Completed {
            generation,
            result: Ok(completion(" now")),
        });
        assert!(out.is_empty());
    }

    #[test]
    fn low_confidence_and_cancelled_hidden() {
        let mut engine = Engine::new(Config::default());
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        let Action::ScheduleDebounce { generation, .. } = out[0] else {
            panic!()
        };
        engine.handle(Event::DebounceElapsed(generation));
        let weak = Completion {
            mean_logprob: -5.0,
            ..completion(" contract")
        };
        assert!(
            engine
                .handle(Event::Completed {
                    generation,
                    result: Ok(weak)
                })
                .is_empty()
        );
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn dismiss_blocks_until_text_changes() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let out = engine.handle(Event::Dismiss);
        assert_eq!(out, vec![Action::Hide, Action::DisarmKeys]);
        let mut moved = field("Please sign the");
        moved.caret = Some(Rect {
            x: 50,
            y: 10,
            width: 1,
            height: 16,
        });
        assert!(engine.handle(Event::Field(Some(moved))).is_empty());
        let out = engine.handle(Event::Field(Some(field("Please sign the d"))));
        assert!(
            matches!(out.as_slice(), [Action::ScheduleDebounce { .. }]),
            "{out:?}"
        );
    }

    #[test]
    fn trailing_space_strips_leading_space_of_suggestion() {
        let mut engine = Engine::new(Config::default());
        let out = suggest(&mut engine, "Please sign the ", " contract");
        assert!(matches!(&out[0], Action::Show { text, .. } if text == "contract"));
    }

    fn blind(before: &str) -> FieldSnapshot {
        FieldSnapshot {
            caret: None,
            ..field(before)
        }
    }

    #[test]
    fn never_infers_without_a_visible_caret() {
        let mut engine = Engine::new(Config::default());
        for _ in 0..3 {
            let out = engine.handle(Event::Field(Some(blind("Please sign the"))));
            assert!(out.is_empty(), "{out:?}");
        }
        assert!(engine.handle(Event::DebounceElapsed(0)).is_empty());
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn caret_lost_before_the_debounce_fires_starts_nothing() {
        let mut engine = Engine::new(Config::default());
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        assert!(
            engine
                .handle(Event::Field(Some(blind("Please sign the"))))
                .is_empty()
        );
        assert!(engine.handle(Event::DebounceElapsed(generation)).is_empty());
    }

    #[test]
    fn caret_lost_while_visible_hides_and_disarms() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let mut blind = field("Please sign the");
        blind.caret = None;
        let out = engine.handle(Event::Field(Some(blind)));
        assert_eq!(out, vec![Action::Hide, Action::DisarmKeys]);
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn focus_loss_resets() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let out = engine.handle(Event::Field(None));
        assert_eq!(out, vec![Action::Hide, Action::DisarmKeys]);
    }

    #[test]
    fn other_field_with_same_text_reschedules() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let mut other = field("Please sign the");
        other.field_id = 2;
        let out = engine.handle(Event::Field(Some(other)));
        assert_eq!(out[0], Action::Hide);
        assert!(matches!(out.last(), Some(Action::ScheduleDebounce { .. })));
    }

    #[test]
    fn selecting_while_visible_hides_and_disarms() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        for change in [
            (|f: &mut FieldSnapshot| f.has_selection = true) as fn(&mut FieldSnapshot),
            |f| f.composing = true,
            |f| f.is_password = true,
        ] {
            let mut changed = field("Please sign the");
            change(&mut changed);
            let out = engine.handle(Event::Field(Some(changed)));
            assert!(engine.suggestion().is_none());
            assert!(
                !out.iter()
                    .any(|a| matches!(a, Action::ScheduleDebounce { .. })),
                "{out:?}"
            );
            suggest(&mut engine, "Please sign the", " contract");
        }
    }

    #[test]
    fn clearing_selection_reschedules() {
        let mut engine = Engine::new(Config::default());
        let mut selected = field("Please sign the");
        selected.has_selection = true;
        engine.handle(Event::Field(Some(selected)));
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        assert!(
            matches!(out.last(), Some(Action::ScheduleDebounce { .. })),
            "{out:?}"
        );
    }

    #[test]
    fn resume_reschedules_same_field() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let out = engine.handle(Event::SetEnabled(false));
        assert_eq!(out, vec![Action::Hide, Action::DisarmKeys]);
        let out = engine.handle(Event::SetEnabled(true));
        assert!(
            matches!(out.as_slice(), [Action::ScheduleDebounce { .. }]),
            "{out:?}"
        );
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
            matches!(&out[0], Action::Show { text, .. } if text == " contract by"),
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
    fn dismiss_holds_through_focus_flicker_but_not_new_text() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        engine.handle(Event::Dismiss);
        engine.handle(Event::Field(None));
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        assert!(out.is_empty(), "{out:?}");
        let out = engine.handle(Event::Field(Some(field("Please sign the c"))));
        assert!(
            matches!(out.last(), Some(Action::ScheduleDebounce { .. })),
            "{out:?}"
        );
    }

    #[test]
    fn caret_returning_reschedules_once() {
        let mut engine = Engine::new(Config::default());
        assert!(
            engine
                .handle(Event::Field(Some(blind("Please sign the"))))
                .is_empty()
        );
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        assert!(
            matches!(out.as_slice(), [Action::ScheduleDebounce { .. }]),
            "{out:?}"
        );
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
    fn long_document_typing_through_and_accepting_keep_the_suggestion() {
        let mut engine = Engine::new(Config::default());
        let lookback = engine.config().lookback_chars();
        let document = long_text("Please sign the");
        let end = document.len();
        let out = engine.handle(Event::Field(Some(slid(&document, end, lookback))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        engine.handle(Event::DebounceElapsed(generation));
        engine.handle(Event::Completed {
            generation,
            result: Ok(completion(" contract by Friday.")),
        });
        assert_eq!(engine.suggestion(), Some(" contract by Friday."));
        let typed = format!("{document} con");
        let out = engine.handle(Event::Field(Some(slid(&typed, typed.len(), lookback))));
        assert!(
            matches!(out.as_slice(), [Action::Show { text, .. }] if text == "tract by Friday."),
            "{out:?}"
        );
        let out = engine.handle(Event::AcceptWord);
        assert_eq!(out[0], Action::Insert("tract".into()));
        let accepted = format!("{typed}tract");
        let landed = slid(&accepted, accepted.len(), lookback);
        let out = engine.handle(Event::Field(Some(landed)));
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(engine.suggestion(), Some(" by Friday."));
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
    fn a_landed_or_diverged_field_ends_the_pending_insert() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::AcceptWord);
        let out = engine.handle(Event::Field(Some(field("Please sign the contract"))));
        assert!(out.is_empty(), "{out:?}");
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        assert_eq!(out[0], Action::Hide, "{out:?}");
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::AcceptWord);
        let out = engine.handle(Event::Field(Some(field("Please sign the x"))));
        assert_eq!(out[0], Action::Hide, "{out:?}");
        assert!(engine.suggestion().is_none());
    }

    #[test]
    fn a_pending_insert_never_hides_a_selection_or_password() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::AcceptWord);
        let mut selected = field("Please sign the");
        selected.has_selection = true;
        let out = engine.handle(Event::Field(Some(selected)));
        assert_eq!(out[0], Action::Hide, "{out:?}");
    }

    #[test]
    fn same_id_in_another_app_is_another_field() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let mut other = field("Please sign the");
        other.app = "wordpad".into();
        let out = engine.handle(Event::Field(Some(other)));
        assert_eq!(out[0], Action::Hide);
    }

    #[test]
    fn dismiss_ends_once_the_text_changes() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        engine.handle(Event::Dismiss);
        engine.handle(Event::Field(Some(field("Please sign the d"))));
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        assert!(
            matches!(out.last(), Some(Action::ScheduleDebounce { .. })),
            "{out:?}"
        );
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

    #[test]
    fn nan_confidence_is_never_shown() {
        let mut engine = Engine::new(Config::default());
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        engine.handle(Event::DebounceElapsed(generation));
        let unknown = Completion {
            mean_logprob: f32::NAN,
            ..completion(" contract")
        };
        let out = engine.handle(Event::Completed {
            generation,
            result: Ok(unknown),
        });
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn duplicate_completion_for_one_request_is_ignored() {
        let mut engine = Engine::new(Config::default());
        let out = engine.handle(Event::Field(Some(field("Please sign the"))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        engine.handle(Event::DebounceElapsed(generation));
        let first = engine.handle(Event::Completed {
            generation,
            result: Ok(completion(" contract")),
        });
        assert_eq!(first.len(), 2);
        let again = engine.handle(Event::Completed {
            generation,
            result: Ok(completion(" other")),
        });
        assert!(again.is_empty(), "{again:?}");
        assert_eq!(engine.suggestion(), Some(" contract"));
    }
}
