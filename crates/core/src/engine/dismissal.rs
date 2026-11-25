use super::{Action, Engine};
use crate::context::field::FieldKey;
use crate::{FieldSnapshot, Placement, SkipReason, check};

enum Release {
    Edit,
    EditOrMove(Option<Placement>),
    Crowded,
}

pub(super) struct Dismissal {
    pub(super) field: FieldKey,
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

    pub(super) fn holds(&self, field: &FieldSnapshot) -> bool {
        self.field.matches(field)
            && self.before == field.before
            && match self.release {
                Release::Edit | Release::Crowded => true,
                Release::EditOrMove(placement) => placement == field.placement,
            }
    }
}

impl Engine {
    pub fn skip(&self) -> Option<SkipReason> {
        let field = self.field.as_ref()?;
        check(field, &self.config).err().or_else(|| {
            let crowded = self.dismissed.as_ref().is_some_and(|dismissal| {
                !matches!(dismissal.release, Release::Edit) && dismissal.holds(field)
            });
            crowded.then_some(SkipReason::NoRoom)
        })
    }

    pub(super) fn on_dismiss(&mut self, out: &mut Vec<Action>) {
        let ignored = self.unengaged_app();
        self.decline_offer();
        self.reset(out);
        if let Some(app) = ignored {
            self.note_ignored(&app, out);
        }
        self.dismissed = self
            .field
            .as_ref()
            .map(|field| Dismissal::of(field, Release::Edit));
    }

    pub(super) fn give_up(&mut self, out: &mut Vec<Action>) {
        self.withdraw(out);
        self.forced = None;
        let Some(field) = &self.field else { return };
        let repeated = self
            .crowded_at
            .as_ref()
            .is_some_and(|(key, before)| key.matches(field) && *before == field.before);
        let release = if repeated {
            Release::Crowded
        } else {
            Release::EditOrMove(field.placement)
        };
        self.crowded_at = Some((FieldKey::of(field), field.before.clone()));
        self.dismissed = Some(Dismissal::of(field, release));
    }
}
