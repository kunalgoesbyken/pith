use std::time::{Duration, Instant};

use super::show::Shown;
use super::{Action, Engine, Phase};
use crate::{FieldSnapshot, ends_spaced_word, next_word};

fn closes_spaced_word(field: &FieldSnapshot, accepted: &str) -> bool {
    field.after.text().is_empty() && ends_spaced_word(&format!("{}{accepted}", field.before))
}

impl Engine {
    pub fn can_accept(&self, now: Instant, moves_caret: bool) -> bool {
        let shown_at = match &self.phase {
            Phase::Showing(shown) => shown.shown_at,
            Phase::Offering(offer) => Some(offer.shown_at),
            Phase::Idle | Phase::Requested | Phase::Checking(_) => return false,
        };
        let Some(field) = &self.field else {
            return false;
        };
        let racing = shown_at.is_some_and(|at| {
            now.saturating_duration_since(at) < Duration::from_millis(self.config.accept_race_ms)
        });
        !racing && (!moves_caret || field.after.ends_text())
    }

    pub(super) fn on_accept(&mut self, one_word: bool, out: &mut Vec<Action>) {
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Offering(offer) => self.accept_correction(offer.correction, out),
            Phase::Showing(shown) => self.accept_suggestion(shown, one_word, out),
            other => self.phase = other,
        }
    }

    fn accept_suggestion(&mut self, shown: Shown, one_word: bool, out: &mut Vec<Action>) {
        if !shown.engaged {
            self.counts.record_used();
        }
        let suggestion = shown.text;
        let accepted = if one_word {
            next_word(&suggestion).to_string()
        } else {
            suggestion.clone()
        };
        let rest = suggestion[accepted.len()..].to_string();
        let mut accepted = accepted;
        if let Some(field) = &self.field {
            self.ignores.accepted(&field.app);
            if self.config.smart_space && rest.is_empty() && closes_spaced_word(field, &accepted) {
                accepted.push(' ');
            }
            self.harvest.accepted(field, &self.config, &accepted);
        }
        self.counts.record_accept(&accepted, one_word);
        out.push(Action::Insert(accepted.clone()));
        if let Some(field) = &mut self.field {
            field.before.push_str(&accepted);
        }
        self.unlanded.push(accepted);
        if rest.trim().is_empty() {
            self.forced = None;
            self.withdraw(out);
            self.generation += 1;
            self.schedule(out);
        } else {
            self.phase = Phase::Showing(Shown {
                text: rest,
                cuts: Vec::new(),
                shown_at: None,
                engaged: true,
                counted: true,
            });
            self.show_current(out);
        }
    }
}
