use std::sync::mpsc::Sender;
use std::time::Instant;

use pith_core::{Action, Chord, Completion, Correction, Drawn, Event, log};

use super::{App, Inbound, InsertMode, TIMER_DEBOUNCE, TIMER_REPLACE};
use crate::clipboard::{Trigger, paste};
use crate::reader::Selection;

pub struct Effects {
    insert: InsertMode,
    observer: Option<Sender<Action>>,
    pending_debounce: u64,
    pending_replace: u64,
    started: Option<Instant>,
}

impl Effects {
    pub fn new(insert: InsertMode, observer: Option<Sender<Action>>) -> Self {
        Self {
            insert,
            observer,
            pending_debounce: 0,
            pending_replace: 0,
            started: None,
        }
    }

    fn observe(&self, action: &Action) {
        if let Some(observer) = &self.observer {
            let _ = observer.send(action.clone());
        }
    }

    pub fn insert(&self, text: String) {
        match &self.insert {
            InsertMode::Paste => paste(text, Trigger::Chord),
            InsertMode::Custom(insert) => insert(text),
        }
    }

    pub fn log_result(&mut self, result: &Result<Completion, String>) {
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
        self.presenter.set_skip(self.engine.skip());
        self.refresh();
    }

    fn apply(&mut self, actions: Vec<Action>) {
        let mut drawn = None;
        for action in actions {
            if !matches!(action, Action::Show { .. } | Action::Offer { .. }) {
                self.effects.observe(&action);
            }
            match action {
                Action::ScheduleDebounce {
                    generation,
                    delay_ms,
                } => {
                    self.effects.pending_debounce = generation;
                    self.set_timer(TIMER_DEBOUNCE, delay_ms as u32);
                }
                Action::StartCompletion {
                    generation,
                    request,
                } => {
                    self.effects.started = Some(Instant::now());
                    self.worker.submit(generation, request);
                }
                Action::CancelCompletion => self.worker.cancel(),
                Action::CheckSpelling {
                    generation,
                    language,
                    word,
                } => {
                    if let Some(speller) = &self.speller {
                        speller.check(generation, language, word);
                    }
                }
                Action::Offer {
                    correction,
                    placement,
                } => {
                    let outcome = self.overlay.offer(&correction.label(), placement);
                    if matches!(outcome, Drawn::Shown(_)) {
                        self.effects.observe(&Action::Offer {
                            correction,
                            placement,
                        });
                    }
                    drawn = Some(outcome);
                }
                Action::Replace {
                    id,
                    field_id,
                    correction,
                } => self.replace(id, field_id, correction),
                Action::ArmReplaceTimeout { id, delay_ms } => {
                    self.effects.pending_replace = id;
                    self.set_timer(TIMER_REPLACE, delay_ms as u32);
                }
                Action::Overwrite(text) => self.effects.insert(text),
                Action::Collapse { field_id, text } => self.reader.collapse(field_id, text),
                Action::Show {
                    cuts,
                    placement,
                    style,
                } => {
                    let hint = self.key_hint();
                    let outcome = self.overlay.show(&cuts, placement, &style, hint.as_deref());
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
                Action::Hide => self.overlay.hide(),
                Action::Insert(text) => self.effects.insert(text),
                Action::AutoPaused(apps) => {
                    log(&format!("auto-paused in {apps:?}"));
                    self.presenter.set_auto_paused(&apps);
                }
                Action::ArmKeys => self.keyboard.arm(),
                Action::DisarmKeys => self.keyboard.disarm(),
            }
        }
        if let Some(drawn) = drawn {
            let actions = self.engine.handle(Event::Drawn(drawn));
            self.apply(actions);
        }
    }

    fn replace(&mut self, id: u64, field_id: u64, correction: Correction) {
        let inbox = self.inbox.clone();
        self.reader.select(Selection {
            field_id,
            text: correction.current(),
            done: Box::new(move |outcome| inbox.post(Inbound::Selected(id, outcome))),
        });
    }

    pub(super) fn selection_answered(&mut self, id: u64) {
        if self.effects.pending_replace == id {
            self.kill_timer(TIMER_REPLACE);
        }
    }

    pub(super) fn replace_timed_out(&mut self) {
        self.kill_timer(TIMER_REPLACE);
        self.dispatch(Event::ReplaceTimedOut(self.effects.pending_replace));
    }

    fn key_hint(&self) -> Option<String> {
        let mut seen = self.presenter.recorded();
        seen.add(self.engine.counts());
        if !seen.needs_key_hint() {
            return None;
        }
        self.prefs.saved().accept_keys.first().map(Chord::label)
    }

    pub(super) fn debounce_elapsed(&mut self) {
        self.kill_timer(TIMER_DEBOUNCE);
        self.dispatch(Event::DebounceElapsed(self.effects.pending_debounce));
    }
}
