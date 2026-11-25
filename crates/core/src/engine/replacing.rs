use crate::context::field::FieldKey;
use crate::{Correction, FieldSnapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectOutcome {
    Selected { field_id: u64, text: String },
    Unsupported,
    Lost,
}

pub(crate) struct Replacing {
    pub(crate) id: u64,
    pub(crate) correction: Correction,
    pub(crate) field: FieldKey,
    prefix: String,
    pub(crate) aborted: bool,
}

impl Replacing {
    pub(crate) fn of(id: u64, correction: Correction, field: &FieldSnapshot) -> Self {
        let prefix = field
            .before
            .strip_suffix(&correction.current())
            .unwrap_or_default()
            .to_string();
        Self {
            id,
            correction,
            field: FieldKey::of(field),
            prefix,
            aborted: false,
        }
    }

    pub(crate) fn observe(&mut self, field: &FieldSnapshot) {
        self.aborted |= !self.expects(field);
    }

    fn expects(&self, field: &FieldSnapshot) -> bool {
        self.field.matches(field)
            && (field.before.ends_with(&self.correction.current())
                || (field.has_selection && self.prefix.ends_with(&field.before)))
    }
}
