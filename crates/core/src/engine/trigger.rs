use super::{Action, Engine, Phase};
use crate::field::FieldKey;
use crate::{FieldSnapshot, Primer, SkipReason, build, check, check_forced};

impl Engine {
    fn is_forced(&self, field: &FieldSnapshot) -> bool {
        self.forced.as_ref().is_some_and(|key| key.matches(field))
    }

    pub(super) fn eligible(&self, field: &FieldSnapshot) -> Result<(), SkipReason> {
        if self.is_forced(field) {
            check_forced(field, &self.config)
        } else {
            check(field, &self.config)
        }
    }

    pub(super) fn suppressed(&self, field: &FieldSnapshot) -> bool {
        self.eligible(field).is_err()
            || (!self.is_forced(field) && self.ignores.is_paused(&field.app))
    }

    pub(super) fn on_trigger(&mut self, out: &mut Vec<Action>) {
        let Some(field) = &self.field else { return };
        if !matches!(self.phase, Phase::Idle | Phase::Checking(_))
            || self.replacing.is_some()
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
                request: build(
                    field,
                    &self.config,
                    &Primer::select(&self.style, field, &self.config),
                ),
            });
        }
    }
}
