use std::time::Instant;

use super::{Action, Engine, Phase};
use crate::context::field::FieldKey;
use crate::replacing::{Replacing, SelectOutcome};
use crate::text::typo::Finished;
use crate::{Correction, FieldSnapshot, Spelling, TriggerMode, check};

const MAX_SPELL_WAIT_MS: u64 = 1_000;
const MAX_REPLACE_WAIT_MS: u64 = 3_000;

pub(super) struct Offer {
    pub(super) correction: Correction,
    pub(super) shown_at: Instant,
}

impl Engine {
    pub(super) fn finished_word(&self, field: &FieldSnapshot) -> Option<Finished> {
        let previous = self
            .field
            .as_ref()
            .filter(|previous| FieldKey::of(previous).matches(field))?;
        Finished::of(&previous.before, &field.before)
    }

    pub(super) fn decline_offer(&mut self) {
        if let Phase::Offering(offer) = &self.phase {
            self.declined.insert(&offer.correction.word);
        }
    }

    pub(super) fn start_check(
        &mut self,
        finished: Option<Finished>,
        out: &mut Vec<Action>,
    ) -> bool {
        let (Some(finished), Some(field)) = (finished, &self.field) else {
            return false;
        };
        let Some(language) = field.language.clone() else {
            return false;
        };
        let wanted = self.config.typo_fix
            && self.replacing.is_none()
            && self.config.trigger_mode == TriggerMode::Auto
            && field.placement.is_some()
            && check(field, &self.config).is_ok()
            && !self.ignores.is_paused(&field.app)
            && !self.declined.contains(finished.word());
        if !wanted {
            return false;
        }
        out.push(Action::CheckSpelling {
            generation: self.generation,
            language,
            word: finished.word().to_string(),
        });
        out.push(Action::ScheduleDebounce {
            generation: self.generation,
            delay_ms: MAX_SPELL_WAIT_MS,
        });
        self.phase = Phase::Checking(finished);
        true
    }

    pub(super) fn on_spelled(
        &mut self,
        generation: u64,
        spelling: Spelling,
        out: &mut Vec<Action>,
    ) {
        if generation != self.generation {
            return;
        }
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Checking(finished) => self.offer(finished, spelling, out),
            other => self.phase = other,
        }
    }

    fn offer(&mut self, finished: Finished, spelling: Spelling, out: &mut Vec<Action>) {
        let Some(correction) = finished.correct(spelling) else {
            return self.schedule(out);
        };
        let offerable = self
            .field
            .as_ref()
            .is_some_and(|field| field.placement.is_some() && self.eligible(field).is_ok());
        if offerable {
            self.phase = Phase::Offering(Offer {
                correction,
                shown_at: Instant::now(),
            });
            self.show_current(out);
        } else {
            self.schedule(out);
        }
    }

    pub(super) fn accept_correction(&mut self, correction: Correction, out: &mut Vec<Action>) {
        if let Some(field) = &self.field {
            self.replaced += 1;
            let id = self.replaced;
            out.push(Action::Replace {
                id,
                field_id: field.field_id,
                correction: correction.clone(),
            });
            out.push(Action::ArmReplaceTimeout {
                id,
                delay_ms: MAX_REPLACE_WAIT_MS,
            });
            self.replacing = Some(Replacing::of(id, correction, field));
        }
        self.withdraw(out);
        self.generation += 1;
    }

    pub(super) fn on_replace_timeout(&mut self, id: u64, out: &mut Vec<Action>) {
        if self
            .replacing
            .take_if(|replacing| replacing.id == id)
            .is_some()
        {
            self.schedule(out);
        }
    }

    pub(super) fn on_selected(&mut self, id: u64, outcome: SelectOutcome, out: &mut Vec<Action>) {
        let replacing = self.replacing.take_if(|replacing| replacing.id == id);
        match outcome {
            SelectOutcome::Selected { field_id, text } => {
                match replacing.filter(|replacing| !replacing.aborted) {
                    Some(replacing) => {
                        out.push(Action::Overwrite(replacing.correction.corrected()))
                    }
                    None => {
                        out.push(Action::Collapse { field_id, text });
                        self.schedule(out);
                    }
                }
            }
            SelectOutcome::Unsupported => {
                if let Some(replacing) = replacing.filter(|replacing| !replacing.aborted) {
                    self.declined.insert(&replacing.correction.word);
                }
                self.schedule(out);
            }
            SelectOutcome::Lost => self.schedule(out),
        }
    }
}
