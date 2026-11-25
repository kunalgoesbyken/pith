use std::collections::VecDeque;
use std::time::{Duration, Instant};

use pith_core::{Action, Completion, Drawn, Event, Placement, TextStyle, inserts_by_paste, log};

use super::{App, InsertPolicy};
use crate::error::{InsertError, Inserted, Target};
use crate::x11::{Clipboard, PasteTarget};

struct Inserting {
    text: String,
    target: Target,
    focus: Option<PasteTarget>,
    since: Instant,
}

pub(super) struct Effects {
    policy: InsertPolicy,
    #[cfg(feature = "e2e")]
    observer: Option<std::sync::mpsc::Sender<Action>>,
    clipboard: Option<Clipboard>,
    inserting: VecDeque<Inserting>,
    started: Option<Instant>,
}

impl Effects {
    pub(super) fn new(
        policy: InsertPolicy,
        #[cfg(feature = "e2e")] observer: Option<std::sync::mpsc::Sender<Action>>,
        clipboard: Option<Clipboard>,
    ) -> Self {
        Self {
            policy,
            #[cfg(feature = "e2e")]
            observer,
            clipboard,
            inserting: VecDeque::new(),
            started: None,
        }
    }

    #[cfg(feature = "e2e")]
    fn observe(&self, action: &Action) {
        if let Some(observer) = &self.observer {
            let _ = observer.send(action.clone());
        }
    }

    #[cfg(not(feature = "e2e"))]
    fn observe(&self, _action: &Action) {}

    #[cfg(feature = "e2e")]
    fn observe_shown(
        &self,
        outcome: Drawn,
        cuts: Vec<String>,
        placement: Placement,
        style: TextStyle,
    ) {
        if let Drawn::Shown(index) = outcome
            && let Some(cut) = cuts.into_iter().nth(index)
        {
            self.observe(&Action::Show {
                cuts: vec![cut],
                placement,
                style,
            });
        }
    }

    #[cfg(not(feature = "e2e"))]
    fn observe_shown(
        &self,
        _outcome: Drawn,
        _cuts: Vec<String>,
        _placement: Placement,
        _style: TextStyle,
    ) {
    }

    pub(super) fn inserting(&self, limit: Duration) -> bool {
        self.inserting
            .iter()
            .any(|pending| pending.since.elapsed() < limit)
    }

    pub(super) fn inserted(
        &mut self,
        outcome: Result<Inserted, InsertError>,
        current: Option<&Target>,
    ) {
        let Some(pending) = self.inserting.pop_front() else {
            return;
        };
        match outcome {
            Ok(Inserted::Confirmed) => {}
            Ok(Inserted::Unconfirmed) | Err(InsertError::TimedOut) => {
                log("the field did not confirm the inserted text, so nothing more is inserted");
            }
            Err(InsertError::NoField) => log("the focus moved, nothing was inserted"),
            Err(error) => {
                log(&format!("insert through accessibility failed: {error}"));
                if self.policy.falls_back() && current == Some(&pending.target) {
                    self.paste(pending.text, pending.focus, pending.target);
                }
            }
        }
    }

    fn paste(&self, text: String, focus: Option<PasteTarget>, target: Target) {
        match (&self.clipboard, focus) {
            (Some(clipboard), Some(focus)) => clipboard.paste(text, focus, target),
            _ => log("no clipboard or no focused window to paste into"),
        }
    }

    pub(super) fn finish(&mut self) {
        if let Some(clipboard) = self.clipboard.take() {
            clipboard.finish();
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
        self.shell.skip = self.engine.skip();
        self.refresh();
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
                    self.effects.observe_shown(outcome, cuts, placement, style);
                    drawn = Some(outcome);
                }
                Action::CheckSpelling { .. }
                | Action::Offer { .. }
                | Action::Replace { .. }
                | Action::ArmReplaceTimeout { .. }
                | Action::Overwrite(_)
                | Action::Collapse { .. } => {}
                Action::Learn(sample) => self.learn(sample),
                Action::Hide => self.overlay.hide(),
                Action::Insert(text) => self.insert(text),
                Action::AutoPaused(apps) => {
                    log(&format!("auto-paused in {apps:?}"));
                    self.shell.auto_paused = apps;
                }
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
        let Some(target) = self.current.get() else {
            log("no field is known, nothing was inserted");
            return;
        };
        let focus = self.effects.clipboard.as_ref().and_then(Clipboard::focus);
        if self.effects.policy.via_accessibility() && !inserts_by_paste(&target.app) {
            self.effects.inserting.push_back(Inserting {
                text: text.clone(),
                target: target.clone(),
                focus,
                since: Instant::now(),
            });
            self.reader.insert(text, target);
        } else {
            self.effects.paste(text, focus, target);
        }
    }

    pub(super) fn debounce_elapsed(&mut self, generation: u64) {
        self.dispatch(Event::DebounceElapsed(generation));
    }
}
