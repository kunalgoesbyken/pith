use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use pith_core::{Action, Completion, Drawn, Event, log};

use super::{App, InsertPolicy};
use crate::error::{InsertError, Inserted};
use crate::x11::Clipboard;

pub(super) struct Effects {
    policy: InsertPolicy,
    observer: Option<Sender<Action>>,
    clipboard: Option<Clipboard>,
    inserting: VecDeque<String>,
    unconfirmed: Vec<String>,
    started: Option<Instant>,
}

impl Effects {
    pub(super) fn new(
        policy: InsertPolicy,
        observer: Option<Sender<Action>>,
        clipboard: Option<Clipboard>,
    ) -> Self {
        Self {
            policy,
            observer,
            clipboard,
            inserting: VecDeque::new(),
            unconfirmed: Vec::new(),
            started: None,
        }
    }

    fn observe(&self, action: &Action) {
        if let Some(observer) = &self.observer {
            let _ = observer.send(action.clone());
        }
    }

    pub(super) fn inserting(&self) -> bool {
        !self.inserting.is_empty()
    }

    pub(super) fn inserted(&mut self, outcome: Result<Inserted, InsertError>) {
        let Some(text) = self.inserting.pop_front() else {
            return;
        };
        match outcome {
            Ok(Inserted::Confirmed) => {}
            Ok(Inserted::Unconfirmed) | Err(InsertError::TimedOut) => self.unconfirmed.push(text),
            Err(InsertError::NoField) => log("no field had the focus, nothing was inserted"),
            Err(error) => {
                log(&format!("insert through accessibility failed: {error}"));
                if self.policy.falls_back() {
                    self.paste(text);
                }
            }
        }
    }

    pub(super) fn verify(&mut self, before: Option<&str>) {
        for text in std::mem::take(&mut self.unconfirmed) {
            let missing = before.is_some_and(|before| !before.ends_with(&text));
            if missing && self.policy.falls_back() {
                self.paste(text);
            }
        }
    }

    fn paste(&self, text: String) {
        match &self.clipboard {
            Some(clipboard) => clipboard.paste(text),
            None => log("no clipboard to paste through"),
        }
    }

    pub(super) fn log_result(&mut self, result: &Result<Completion, String>) {
        match (self.started.take(), result) {
            (Some(started), Ok(completion)) => log(&format!(
                "completion {} ms, {} tokens, {} prompt tokens, logprob {}, {:?}",
                started.elapsed().as_millis(),
                completion.tokens,
                completion.prompt_tokens_evaluated,
                completion.logprob_label(),
                completion.stop
            )),
            (_, Err(error)) => log(&format!("completion error: {error}")),
            (None, Ok(_)) => {}
        }
    }
}

impl App {
    pub(super) fn dispatch(&mut self, event: Event) {
        let actions = self.engine.handle(event);
        self.apply(actions);
    }

    fn apply(&mut self, actions: Vec<Action>) {
        let mut drawn = None;
        for action in actions {
            if !matches!(action, Action::Show { .. }) {
                self.effects.observe(&action);
            }
            match action {
                Action::ScheduleDebounce {
                    generation,
                    delay_ms,
                } => {
                    self.timers
                        .schedule_debounce(Duration::from_millis(delay_ms), generation);
                }
                Action::StartCompletion {
                    generation,
                    request,
                } => {
                    self.effects.started = Some(Instant::now());
                    self.worker.submit(generation, request);
                }
                Action::CancelCompletion => self.worker.cancel(),
                Action::Show {
                    cuts,
                    placement,
                    style,
                } => {
                    let outcome = self.overlay.show(&cuts, placement, &style);
                    if let Drawn::Shown(index) = outcome
                        && let Some(cut) = cuts.into_iter().nth(index)
                    {
                        self.effects.observe(&Action::Show {
                            cuts: vec![cut],
                            placement,
                            style,
                        });
                    }
                    drawn = Some(outcome);
                }
                Action::CheckSpelling { .. }
                | Action::Offer { .. }
                | Action::Replace { .. }
                | Action::ArmReplaceTimeout { .. }
                | Action::Overwrite(_)
                | Action::Collapse { .. } => {}
                Action::Hide => self.overlay.hide(),
                Action::Insert(text) => self.insert(text),
                Action::AutoPaused(apps) => log(&format!("auto-paused in {apps:?}")),
                Action::ArmKeys => self.keyboard.keys.arm(),
                Action::DisarmKeys => self.keyboard.keys.disarm(),
            }
        }
        if let Some(drawn) = drawn {
            let actions = self.engine.handle(Event::Drawn(drawn));
            self.apply(actions);
        }
    }

    fn insert(&mut self, text: String) {
        if self.effects.policy.via_accessibility {
            self.effects.inserting.push_back(text.clone());
            self.reader.insert(text);
        } else {
            self.effects.paste(text);
        }
    }

    pub(super) fn debounce_elapsed(&mut self, generation: u64) {
        self.dispatch(Event::DebounceElapsed(generation));
    }
}
