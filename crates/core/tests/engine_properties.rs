mod common;

use common::{Rng, fragment, is_boundary, text};
use pith_core::{
    Action, Completion, Config, Engine, Event, FieldSnapshot, LINE_BREAKS, Placement, Rect,
    StopReason, check,
};
use unicode_segmentation::UnicodeSegmentation;

const SEEDS: u64 = 300;
const STEPS: usize = 300;

const WORDS: &[&str] = &[
    "Please ",
    "sign ",
    "the ",
    "contract ",
    "by ",
    "Friday",
    "नमस्ते ",
    "दोस्त ",
    "你好世界",
    "hello ",
    "مرحبا ",
    "שלום ",
    "e\u{301}cole ",
    "👨\u{200d}👩\u{200d}👧 ",
    "tomorrow ",
    ". ",
];

const COMPLETIONS: &[&str] = &[
    " contract by Friday.",
    " world and more words here",
    "act",
    " 世界和平",
    " दोस्त कल आऊँगा।",
    "\u{301}x",
    " \u{301}y",
    "  ",
    ".",
    " ok\nnext line",
    "\r\n",
    "",
];

const LOGPROBS: &[Option<f32>] = &[
    Some(-0.5),
    Some(0.0),
    Some(-5.0),
    Some(f32::NEG_INFINITY),
    Some(f32::NAN),
    Some(-1.6),
    None,
];

const STOPS: &[StopReason] = &[
    StopReason::Newline,
    StopReason::Sentence,
    StopReason::MaxTokens,
    StopReason::EndOfText,
    StopReason::LowConfidence,
    StopReason::Cancelled,
];

const APPS: &[&str] = &["notepad", "WINWORD.EXE", "Code", "C:\\x\\mail.exe"];

fn placement(rng: &mut Rng) -> Placement {
    let rect = Rect {
        x: rng.below(200) as i32,
        y: rng.below(200) as i32,
        width: 1,
        height: 16,
    };
    if rng.chance(50) {
        Placement::Inline(rect)
    } else {
        Placement::Anchored(rect)
    }
}

fn prose(rng: &mut Rng) -> String {
    if rng.chance(70) {
        rng.pick(WORDS).to_string()
    } else {
        fragment(rng).to_string()
    }
}

fn completion(rng: &mut Rng) -> Completion {
    let body = if rng.chance(60) {
        rng.pick(COMPLETIONS).to_string()
    } else {
        text(rng, 8)
    };
    Completion {
        text: body,
        mean_logprob: *rng.pick(LOGPROBS),
        tokens: rng.below(20),
        prompt_tokens_evaluated: rng.below(100),
        stop: *rng.pick(STOPS),
    }
}

fn context_key(field: &Option<FieldSnapshot>) -> Option<(u64, String, String, String, [bool; 3])> {
    field.as_ref().map(|f| {
        (
            f.field_id,
            f.app.clone(),
            f.before.clone(),
            f.after.text().to_string(),
            [f.has_selection, f.is_password, f.composing],
        )
    })
}

struct Harness {
    engine: Engine,
    rng: Rng,
    config: Config,
    world: Option<FieldSnapshot>,
    sent: Option<FieldSnapshot>,
    scheduled: Vec<u64>,
    sched: Option<u64>,
    last_sched: u64,
    last_start: u64,
    flight: Option<u64>,
    live: Option<u64>,
    shown: Option<String>,
    armed: bool,
    dismissed: Option<(u64, String)>,
    trace: Vec<String>,
    stats: Stats,
}

#[derive(Default)]
struct Stats {
    shows: usize,
    inserts: usize,
    starts: usize,
    cancels: usize,
    stale: usize,
    dismissals: usize,
}

impl Stats {
    fn add(&mut self, other: &Stats) {
        self.shows += other.shows;
        self.inserts += other.inserts;
        self.starts += other.starts;
        self.cancels += other.cancels;
        self.stale += other.stale;
        self.dismissals += other.dismissals;
    }
}

