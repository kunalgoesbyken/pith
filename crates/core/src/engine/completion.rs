use std::time::Instant;

use super::show::Shown;
use super::{Action, Engine, Phase};
use crate::{Completion, Primer, TriggerMode, build, check, suggestion};

impl Engine {
    pub(super) fn schedule(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        if self.replacing.is_some() {
            return;
        }
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

    pub(super) fn on_debounce(&mut self, generation: u64, out: &mut Vec<Action>) {
        if generation != self.generation {
            return;
        }
        if matches!(self.phase, Phase::Checking(_)) {
            self.phase = Phase::Idle;
        }
        if !self.is_idle() {
            return;
        }
        let Some(field) = &self.field else { return };
        if field.placement.is_none() || self.suppressed(field) {
            return;
        }
        self.phase = Phase::Requested;
        out.push(Action::StartCompletion {
            generation,
            request: build(
                field,
                &self.config,
                &Primer::select(&self.style, field, &self.config),
            ),
        });
    }

    pub(super) fn on_completed(
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
    }
}
