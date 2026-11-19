use std::time::Instant;

use super::{Action, Engine, Phase};
use crate::suffix::candidates;
use crate::{Drawn, Placement, Suffix};

pub(super) struct Shown {
    pub(super) text: String,
    pub(super) cuts: Vec<String>,
    pub(super) shown_at: Option<Instant>,
    pub(super) engaged: bool,
    pub(super) counted: bool,
}

impl Engine {
    pub(super) fn withdraw(&mut self, out: &mut Vec<Action>) {
        self.phase = Phase::Idle;
        out.push(Action::Hide);
        if std::mem::replace(&mut self.armed, false) {
            out.push(Action::DisarmKeys);
        }
    }

    pub(super) fn reset(&mut self, out: &mut Vec<Action>) {
        match self.phase {
            Phase::Idle => {}
            Phase::Requested => {
                self.phase = Phase::Idle;
                out.push(Action::CancelCompletion);
            }
            Phase::Checking(_) => self.phase = Phase::Idle,
            Phase::Showing(_) | Phase::Offering(_) => self.withdraw(out),
        }
        self.forced = None;
        self.unlanded.clear();
        self.generation += 1;
    }

    pub(super) fn on_drawn(&mut self, drawn: Drawn, out: &mut Vec<Action>) {
        match &mut self.phase {
            Phase::Showing(shown) => {
                let Drawn::Shown(index) = drawn else {
                    return self.give_up(out);
                };
                let Some(text) = shown.cuts.get(index) else {
                    return self.give_up(out);
                };
                shown.text = text.clone();
                if !std::mem::replace(&mut shown.counted, true) {
                    self.counts.record_shown();
                }
                self.crowded_at = None;
                self.arm(out);
            }
            Phase::Offering(_) => match drawn {
                Drawn::Shown(_) => self.arm(out),
                Drawn::Nothing => {
                    self.withdraw(out);
                    self.schedule(out);
                }
            },
            Phase::Idle | Phase::Requested | Phase::Checking(_) => {}
        }
    }

    fn arm(&mut self, out: &mut Vec<Action>) {
        if !std::mem::replace(&mut self.armed, true) {
            out.push(Action::ArmKeys);
        }
    }

    pub(super) fn show_current(&mut self, out: &mut Vec<Action>) {
        match &self.phase {
            Phase::Showing(_) => self.show_suggestion(out),
            Phase::Offering(_) => self.show_offer(out),
            Phase::Idle | Phase::Requested | Phase::Checking(_) => {}
        }
    }

    fn show_offer(&mut self, out: &mut Vec<Action>) {
        let (Phase::Offering(offer), Some(field)) = (&self.phase, &self.field) else {
            return;
        };
        let Some(placement) = field.placement else {
            return self.withdraw(out);
        };
        out.push(Action::Offer {
            correction: offer.correction.clone(),
            placement,
        });
    }

    fn show_suggestion(&mut self, out: &mut Vec<Action>) {
        let (Phase::Showing(shown), Some(field)) = (&mut self.phase, &self.field) else {
            return;
        };
        let Some(placement) = field.placement else {
            return self.withdraw(out);
        };
        let cuts = candidates(&shown.text, &field.after);
        if cuts.is_empty() {
            return self.give_up(out);
        }
        let placement = match placement {
            Placement::Inline(caret) if !matches!(field.after.suffix(), Suffix::Blank) => {
                Placement::BelowCaret(caret)
            }
            placement => placement,
        };
        shown.cuts.clone_from(&cuts);
        out.push(Action::Show {
            cuts,
            placement,
            style: field.style.clone(),
        });
    }
}