impl Harness {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let config = Config {
            max_words: 1 + rng.below(8),
            min_chars_before: rng.below(6),
            ..Config::default()
        };
        let world = FieldSnapshot {
            field_id: 1,
            app: "notepad".into(),
            before: "Please sign the".into(),
            placement: Some(placement(&mut rng)),
            ..Default::default()
        };
        Self {
            engine: Engine::new(config.clone()),
            rng,
            config,
            world: Some(world),
            sent: None,
            scheduled: Vec::new(),
            sched: None,
            last_sched: 0,
            last_start: 0,
            flight: None,
            live: None,
            shown: None,
            armed: false,
            dismissed: None,
            trace: Vec::new(),
            stats: Stats::default(),
        }
    }

    fn fail(&self, message: &str) -> ! {
        let tail = self.trace.len().saturating_sub(12);
        panic!("{message}\nlast events:\n{}", self.trace[tail..].join("\n"));
    }

    fn expect(&self, ok: bool, message: &str) {
        if !ok {
            self.fail(message);
        }
    }

    fn send(&mut self, event: Event) -> Vec<Action> {
        self.trace.push(format!("{event:?}"));
        let before_key = context_key(&self.sent);
        let pre = self.engine.suggestion().map(str::to_string);
        let accepting = matches!(event, Event::AcceptAll | Event::AcceptWord);
        match &event {
            Event::Field(field) => self.sent = field.clone(),
            Event::SetConfig(config) => self.config = config.clone(),
            _ => {}
        }
        let actions = self.engine.handle(event.clone());
        self.trace.push(format!("  -> {actions:?}"));
        let mut scheduled_now = false;
        let mut inserted = None;
        for action in &actions {
            match action {
                Action::ScheduleDebounce { generation, .. } => {
                    self.expect(*generation >= self.last_sched, "generation went backwards");
                    self.last_sched = *generation;
                    self.sched = Some(*generation);
                    self.scheduled.push(*generation);
                    scheduled_now = true;
                }
                Action::StartCompletion {
                    generation,
                    request,
                } => {
                    self.expect(self.flight.is_none(), "start while one is in flight");
                    self.expect(
                        self.sched == Some(*generation),
                        "start from a stale generation",
                    );
                    self.expect(
                        *generation >= self.last_start,
                        "start generation went backwards",
                    );
                    let field = self.sent.as_ref();
                    self.expect(
                        field.is_some_and(|f| check(f, &self.config).is_ok()),
                        "start for a field policy rejects",
                    );
                    self.expect(
                        field.is_some_and(|f| f.placement.is_some()),
                        "start without a caret",
                    );
                    self.expect(
                        field.is_some_and(|f| f.before.ends_with(&request.forced_prefix)),
                        "forced prefix is not the end of the text",
                    );
                    self.stats.starts += 1;
                    self.last_start = *generation;
                    self.flight = Some(*generation);
                    self.live = Some(*generation);
                }
                Action::CancelCompletion => {
                    self.expect(self.flight.is_some(), "cancel with nothing in flight");
                    self.stats.cancels += 1;
                    self.flight = None;
                    self.live = None;
                }
                Action::Show { text, .. } => {
                    self.expect(!text.trim().is_empty(), "blank suggestion shown");
                    self.expect(!text.contains(LINE_BREAKS), "multiline suggestion shown");
                    self.expect(
                        self.sent.as_ref().is_some_and(|f| {
                            f.placement.is_some() && check(f, &self.config).is_ok()
                        }),
                        "show for a field that is not eligible",
                    );
                    self.stats.shows += 1;
                    self.shown = Some(text.clone());
                }
                Action::Hide => {
                    self.expect(self.shown.is_some(), "hide with nothing shown");
                    self.shown = None;
                }
                Action::ArmKeys => {
                    self.expect(!self.armed, "armed twice");
                    self.expect(self.shown.is_some(), "armed without a suggestion");
                    self.armed = true;
                }
                Action::DisarmKeys => {
                    self.expect(self.armed, "disarmed while not armed");
                    self.expect(self.shown.is_none(), "disarmed before hide");
                    self.armed = false;
                }
                Action::AutoPaused(_) => {}
                Action::Insert(inserted_text) => {
                    self.expect(
                        self.shown.is_some() && self.armed,
                        "insert without armed suggestion",
                    );
                    self.expect(accepting, "insert without an accept event");
                    self.expect(!inserted_text.is_empty(), "empty insert");
                    self.stats.inserts += 1;
                    inserted = Some(inserted_text.clone());
                }
            }
        }
        if let (Some(inserted), Some(field)) = (&inserted, &mut self.sent) {
            field.before.push_str(inserted);
        }
        self.expect(
            self.shown.as_deref() == self.engine.suggestion(),
            "shown text differs from engine suggestion",
        );
        self.expect(
            self.armed == self.shown.is_some(),
            "armed state differs from shown state",
        );
        if let (Some(inserted), Some(pre)) = (&inserted, &pre) {
            self.expect(
                pre.starts_with(inserted.as_str()),
                "inserted text is not a prefix of the suggestion",
            );
            self.expect(is_boundary(pre, inserted.len()), "insert cuts a grapheme");
            let rest = &pre[inserted.len()..];
            let expected = (!rest.trim().is_empty()
                && self.sent.as_ref().is_some_and(|f| f.placement.is_some()))
            .then_some(rest);
            self.expect(
                self.engine.suggestion() == expected,
                "remainder after insert is wrong",
            );
        }
        if accepting && pre.is_none() {
            self.expect(actions.is_empty(), "accept without suggestion acted");
        }
        match &event {
            Event::Completed { generation, .. } => {
                if self.live != Some(*generation) {
                    self.stats.stale += 1;
                    self.expect(actions.is_empty(), "stale completion had an effect");
                } else {
                    self.flight = None;
                    self.live = None;
                }
            }
            Event::Field(Some(_))
            | Event::Dismiss
            | Event::SetConfig(_)
            | Event::AcceptAll
            | Event::AcceptWord
            | Event::Field(None) => {
                let changed = match &event {
                    Event::Field(Some(_)) => before_key != context_key(&self.sent),
                    Event::AcceptAll | Event::AcceptWord => pre.is_some(),
                    _ => true,
                };
                if changed && !scheduled_now {
                    self.sched = None;
                }
            }
            Event::DebounceElapsed(_) | Event::Resume(_) | Event::Trigger | Event::Drawn(_) => {}
        }
        self.check_dismissal(&event, scheduled_now);
        if matches!(event, Event::Field(None)) {
            self.expect(
                self.shown.is_none() && self.flight.is_none(),
                "focus loss left state behind",
            );
        }
        actions
    }

    fn check_dismissal(&mut self, event: &Event, scheduled: bool) {
        if matches!(event, Event::Dismiss) {
            self.stats.dismissals += 1;
            self.dismissed = self.sent.as_ref().map(|f| (f.field_id, f.before.clone()));
            return;
        }
        let Some((id, before)) = self.dismissed.clone() else {
            return;
        };
        let Some(sent) = &self.sent else { return };
        if sent.field_id != id {
            return;
        }
        if sent.before != before && matches!(event, Event::Field(Some(_))) {
            self.dismissed = None;
            return;
        }
        if sent.before == before {
            self.expect(!scheduled, "scheduled again for dismissed text");
        }
    }

    fn run_step(&mut self) {
        let roll = self.rng.below(100);
        match roll {
            0..=11 => self.type_text(),
            12..=14 => self.backspace(),
            15..=16 => self.replace_text(),
            17..=18 => self.switch_field(),
            19..=21 => self.toggle_placement(),
            22..=23 => self.toggle_flag(),
            24..=25 => self.set_after(),
            26..=33 => self.type_through(),
            34..=37 => self.resend(),
            38 => {
                self.send(Event::Field(None));
            }
            39..=44 => self.debounce(),
            45..=51 => self.complete(false),
            52..=76 => self.settle(),
            77..=86 => self.accept(),
            87..=89 => {
                self.send(Event::Dismiss);
            }
            90..=91 => {
                let config = Config {
                    enabled: self.rng.chance(60),
                    min_chars_before: if self.rng.chance(50) { 3 } else { 6 },
                    ..self.config.clone()
                };
                self.send(Event::SetConfig(config));
            }
            _ => self.complete(false),
        }
    }

    fn settle(&mut self) {
        self.debounce();
        if self.flight.is_some() {
            self.complete(true);
        }
    }

    fn publish(&mut self) {
        let snapshot = self.world.clone();
        self.send(Event::Field(snapshot));
    }

    fn type_text(&mut self) {
        let piece = prose(&mut self.rng);
        if let Some(field) = &mut self.world {
            field.before.push_str(&piece);
        }
        self.publish();
    }

    fn backspace(&mut self) {
        if let Some(field) = &mut self.world
            && let Some((start, _)) = field.before.grapheme_indices(true).next_back()
        {
            field.before.truncate(start);
        }
        self.publish();
    }

    fn replace_text(&mut self) {
        let replacement = text(&mut self.rng, 12);
        if let Some(field) = &mut self.world {
            field.before = replacement;
        }
        self.publish();
    }

    fn switch_field(&mut self) {
        let id = 1 + self.rng.below(3) as u64;
        let app = self.rng.pick(APPS).to_string();
        let before = if self.rng.chance(50) {
            "Please sign the".to_string()
        } else {
            text(&mut self.rng, 8)
        };
        let placement = Some(placement(&mut self.rng));
        self.world = Some(FieldSnapshot {
            field_id: id,
            app,
            before,
            placement,
            ..Default::default()
        });
        self.publish();
    }

    fn toggle_placement(&mut self) {
        let placement = self.rng.chance(50).then(|| placement(&mut self.rng));
        if let Some(field) = &mut self.world {
            field.placement = placement;
        }
        self.publish();
    }

    fn toggle_flag(&mut self) {
        let which = self.rng.below(3);
        if let Some(field) = &mut self.world {
            match which {
                0 => field.has_selection = !field.has_selection,
                1 => field.is_password = !field.is_password,
                _ => field.composing = !field.composing,
            }
        }
        self.publish();
    }

    fn set_after(&mut self) {
        let after = text(&mut self.rng, 3);
        if let Some(field) = &mut self.world {
            field.after = after.into();
        }
        self.publish();
    }

    fn resend(&mut self) {
        self.publish();
    }

    fn type_through(&mut self) {
        let (Some(suggestion), Some(sent)) = (
            self.engine.suggestion().map(str::to_string),
            self.world.clone(),
        ) else {
            return self.type_text();
        };
        let graphemes = suggestion.graphemes(true).count();
        let take = 1 + self.rng.below(graphemes);
        let typed: String = suggestion.graphemes(true).take(take).collect();
        let mut next = sent.clone();
        next.before.push_str(&typed);
        self.world = Some(next.clone());
        self.send(Event::Field(Some(next.clone())));
        let rest = &suggestion[typed.len()..];
        let eligible = check(&next, &self.config).is_ok() && next.placement.is_some();
        let expected = (eligible && !rest.trim().is_empty()).then_some(rest);
        self.expect(
            self.engine.suggestion() == expected,
            "typing through the suggestion did not shrink it",
        );
    }

    fn debounce(&mut self) {
        let Some(latest) = self.scheduled.last().copied() else {
            return;
        };
        let generation = if self.rng.chance(75) {
            latest
        } else {
            *self.rng.pick(&self.scheduled)
        };
        self.send(Event::DebounceElapsed(generation));
    }

    fn complete(&mut self, sane: bool) {
        let generation = match self.flight {
            Some(flight) if self.rng.chance(85) => flight,
            _ => self.last_start.saturating_sub(self.rng.below(3) as u64),
        };
        let result = if sane {
            Ok(Completion {
                text: self.rng.pick(COMPLETIONS).to_string(),
                mean_logprob: Some(-0.5),
                tokens: 4,
                prompt_tokens_evaluated: 4,
                stop: *self.rng.pick(&STOPS[..5]),
            })
        } else if self.rng.chance(10) {
            Err("boom".to_string())
        } else {
            Ok(completion(&mut self.rng))
        };
        let shown_before = self.shown.clone();
        let live = self.live == Some(generation);
        let before = self.sent.as_ref().map(|f| f.before.clone());
        let words = self.config.max_words;
        let actions = self.send(Event::Completed {
            generation,
            result: result.clone(),
        });
        if !live {
            return;
        }
        let Some(Action::Show { text: shown, .. }) = actions.first() else {
            return;
        };
        let Ok(done) = result else {
            self.fail("error result was shown");
        };
        self.expect(
            shown_before.is_none(),
            "completion replaced a visible suggestion",
        );
        self.expect(
            done.stop != StopReason::Cancelled,
            "cancelled result was shown",
        );
        self.expect(
            done.mean_logprob
                .is_none_or(|mean| mean >= self.config.min_mean_logprob),
            "weak result was shown",
        );
        self.expect(
            done.text.trim_start().starts_with(shown.trim_start()),
            "shown text is not a prefix of the completion",
        );
        let count = shown
            .split_word_bounds()
            .filter(|s| s.chars().any(char::is_alphanumeric))
            .count();
        self.expect(count <= words, "more words than max_words");
        if before.is_some_and(|b| b.ends_with(char::is_whitespace)) {
            self.expect(!shown.starts_with(char::is_whitespace), "doubled space");
        }
    }

    fn accept(&mut self) {
        let event = if self.rng.chance(50) {
            Event::AcceptAll
        } else {
            Event::AcceptWord
        };
        let actions = self.send(event);
        if matches!(actions.first(), Some(Action::Insert(_))) {
            self.world = self.sent.clone();
            if self.rng.chance(60) {
                self.publish();
            }
        }
    }
}

#[test]
fn engine_invariants_hold_over_random_event_sequences() {
    let mut totals = Stats::default();
    for seed in 0..SEEDS {
        let mut harness = Harness::new(seed);
        harness.publish();
        for _ in 0..STEPS {
            harness.run_step();
        }
        totals.add(&harness.stats);
    }
    assert!(totals.shows > 1500, "{}", totals.shows);
    assert!(totals.inserts > 300, "{}", totals.inserts);
    assert!(totals.starts > 2000, "{}", totals.starts);
    assert!(totals.cancels > 250, "{}", totals.cancels);
    assert!(totals.stale > 5000, "{}", totals.stale);
    assert!(totals.dismissals > 1000, "{}", totals.dismissals);
}
