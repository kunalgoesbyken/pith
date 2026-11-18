use crate::{
    Completion, CompletionRequest, Config, FieldSnapshot, Rect, StopReason, build, check,
    clean_completion, next_word,
};

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
    dismissed: bool,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            field: None,
            generation: 0,
            in_flight: false,
            suggestion: None,
            dismissed: false,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn suggestion(&self) -> Option<&str> {
        self.suggestion.as_deref()
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
                self.dismissed = true;
            }
            Event::SetEnabled(enabled) => {
                self.config.enabled = enabled;
                if !enabled {
                    self.reset(&mut out);
                }
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
        self.generation += 1;
    }

    fn on_field(&mut self, field: FieldSnapshot, out: &mut Vec<Action>) {
        if let Some(previous) = &self.field {
            let same_field = previous.field_id == field.field_id;
            if same_field && previous.before == field.before && previous.after == field.after {
                let moved = previous.caret != field.caret;
                self.field = Some(field);
                if moved {
                    self.show_current(out);
                }
                return;
            }
            if same_field && let Some(rest) = self.typed_through(previous, &field) {
                self.field = Some(field);
                if rest.trim().is_empty() {
                    self.suggestion = None;
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
        self.dismissed = false;
        self.field = Some(field);
        self.schedule(out);
    }

    fn typed_through(&self, previous: &FieldSnapshot, field: &FieldSnapshot) -> Option<String> {
        let suggestion = self.suggestion.as_ref()?;
        let typed = field.before.strip_prefix(previous.before.as_str())?;
        if typed.is_empty() || previous.after != field.after {
            return None;
        }
        suggestion.strip_prefix(typed).map(str::to_string)
    }

    fn schedule(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        if self.dismissed || check(field, &self.config).is_err() {
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
        if check(field, &self.config).is_err() {
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
        if generation != self.generation {
            return;
        }
        self.in_flight = false;
        let Ok(completion) = result else { return };
        if completion.stop == StopReason::Cancelled
            || completion.mean_logprob < self.config.min_mean_logprob
        {
            return;
        }
        let truncated = completion.stop == StopReason::MaxTokens;
        let Some(text) = clean_completion(&completion.text, self.config.max_words, truncated)
        else {
            return;
        };
        let Some(field) = &self.field else { return };
        let text = if field.before.ends_with(char::is_whitespace) {
            text.trim_start().to_string()
        } else {
            text
        };
        if text.is_empty() {
            return;
        }
        self.suggestion = Some(text);
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
        out.push(Action::Insert(accepted.clone()));
        if let Some(field) = &mut self.field {
            field.before.push_str(&accepted);
        }
        if rest.trim().is_empty() {
            out.push(Action::Hide);
            out.push(Action::DisarmKeys);
            self.generation += 1;
            self.schedule(out);
        } else {
            self.suggestion = Some(rest);
            self.show_current(out);
        }
    }

    fn show_current(&self, out: &mut Vec<Action>) {
        let (Some(text), Some(field)) = (&self.suggestion, &self.field) else {
            return;
        };
        let Some(caret) = field.caret else { return };
        out.push(Action::Show {
            text: text.clone(),
            caret,
            font_family: field.font_family.clone(),
            font_px: field.font_px,
            color: field.text_color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
