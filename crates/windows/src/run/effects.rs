use std::sync::mpsc::Sender;
use std::time::Instant;

use pith_core::{Action, Completion, Event, log};

use super::{App, InsertMode, TIMER_DEBOUNCE};
use crate::clipboard::{Trigger, paste};

pub struct Effects {
    insert: InsertMode,
    observer: Option<Sender<Action>>,
    pending_debounce: u64,
    started: Option<Instant>,
}

impl Effects {
    pub fn new(insert: InsertMode, observer: Option<Sender<Action>>) -> Self {
        Self {
            insert,
            observer,
            pending_debounce: 0,
            started: None,
        }
    }

    fn observe(&self, action: &Action) {
        if let Some(observer) = &self.observer {
            let _ = observer.send(action.clone());
        }
    }

    fn insert(&self, text: String) {
        match &self.insert {
            InsertMode::Paste => paste(text, Trigger::Chord),
            InsertMode::Custom(insert) => insert(text),
        }
    }

    pub fn log_result(&mut self, result: &Result<Completion, String>) {
        match (self.started.take(), result) {
            (Some(started), Ok(completion)) => log(&format!(
                "completion {} ms, {} tokens, {} prompt tokens, logprob {:.2}, {:?}",
                started.elapsed().as_millis(),
                completion.tokens,
                completion.prompt_tokens_evaluated,
                completion.mean_logprob,
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
        for action in actions {
            self.effects.observe(&action);
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
                Action::Show {
                    text,
                    placement,
                    style,
                } => self.overlay.show(&text, placement, &style),
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
    }

    pub(super) fn debounce_elapsed(&mut self) {
        self.kill_timer(TIMER_DEBOUNCE);
        self.dispatch(Event::DebounceElapsed(self.effects.pending_debounce));
    }
}
