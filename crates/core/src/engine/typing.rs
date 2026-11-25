use unicode_segmentation::UnicodeSegmentation;

use super::{Action, Engine, Phase};
use crate::context::field::FieldKey;
use crate::{FieldSnapshot, check, continue_window};

fn is_grapheme_boundary(text: &str, index: usize) -> bool {
    index == text.len() || text.grapheme_indices(true).any(|(start, _)| start == index)
}

impl Engine {
    pub(super) fn on_field_lost(&mut self, out: &mut Vec<Action>) {
        self.harvest.clear();
        if let Some(replacing) = &mut self.replacing {
            replacing.aborted = true;
        }
        self.reset(out);
        self.field = None;
    }

    pub(super) fn on_field(&mut self, mut field: FieldSnapshot, out: &mut Vec<Action>) {
        if let Some(replacing) = &mut self.replacing {
            replacing.observe(&field);
        }
        self.replacing
            .take_if(|replacing| !replacing.field.matches(&field));
        if self.insert_pending(&field) {
            return;
        }
        if let Some(previous) = &self.field
            && FieldKey::of(previous).matches(&field)
        {
            field.before = continue_window(
                &previous.before,
                &field.before,
                self.config.lookback_chars(),
            );
        }
        if let Some(sample) = self.harvest.observe(&field, &self.config) {
            out.push(Action::Learn(sample));
        }
        let finished = self.finished_word(&field);
        let suggestable = field.placement.is_some() && check(&field, &self.config).is_ok();
        let released = self
            .dismissed
            .take_if(|dismissal| {
                suggestable && dismissal.field.matches(&field) && !dismissal.holds(&field)
            })
            .is_some();
        let mut ignored = None;
        if let Some(previous) = &self.field {
            let same_field =
                FieldKey::of(previous).matches(&field) && self.eligible(&field).is_ok();
            if same_field && previous.before == field.before && previous.after == field.after {
                let moved = previous.placement != field.placement;
                let unblocked = released
                    || self.eligible(previous).is_err()
                    || (previous.placement.is_none() && self.is_idle());
                self.field = Some(field);
                if unblocked {
                    self.schedule(out);
                } else if moved {
                    self.show_current(out);
                }
                return;
            }
            if same_field && let Some(rest) = self.typed_through(previous, &field) {
                if rest.trim().is_empty() {
                    self.ignores.accepted(&field.app);
                    self.forced = None;
                    self.withdraw(out);
                    self.field = Some(field);
                    self.schedule(out);
                } else {
                    self.field = Some(field);
                    if let Phase::Showing(shown) = &mut self.phase {
                        shown.text = rest;
                    }
                    self.show_current(out);
                }
                return;
            }
            if same_field && previous.before != field.before && self.was_readable() {
                ignored = self.unengaged_app();
            }
        }
        if self.typed_past(&field) {
            self.decline_offer();
        }
        self.reset(out);
        if let Some(app) = ignored {
            self.note_ignored(&app, out);
        }
        self.field = Some(field);
        if !self.start_check(finished, out) {
            self.schedule(out);
        }
    }

    fn insert_pending(&mut self, field: &FieldSnapshot) -> bool {
        if self.unlanded.is_empty() {
            return false;
        }
        let position = self
            .field
            .as_ref()
            .filter(|previous| FieldKey::of(previous).matches(field))
            .filter(|_| self.eligible(field).is_ok())
            .and_then(|previous| {
                let mut candidates = vec![previous.before.as_str()];
                for accepted in self.unlanded.iter().rev() {
                    let Some(rest) = candidates
                        .last()
                        .and_then(|text| text.strip_suffix(accepted.as_str()))
                    else {
                        break;
                    };
                    candidates.push(rest);
                }
                let window = self.config.lookback_chars() / 2;
                candidates.iter().position(|candidate| {
                    *candidate == field.before
                        || (field.before.chars().count() >= window
                            && candidate.ends_with(field.before.as_str()))
                })
            });
        match position {
            Some(behind) if behind > 0 => {
                let landed = self.unlanded.len() - behind;
                self.unlanded.drain(..landed);
                true
            }
            _ => {
                self.unlanded.clear();
                false
            }
        }
    }

    fn typed_past(&self, field: &FieldSnapshot) -> bool {
        self.field.as_ref().is_some_and(|previous| {
            FieldKey::of(previous).matches(field) && previous.before != field.before
        })
    }

    fn typed_through(&self, previous: &FieldSnapshot, field: &FieldSnapshot) -> Option<String> {
        let suggestion = self.suggestion()?;
        let typed = field.before.strip_prefix(previous.before.as_str())?;
        if typed.is_empty() || previous.after != field.after {
            return None;
        }
        let rest = suggestion.strip_prefix(typed)?;
        is_grapheme_boundary(suggestion, typed.len()).then(|| rest.to_string())
    }
}
