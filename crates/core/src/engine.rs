use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use crate::ignore::IgnoreTracker;
use crate::stats::Counts;
use crate::suffix::candidates;
use crate::{
    Completion, CompletionRequest, Config, Drawn, FieldSnapshot, Placement, SkipReason, Suffix,
    TextStyle, TriggerMode, build, check, check_forced, continue_window, ends_spaced_word,
    next_word, suggestion,
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
    Drawn(Drawn),
    Trigger,
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
        cuts: Vec<String>,
        placement: Placement,
        style: TextStyle,
    },
    Hide,
    Insert(String),
    ArmKeys,
    DisarmKeys,
    AutoPaused(Vec<String>),
}

fn closes_spaced_word(field: &FieldSnapshot, accepted: &str) -> bool {
    field.after.text().is_empty() && ends_spaced_word(&format!("{}{accepted}", field.before))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldKey {
    id: u64,
    app: String,
}

impl FieldKey {
    fn of(field: &FieldSnapshot) -> Self {
        Self {
            id: field.field_id,
            app: field.app.clone(),
        }
    }

    fn matches(&self, field: &FieldSnapshot) -> bool {
        self.id == field.field_id && self.app == field.app
    }
}

enum Release {
    Edit,
    EditOrMove(Option<Placement>),
}

struct Dismissal {
    field: FieldKey,
    before: String,
    release: Release,
}

impl Dismissal {
    fn of(field: &FieldSnapshot, release: Release) -> Self {
        Self {
            field: FieldKey::of(field),
            before: field.before.clone(),
            release,
        }
    }

    fn holds(&self, field: &FieldSnapshot) -> bool {
        self.field.matches(field)
            && self.before == field.before
            && match self.release {
                Release::Edit => true,
                Release::EditOrMove(placement) => placement == field.placement,
            }
    }
}

struct Shown {
    text: String,
    cuts: Vec<String>,
    shown_at: Option<Instant>,
    engaged: bool,
    counted: bool,
}

enum Phase {
    Idle,
    Requested,
    Showing(Shown),
}

pub struct Engine {
    config: Config,
    field: Option<FieldSnapshot>,
    generation: u64,
    phase: Phase,
    dismissed: Option<Dismissal>,
    ignores: IgnoreTracker,
    forced: Option<FieldKey>,
    unlanded: Vec<String>,
    counts: Counts,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            field: None,
            generation: 0,
            phase: Phase::Idle,
            dismissed: None,
            ignores: IgnoreTracker::default(),
            forced: None,
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
        match &self.phase {
            Phase::Showing(shown) => Some(&shown.text),
            Phase::Idle | Phase::Requested => None,
        }
    }

    fn is_idle(&self) -> bool {
        matches!(self.phase, Phase::Idle)
    }

    pub fn can_accept(&self, now: Instant, moves_caret: bool) -> bool {
        let (Some(field), Phase::Showing(shown)) = (&self.field, &self.phase) else {
            return false;
        };
        let racing = shown.shown_at.is_some_and(|at| {
            now.saturating_duration_since(at) < Duration::from_millis(self.config.accept_race_ms)
        });
        !racing && (!moves_caret || field.after.ends_text())
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
                    .map(|field| Dismissal::of(field, Release::Edit));
            }
            Event::Drawn(drawn) => self.on_drawn(drawn, &mut out),
            Event::Trigger => self.on_trigger(&mut out),
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
        matches!(&self.phase, Phase::Showing(shown) if shown.shown_at.is_some_and(|at| at.elapsed() >= READ_TIME))
    }

    fn unengaged_app(&self) -> Option<String> {
        let field = self.field.as_ref()?;
        matches!(&self.phase, Phase::Showing(shown) if !shown.engaged).then(|| field.app.clone())
    }

    fn note_ignored(&mut self, app: &str, out: &mut Vec<Action>) {
        if self.ignores.ignored(app, self.config.auto_pause_after) {
            out.push(Action::AutoPaused(self.ignores.paused()));
        }
    }

    fn withdraw(&mut self, out: &mut Vec<Action>) {
        self.phase = Phase::Idle;
        out.push(Action::Hide);
        out.push(Action::DisarmKeys);
    }

    fn reset(&mut self, out: &mut Vec<Action>) {
        match self.phase {
            Phase::Idle => {}
            Phase::Requested => {
                self.phase = Phase::Idle;
                out.push(Action::CancelCompletion);
            }
            Phase::Showing(_) => self.withdraw(out),
        }
        self.forced = None;
        self.unlanded.clear();
        self.generation += 1;
    }

    fn is_forced(&self, field: &FieldSnapshot) -> bool {
        self.forced.as_ref().is_some_and(|key| key.matches(field))
    }

    fn eligible(&self, field: &FieldSnapshot) -> Result<(), SkipReason> {
        if self.is_forced(field) {
            check_forced(field, &self.config)
        } else {
            check(field, &self.config)
        }
    }

    fn on_trigger(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        if !self.is_idle()
            || field.placement.is_none()
            || check_forced(field, &self.config).is_err()
        {
            return;
        }
        let forced = FieldKey::of(field);
        self.dismissed = None;
        self.reset(out);
        self.forced = Some(forced);
        self.phase = Phase::Requested;
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
            .filter(|previous| FieldKey::of(previous).matches(field))
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
            && FieldKey::of(previous).matches(&field)
        {
            field.before = continue_window(
                &previous.before,
                &field.before,
                self.config.lookback_chars(),
            );
        }
        let suggestable = field.placement.is_some() && check(&field, &self.config).is_ok();
        let released = self
            .dismissed
            .take_if(|dismissal| {
                suggestable && dismissal.field.matches(&field) && !dismissal.holds(&field)
            })
            .is_some();
        let mut ignored = None;
        if let Some(previous) = &self.field {
            let same_field =
                FieldKey::of(previous).matches(&field) && self.eligible(&field).is_ok();
            if same_field && previous.before == field.before && previous.after == field.after {
                let moved = previous.placement != field.placement;
                let unblocked = released
                    || self.eligible(previous).is_err()
                    || (previous.placement.is_none() && self.is_idle());
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
                    self.forced = None;
                    self.withdraw(out);
                    self.field = Some(field);
                    self.schedule(out);
                } else {
                    self.field = Some(field);
                    if let Phase::Showing(shown) = &mut self.phase {
                        shown.text = rest;
                    }
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
        let suggestion = self.suggestion()?;
        let typed = field.before.strip_prefix(previous.before.as_str())?;
        if typed.is_empty() || previous.after != field.after {
            return None;
        }
        let rest = suggestion.strip_prefix(typed)?;
        is_grapheme_boundary(suggestion, typed.len()).then(|| rest.to_string())
    }

    fn suppressed(&self, field: &FieldSnapshot) -> bool {
        self.eligible(field).is_err()
            || (!self.is_forced(field) && self.ignores.is_paused(&field.app))
    }

    fn schedule(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        let dismissed = self
            .dismissed
            .as_ref()
            .is_some_and(|dismissal| dismissal.holds(field));
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
        if generation != self.generation || !self.is_idle() {
            return;
        }
        let Some(field) = &self.field else { return };
        if field.placement.is_none() || self.suppressed(field) {
            return;
        }
        self.phase = Phase::Requested;
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
        if generation != self.generation || !matches!(self.phase, Phase::Requested) {
            return;
        }
        self.phase = Phase::Idle;
        let Ok(completion) = result else { return };
        let Some(field) = &self.field else { return };
        let Some(text) = suggestion(&completion, &field.before, &field.after, &self.config) else {
            return;
        };
        if field.placement.is_none() {
            return;
        }
        self.phase = Phase::Showing(Shown {
            text,
            cuts: Vec::new(),
            shown_at: Some(Instant::now()),
            engaged: false,
            counted: false,
        });
        self.show_current(out);
        out.push(Action::ArmKeys);
    }

    fn on_accept(&mut self, one_word: bool, out: &mut Vec<Action>) {
        let Phase::Showing(shown) = &mut self.phase else {
            return;
        };
        let suggestion = std::mem::take(&mut shown.text);
        self.phase = Phase::Idle;
        let accepted = if one_word {
            next_word(&suggestion).to_string()
        } else {
            suggestion.clone()
        };
        let rest = suggestion[accepted.len()..].to_string();
        let mut accepted = accepted;
        if let Some(field) = &self.field {
            self.ignores.accepted(&field.app);
            if self.config.smart_space && rest.is_empty() && closes_spaced_word(field, &accepted) {
                accepted.push(' ');
            }
        }
        self.counts.record_accept(&accepted, one_word);
        out.push(Action::Insert(accepted.clone()));
        if let Some(field) = &mut self.field {
            field.before.push_str(&accepted);
        }
        self.unlanded.push(accepted);
        if rest.trim().is_empty() {
            self.forced = None;
            self.withdraw(out);
            self.generation += 1;
            self.schedule(out);
        } else {
            self.phase = Phase::Showing(Shown {
                text: rest,
                cuts: Vec::new(),
                shown_at: None,
                engaged: true,
                counted: true,
            });
            self.show_current(out);
        }
    }

    fn on_drawn(&mut self, drawn: Drawn, out: &mut Vec<Action>) {
        let Phase::Showing(shown) = &mut self.phase else {
            return;
        };
        let Drawn::Shown(index) = drawn else {
            return self.give_up(out);
        };
        let Some(text) = shown.cuts.get(index) else {
            return self.give_up(out);
        };
        shown.text = text.clone();
        if !std::mem::replace(&mut shown.counted, true) {
            self.counts.record_shown();
        }
    }

    pub fn crowded(&self) -> bool {
        self.field.as_ref().is_some_and(|field| {
            self.dismissed.as_ref().is_some_and(|dismissal| {
                matches!(dismissal.release, Release::EditOrMove(_)) && dismissal.holds(field)
            })
        })
    }

    fn give_up(&mut self, out: &mut Vec<Action>) {
        self.withdraw(out);
        self.forced = None;
        self.dismissed = self
            .field
            .as_ref()
            .map(|field| Dismissal::of(field, Release::EditOrMove(field.placement)));
    }

    fn show_current(&mut self, out: &mut Vec<Action>) {
        let (Phase::Showing(shown), Some(field)) = (&mut self.phase, &self.field) else {
            return;
        };
        let Some(placement) = field.placement else {
            self.withdraw(out);
            return;
        };
        let cuts = candidates(&shown.text, &field.after);
        if cuts.is_empty() {
            return self.give_up(out);
        }
        let placement = match placement {
            Placement::Inline(caret) if !matches!(field.after.suffix(), Suffix::Blank) => {
                Placement::BelowCaret(caret)
            }
            placement => placement,
        };
        shown.cuts.clone_from(&cuts);
        out.push(Action::Show {
            cuts,
            placement,
            style: field.style.clone(),
        });
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

    fn blind(before: &str) -> FieldSnapshot {
        FieldSnapshot {
            placement: None,
            ..field(before)
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
}
