use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use crate::ignore::IgnoreTracker;
use crate::stats::Counts;
use crate::{
    Completion, CompletionRequest, Config, FieldSnapshot, Placement, SkipReason, TriggerMode,
    build, check, check_forced, continue_window, ends_spaced_word, next_word, suggestion,
};

const READ_TIME: Duration = Duration::from_millis(500);

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
    Resume(String),
    SetConfig(Config),
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
        placement: Placement,
        font_family: Option<String>,
        font_px: Option<f32>,
        color: Option<u32>,
    },
    Hide,
    Insert(String),
    ArmKeys,
    DisarmKeys,
    AutoPaused(Vec<String>),
}

fn closes_spaced_word(field: &FieldSnapshot, accepted: &str) -> bool {
    !field.after.starts_with(char::is_whitespace)
        && ends_spaced_word(&format!("{}{accepted}", field.before))
}

pub struct Engine {
    config: Config,
    field: Option<FieldSnapshot>,
    generation: u64,
    in_flight: bool,
    suggestion: Option<String>,
    engaged: bool,
    dismissed: Option<(u64, String)>,
    ignores: IgnoreTracker,
    forced: Option<(u64, String)>,
    shown_at: Option<Instant>,
    unlanded: Vec<String>,
    counts: Counts,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            field: None,
            generation: 0,
            in_flight: false,
            suggestion: None,
            engaged: false,
            dismissed: None,
            ignores: IgnoreTracker::default(),
            forced: None,
            shown_at: None,
            unlanded: Vec::new(),
            counts: Counts::default(),
        }
    }

    pub fn take_counts(&mut self) -> Counts {
        std::mem::take(&mut self.counts)
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
        self.suggestion.is_some() && !racing && (!moves_caret || field.after.trim().is_empty())
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
                let ignored = self.unengaged_app();
                self.reset(&mut out);
                if let Some(app) = ignored {
                    self.note_ignored(&app, &mut out);
                }
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
            Event::Resume(app) => {
                if self.ignores.resume(&app) {
                    out.push(Action::AutoPaused(self.ignores.paused()));
                    self.schedule(&mut out);
                }
            }
            Event::SetConfig(config) => {
                self.config = config;
                self.reset(&mut out);
                self.schedule(&mut out);
            }
        }
        out
    }

    fn was_readable(&self) -> bool {
        self.shown_at
            .is_some_and(|shown| shown.elapsed() >= READ_TIME)
    }

    fn unengaged_app(&self) -> Option<String> {
        let field = self.field.as_ref()?;
        (self.suggestion.is_some() && !self.engaged).then(|| field.app.clone())
    }

    fn note_ignored(&mut self, app: &str, out: &mut Vec<Action>) {
        if self.ignores.ignored(app, self.config.auto_pause_after) {
            out.push(Action::AutoPaused(self.ignores.paused()));
        }
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
            || field.placement.is_none()
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
        let suggestable = field.placement.is_some() && check(&field, &self.config).is_ok();
        self.dismissed.take_if(|(id, before)| {
            suggestable && *id == field.field_id && *before != field.before
        });
        let mut ignored = None;
        if let Some(previous) = &self.field {
            let same_field = previous.field_id == field.field_id
                && previous.app == field.app
                && self.eligible(&field).is_ok();
            if same_field && previous.before == field.before && previous.after == field.after {
                let moved = previous.placement != field.placement;
                let unblocked = self.eligible(previous).is_err()
                    || (previous.placement.is_none()
                        && self.suggestion.is_none()
                        && !self.in_flight);
                self.field = Some(field);
                if unblocked {
                    self.schedule(out);
                } else if moved {
                    self.show_current(out);
                }
                return;
            }
            if same_field && let Some(rest) = self.typed_through(previous, &field) {
                if rest.trim().is_empty() {
                    self.ignores.accepted(&field.app);
                    self.suggestion = None;
                    self.forced = None;
                    out.push(Action::Hide);
                    out.push(Action::DisarmKeys);
                    self.field = Some(field);
                    self.schedule(out);
                } else {
                    self.field = Some(field);
                    self.suggestion = Some(rest);
                    self.show_current(out);
                }
                return;
            }
            if same_field && previous.before != field.before && self.was_readable() {
                ignored = self.unengaged_app();
            }
        }
        self.reset(out);
        if let Some(app) = ignored {
            self.note_ignored(&app, out);
        }
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

    fn suppressed(&self, field: &FieldSnapshot) -> bool {
        let forced = self
            .forced
            .as_ref()
            .is_some_and(|(id, app)| *id == field.field_id && *app == field.app);
        self.eligible(field).is_err() || (!forced && self.ignores.is_paused(&field.app))
    }

    fn schedule(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        let dismissed = self
            .dismissed
            .as_ref()
            .is_some_and(|(id, before)| *id == field.field_id && *before == field.before);
        if dismissed
            || self.config.trigger_mode == TriggerMode::Manual
            || field.placement.is_none()
            || check(field, &self.config).is_err()
            || self.ignores.is_paused(&field.app)
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
        if field.placement.is_none() || self.suppressed(field) {
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
        if field.placement.is_none() {
            return;
        }
        self.suggestion = Some(text);
        self.counts.record_shown();
        self.engaged = false;
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
        self.engaged = true;
        let mut accepted = accepted;
        if let Some(field) = &self.field {
            self.ignores.accepted(&field.app);
            if self.config.smart_space && rest.is_empty() && closes_spaced_word(field, &accepted) {
                accepted.push(' ');
            }
        }
        self.shown_at = None;
        self.counts.record_accept(&accepted, one_word);
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
        match field.placement {
            Some(placement) => out.push(Action::Show {
                text: text.clone(),
                placement,
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
    use crate::{Rect, StopReason};

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
    fn counts_follow_shows_and_accepts_and_drain_once() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::Field(Some(field("Please sign the con"))));
        engine.handle(Event::AcceptWord);
        engine.handle(Event::AcceptAll);
        let counts = engine.take_counts();
        assert_eq!(counts.shown, 1);
        assert_eq!((counts.accepted, counts.accepted_words), (1, 1));
        assert_eq!(counts.words, 3);
        assert_eq!(counts.chars, "tract".len() as u64 + " by Friday.".len() as u64);
        assert!(engine.take_counts().is_empty());
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
        assert_eq!((counts.accepted, counts.accepted_words, counts.words), (0, 0, 0));
    }

    #[test]
    fn a_suggestion_redrawn_on_the_same_field_is_shown_once() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::Field(Some(field("Please sign the c"))));
        engine.handle(Event::Field(Some(field("Please sign the co"))));
        assert_eq!(engine.take_counts().shown, 1);
    }

    #[test]
    fn smart_space_counts_the_inserted_space() {
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
        moved.placement = Some(Placement::Inline(Rect {
            x: 50,
            y: 10,
            width: 1,
            height: 16,
        }));
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
            placement: None,
            ..field(before)
        }
    }

    #[test]
    fn never_infers_without_a_placement() {
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
        blind.placement = None;
        let out = engine.handle(Event::Field(Some(blind)));
        assert_eq!(out, vec![Action::Hide, Action::DisarmKeys]);
        assert!(engine.suggestion().is_none());
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
    fn a_field_without_a_trusted_caret_still_suggests_anchored() {
        let mut engine = Engine::new(Config::default());
        let out = engine.handle(Event::Field(Some(anchored("Please sign the"))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        engine.handle(Event::DebounceElapsed(generation));
        let out = engine.handle(Event::Completed {
            generation,
            result: Ok(completion(" contract")),
        });
        let expected = anchored("").placement;
        assert!(
            matches!(&out[0], Action::Show { placement, .. } if Some(*placement) == expected),
            "{out:?}"
        );
    }

    #[test]
    fn the_ghost_follows_a_placement_switch_between_inline_and_anchored() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let out = engine.handle(Event::Field(Some(anchored("Please sign the"))));
        let expected = anchored("").placement;
        assert!(
            matches!(out.as_slice(), [Action::Show { placement, .. }] if Some(*placement) == expected),
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
    fn new_config_applies_live_to_the_current_field() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
        let blocked = Config {
            blocked_apps: vec!["notepad".into()],
            ..Config::default()
        };
        let out = engine.handle(Event::SetConfig(blocked));
        assert_eq!(out, vec![Action::Hide, Action::DisarmKeys]);
        assert_eq!(engine.config().blocked_apps, ["notepad"]);
        let out = engine.handle(Event::SetConfig(Config::default()));
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
        let out = engine.handle(Event::Field(Some(in_app(app, before))));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        engine.handle(Event::DebounceElapsed(generation));
        engine.handle(Event::Completed {
            generation,
            result: Ok(completion(text)),
        });
        assert!(engine.suggestion().is_some());
    }

    fn paused_in(out: &[Action]) -> Option<&Vec<String>> {
        out.iter().find_map(|action| match action {
            Action::AutoPaused(apps) => Some(apps),
            _ => None,
        })
    }

    fn read_by_user(engine: &mut Engine) {
        engine.shown_at = engine.shown_at.map(|shown| shown - READ_TIME);
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
    fn dismissals_count_and_report_the_paused_app() {
        let mut engine = pausing(2);
        suggest_in(&mut engine, "Slack", "Please sign the", " contract");
        assert!(paused_in(&engine.handle(Event::Dismiss)).is_none());
        suggest_in(&mut engine, "Slack", "Please sign the d", " contract");
        let out = engine.handle(Event::Dismiss);
        assert_eq!(paused_in(&out), Some(&vec!["Slack".to_string()]));
    }

    #[test]
    fn accepting_resets_the_streak() {
        let mut engine = pausing(2);
        for _ in 0..4 {
            suggest_in(&mut engine, "notepad", "Please sign the", " contract");
            assert!(paused_in(&engine.handle(Event::Dismiss)).is_none());
            suggest_in(&mut engine, "notepad", "Please sign the d", " contract");
            engine.handle(Event::AcceptAll);
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
    fn pause_is_per_app() {
        let mut engine = pausing(1);
        suggest_in(&mut engine, "Slack", "Please sign the", " contract");
        engine.handle(Event::Dismiss);
        let out = engine.handle(Event::Field(Some(in_app("Slack", "Please sign thex"))));
        assert!(out.is_empty(), "{out:?}");
        let out = engine.handle(Event::Field(Some(in_app("notepad", "Please sign the"))));
        assert!(
            matches!(out.last(), Some(Action::ScheduleDebounce { .. })),
            "{out:?}"
        );
    }

    #[test]
    fn resume_restarts_suggesting_in_the_paused_app() {
        let mut engine = pausing(1);
        suggest_in(&mut engine, "Slack", "Please sign the", " contract");
        engine.handle(Event::Dismiss);
        engine.handle(Event::Field(Some(in_app("Slack", "Please sign thex"))));
        let out = engine.handle(Event::Resume("slack".into()));
        assert_eq!(paused_in(&out), Some(&Vec::new()));
        assert!(
            matches!(out.last(), Some(Action::ScheduleDebounce { .. })),
            "{out:?}"
        );
        assert!(engine.handle(Event::Resume("slack".into())).is_empty());
    }

    #[test]
    fn set_config_keeps_the_auto_paused_apps() {
        let mut engine = pausing(1);
        suggest_in(&mut engine, "Slack", "Please sign the", " contract");
        engine.handle(Event::Dismiss);
        let config = engine.config().clone();
        engine.handle(Event::SetConfig(config));
        let out = engine.handle(Event::Field(Some(in_app("Slack", "Please sign thex"))));
        assert!(out.is_empty(), "{out:?}");
        let out = engine.handle(Event::Resume("slack".into()));
        assert_eq!(paused_in(&out), Some(&Vec::new()));
    }

    #[test]
    fn zero_limit_disables_auto_pause() {
        let mut engine = pausing(0);
        for round in 0..30 {
            let before = format!("Please sign the {round}");
            suggest_in(&mut engine, "notepad", &before, " contract");
            let out = engine.handle(Event::Dismiss);
            assert!(paused_in(&out).is_none());
        }
    }

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
    fn trailing_space_is_off_by_default_and_never_doubles() {
        let off = accepted_insert(
            Config::default(),
            "Please sign the",
            " contract",
            Event::AcceptAll,
        );
        assert_eq!(off, " contract");
        let mut engine = Engine::new(spaced());
        let mut snapshot = field("Please sign the");
        snapshot.after = " ".into();
        let out = engine.handle(Event::Field(Some(snapshot)));
        let Some(Action::ScheduleDebounce { generation, .. }) = out.last().cloned() else {
            panic!("{out:?}")
        };
        engine.handle(Event::DebounceElapsed(generation));
        engine.handle(Event::Completed {
            generation,
            result: Ok(completion(" contract")),
        });
        let out = engine.handle(Event::AcceptAll);
        assert!(out.contains(&Action::Insert(" contract".into())), "{out:?}");
    }

    #[test]
    fn spaced_accept_lands_without_resetting_the_engine() {
        let mut engine = Engine::new(spaced());
        suggest(&mut engine, "Please sign the", " contract");
        engine.handle(Event::AcceptAll);
        let out = engine.handle(Event::Field(Some(field("Please sign the contract "))));
        assert!(
            !out.iter().any(|a| matches!(a, Action::CancelCompletion)),
            "{out:?}"
        );
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
                matches!(&out[1], Action::Show { text, .. } if text == rest),
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

    #[test]
    fn word_accept_works_on_right_to_left_text() {
        let mut engine = Engine::new(Config::default());
        let first = " \u{627}\u{644}\u{644}\u{647}";
        let rest = " \u{627}\u{644}\u{631}\u{62d}\u{645}\u{646}";
        suggest(
            &mut engine,
            "\u{628}\u{633}\u{645}",
            &format!("{first}{rest}"),
        );
        let out = engine.handle(Event::AcceptWord);
        assert_eq!(out[0], Action::Insert(first.into()));
        assert!(no_model_work(&out), "{out:?}");
        assert_eq!(engine.suggestion(), Some(rest));
    }

    fn racing_engine() -> Engine {
        Engine::new(Config {
            accept_race_ms: 1_000,
            ..Config::default()
        })
    }

    #[test]
    fn accept_right_after_the_suggestion_appears_is_refused() {
        let mut engine = racing_engine();
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        assert!(!engine.can_accept(Instant::now(), false));
        assert!(!engine.can_accept(Instant::now(), true));
        let later = Instant::now() + Duration::from_secs(5);
        assert!(engine.can_accept(later, false));
    }

    #[test]
    fn race_window_is_configurable_and_can_be_off() {
        let mut engine = Engine::new(Config {
            accept_race_ms: 0,
            ..Config::default()
        });
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        assert!(engine.can_accept(Instant::now(), false));
    }

    #[test]
    fn nothing_to_accept_without_a_suggestion() {
        let engine = Engine::new(Config::default());
        assert!(!engine.can_accept(Instant::now() + Duration::from_secs(5), false));
    }

    #[test]
    fn a_word_accept_ends_the_race_for_the_remainder() {
        let mut engine = racing_engine();
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::AcceptWord);
        assert!(engine.can_accept(Instant::now(), false));
    }

    #[test]
    fn typing_through_does_not_restart_the_race() {
        let mut engine = racing_engine();
        suggest(&mut engine, "Please sign the", " contract by Friday.");
        engine.handle(Event::Field(Some(field("Please sign the con"))));
        assert!(!engine.can_accept(Instant::now(), false));
        assert!(engine.can_accept(Instant::now() + Duration::from_secs(5), false));
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
        assert!(matches!(&out[0], Action::Show { .. }), "{out:?}");
        assert_eq!(out[1], Action::ArmKeys);
        for _ in 0..3 {
            let out = engine.handle(Event::Field(Some(field("Hi"))));
            assert!(out.is_empty(), "{out:?}");
        }
        assert_eq!(engine.suggestion(), Some(" there, how are you?"));
        let out = engine.handle(Event::Field(Some(field("Hi t"))));
        assert!(
            matches!(out.as_slice(), [Action::Show { text, .. }] if text == "here, how are you?"),
            "{out:?}"
        );
    }

    #[test]
    fn trigger_suggests_in_non_prose_lines() {
        let mut engine = Engine::new(Config::default());
        engine.handle(Event::Field(Some(field("4242 4242 4242"))));
        trigger_now(&mut engine);
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
            |f| f.app = "WindowsTerminal".into(),
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
    fn trigger_does_nothing_while_a_suggestion_is_visible_or_in_flight() {
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
    }

    #[test]
    fn trigger_overrides_a_dismiss() {
        let mut engine = Engine::new(Config::default());
        suggest(&mut engine, "Please sign the", " contract");
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
    fn forcing_ends_when_the_suggestion_is_used_up() {
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
    }

    #[test]
    fn forcing_ends_when_the_text_diverges_or_the_field_changes() {
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
    fn manual_mode_never_asks_on_its_own() {
        let mut engine = manual();
        for text in ["Please sign the", "Please sign the c", "Please sign the co"] {
            let out = engine.handle(Event::Field(Some(field(text))));
            assert!(out.is_empty(), "{out:?}");
        }
        assert!(engine.handle(Event::DebounceElapsed(0)).is_empty());
        let out = engine.handle(Event::SetEnabled(true));
        assert!(no_model_work(&out), "{out:?}");
        let out = engine.handle(Event::Field(None));
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn manual_mode_suggests_on_the_trigger_and_stays_manual_afterwards() {
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
}
