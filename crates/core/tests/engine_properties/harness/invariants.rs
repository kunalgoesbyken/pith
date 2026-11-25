use pith_core::{
    Action, Drawn, Event, FieldSnapshot, LINE_BREAKS, SelectOutcome, TriggerMode, check,
};

use super::Harness;
use crate::common::is_boundary;

fn context_key(field: &Option<FieldSnapshot>) -> Option<(u64, String, String, String, [bool; 3])> {
    field.as_ref().map(|f| {
        (
            f.field_id,
            f.app.clone(),
            f.before.clone(),
            f.after.text().to_string(),
            [f.has_selection, f.is_password, f.composing],
        )
    })
}

impl Harness {
    pub(super) fn send(&mut self, event: Event) -> Vec<Action> {
        self.trace.push(format!("{event:?}"));
        let before_key = context_key(&self.sent);
        let pre = self.engine.suggestion().map(str::to_string);
        let pre_offer = self.offer.clone();
        let accepting = matches!(event, Event::AcceptAll | Event::AcceptWord);
        let spelled = matches!(event, Event::Spelled { .. });
        let selected_text = match &event {
            Event::Selected {
                outcome: SelectOutcome::Selected { text, .. },
                ..
            } => Some(text.clone()),
            _ => None,
        };
        let verdict_for = match &event {
            Event::Selected { id, .. } => Some(*id),
            _ => None,
        };
        let answered = self
            .selecting
            .clone()
            .filter(|(id, _)| Some(*id) == verdict_for)
            .map(|(_, correction)| correction);
        let replace_ended = self.replace_ends(&event);
        let blocked = self.selecting.is_some() && !replace_ended && answered.is_none();
        let was_aborted = self.selecting_aborted;
        match &event {
            Event::Field(field) => self.sent = field.clone(),
            Event::SetConfig(config) => self.config = config.clone(),
            _ => {}
        }
        let actions = self.engine.handle(event.clone());
        self.trace.push(format!("  -> {actions:?}"));
        let mut scheduled_now = false;
        let mut inserted = None;
        let mut replacing = false;
        let mut overwritten = None;
        for action in &actions {
            match action {
                Action::ScheduleDebounce { generation, .. } => {
                    self.expect(*generation >= self.last_sched, "generation went backwards");
                    self.last_sched = *generation;
                    self.sched = Some(*generation);
                    self.scheduled.push(*generation);
                    scheduled_now = true;
                }
                Action::StartCompletion {
                    generation,
                    request,
                } => {
                    self.expect(self.flight.is_none(), "start while one is in flight");
                    self.expect(
                        self.sched == Some(*generation),
                        "start from a stale generation",
                    );
                    self.expect(
                        *generation >= self.last_start,
                        "start generation went backwards",
                    );
                    let field = self.sent.as_ref();
                    self.expect(
                        field.is_some_and(|f| check(f, &self.config).is_ok()),
                        "start for a field policy rejects",
                    );
                    self.expect(
                        field.is_some_and(|f| f.placement.is_some()),
                        "start without a caret",
                    );
                    self.expect(
                        field.is_some_and(|f| f.before.ends_with(&request.forced_prefix)),
                        "forced prefix is not the end of the text",
                    );
                    self.stats.starts += 1;
                    self.last_start = *generation;
                    self.flight = Some(*generation);
                    self.live = Some(*generation);
                }
                Action::CancelCompletion => {
                    self.expect(self.flight.is_some(), "cancel with nothing in flight");
                    self.stats.cancels += 1;
                    self.flight = None;
                    self.live = None;
                }
                Action::CheckSpelling {
                    generation,
                    language,
                    word,
                } => {
                    let field = self.sent.as_ref();
                    self.expect(self.config.typo_fix, "spell check with typo fix off");
                    self.expect(
                        self.config.trigger_mode == TriggerMode::Auto,
                        "spell check in manual mode",
                    );
                    self.expect(
                        field.is_some_and(|f| {
                            f.language.as_deref() == Some(language.as_str())
                                && f.placement.is_some()
                                && check(f, &self.config).is_ok()
                        }),
                        "spell check for a field that is not eligible",
                    );
                    self.expect(
                        field.is_some_and(|f| {
                            f.before
                                .strip_suffix(char::is_whitespace)
                                .is_some_and(|head| head.ends_with(word.as_str()))
                        }),
                        "spell check for a word that was not just finished",
                    );
                    self.expect(word.chars().count() >= 3, "spell check for a short word");
                    self.expect(
                        !self.declined.contains(word),
                        "spell check for a declined word",
                    );
                    self.stats.checks += 1;
                    self.checking = Some((*generation, word.clone()));
                }
                Action::Offer { correction, .. } => {
                    self.expect(
                        spelled || self.offer.is_some(),
                        "offer without a spelling verdict",
                    );
                    self.expect(
                        self.checking
                            .as_ref()
                            .is_some_and(|(_, word)| *word == correction.word),
                        "offer for a word that was not checked",
                    );
                    self.expect(
                        !correction.replacement.is_empty()
                            && correction.replacement != correction.word
                            && !correction.replacement.contains(LINE_BREAKS),
                        "offer with an unusable replacement",
                    );
                    self.expect(
                        self.sent.as_ref().is_some_and(|f| {
                            f.placement.is_some()
                                && check(f, &self.config).is_ok()
                                && f.before.ends_with(&correction.current())
                        }),
                        "offer for text that is not before the caret",
                    );
                    self.expect(self.shown.is_none(), "offer over a suggestion");
                    self.expect(
                        !self.declined.contains(&correction.word),
                        "offer for a declined word",
                    );
                    self.expect(
                        self.offer.as_ref().is_none_or(|offer| offer == correction),
                        "offer changed while visible",
                    );
                    if self.offer.is_none() {
                        self.stats.offers += 1;
                    }
                    self.offer = Some(correction.clone());
                }
                Action::ArmReplaceTimeout { id, .. } => {
                    self.expect(
                        self.selecting
                            .as_ref()
                            .is_some_and(|(pending, _)| pending == id),
                        "timeout armed for another replace",
                    );
                }
                Action::Replace {
                    id,
                    field_id,
                    correction,
                } => {
                    self.expect(
                        self.offer.as_ref() == Some(correction) && self.armed,
                        "replace without an armed correction",
                    );
                    self.expect(accepting, "replace without an accept event");
                    self.expect(
                        self.sent.as_ref().is_some_and(|f| {
                            f.field_id == *field_id && f.before.ends_with(&correction.current())
                        }),
                        "replace for text that is not before the caret",
                    );
                    self.stats.replaces += 1;
                    replacing = true;
                    self.selecting = Some((*id, correction.clone()));
                    self.selecting_key = self.sent.as_ref().map(|f| {
                        let prefix = f.before.strip_suffix(&correction.current());
                        (
                            f.field_id,
                            f.app.clone(),
                            prefix.unwrap_or_default().to_string(),
                        )
                    });
                    self.selecting_aborted = false;
                }
                Action::Overwrite(text) => {
                    self.expect(
                        answered.is_some() && selected_text.is_some(),
                        "overwrite without a selection result",
                    );
                    self.expect(
                        answered
                            .as_ref()
                            .is_some_and(|correction| correction.corrected() == *text),
                        "overwrite with text that was not asked for",
                    );
                    self.expect(
                        answered.as_ref().is_some_and(|correction| {
                            self.sent.as_ref().is_some_and(|f| {
                                f.has_selection || f.before.ends_with(&correction.current())
                            })
                        }),
                        "overwrite after the text moved",
                    );
                    overwritten = Some(text.clone());
                }
                Action::Collapse { field_id, text } => {
                    self.expect(
                        selected_text.as_ref() == Some(text) && *field_id != 0,
                        "collapse without a selection result",
                    );
                }
                Action::Show { cuts, .. } => {
                    let text = &cuts[0];
                    self.expect(!text.trim().is_empty(), "blank suggestion shown");
                    self.expect(!text.contains(LINE_BREAKS), "multiline suggestion shown");
                    self.expect(
                        self.sent.as_ref().is_some_and(|f| {
                            f.placement.is_some() && check(f, &self.config).is_ok()
                        }),
                        "show for a field that is not eligible",
                    );
                    self.stats.shows += 1;
                    self.shown = Some(text.clone());
                }
                Action::Hide => {
                    self.expect(
                        self.shown.is_some() || self.offer.is_some(),
                        "hide with nothing shown",
                    );
                    self.shown = None;
                    self.offer = None;
                }
                Action::ArmKeys => {
                    self.expect(!self.armed, "armed twice");
                    self.expect(
                        self.shown.is_some() || self.offer.is_some(),
                        "armed without a suggestion",
                    );
                    self.armed = true;
                }
                Action::DisarmKeys => {
                    self.expect(self.armed, "disarmed while not armed");
                    self.expect(
                        self.shown.is_none() && self.offer.is_none(),
                        "disarmed before hide",
                    );
                    self.armed = false;
                }
                Action::AutoPaused(_) | Action::Learn(_) => {}
                Action::Insert(inserted_text) => {
                    self.expect(
                        self.shown.is_some() && self.armed,
                        "insert without armed suggestion",
                    );
                    self.expect(pre_offer.is_none(), "insert while a correction shows");
                    self.expect(accepting, "insert without an accept event");
                    self.expect(!inserted_text.is_empty(), "empty insert");
                    self.stats.inserts += 1;
                    inserted = Some(inserted_text.clone());
                }
            }
        }
        if let (Some(inserted), Some(field)) = (&inserted, &mut self.sent) {
            field.before.push_str(inserted);
        }
        if let (Some(correction), Some(field), Some(_)) = (&answered, &mut self.sent, &overwritten)
        {
            let start = field.before.len() - correction.current().len();
            field.before.truncate(start);
            field.before.push_str(&correction.corrected());
        }
        if blocked {
            self.expect(
                !actions.iter().any(|action| {
                    matches!(
                        action,
                        Action::CheckSpelling { .. }
                            | Action::Offer { .. }
                            | Action::StartCompletion { .. }
                            | Action::Show { .. }
                            | Action::ScheduleDebounce { .. }
                    )
                }),
                "work started while a replace was pending",
            );
        }
        if let (Some((_, correction)), Some((id, app, prefix))) =
            (&self.selecting, &self.selecting_key)
        {
            match &event {
                Event::Field(None) => self.selecting_aborted = true,
                Event::Field(Some(field)) if field.field_id == *id && field.app == *app => {
                    let expected = field.before.ends_with(&correction.current())
                        || (field.has_selection && prefix.ends_with(&field.before));
                    self.selecting_aborted |= !expected;
                }
                _ => {}
            }
        }
        if replace_ended || answered.is_some() {
            self.selecting = None;
            self.selecting_key = None;
            self.selecting_aborted = false;
        }
        if let Event::Selected { outcome, .. } = &event {
            let selected = matches!(outcome, SelectOutcome::Selected { .. });
            self.expect(
                selected
                    || !actions.iter().any(|action| {
                        matches!(action, Action::Overwrite(_) | Action::Collapse { .. })
                    }),
                "a failed selection acted",
            );
            self.expect(
                !selected || answered.is_some() || actions.iter().any(is_collapse),
                "a late selection was left in place",
            );
            if let (true, Some(_)) = (selected, &answered) {
                self.expect(
                    was_aborted == actions.iter().any(is_collapse)
                        && was_aborted != overwritten.is_some(),
                    "a selection result neither overwrote nor collapsed correctly",
                );
            }
            if let (SelectOutcome::Unsupported, Some(correction), false) =
                (outcome, &answered, was_aborted)
            {
                self.declined.push(correction.word.clone());
            }
        }
        self.expect(
            self.shown.as_deref() == self.engine.suggestion(),
            "shown text differs from engine suggestion",
        );
        let drawn = actions
            .iter()
            .any(|action| matches!(action, Action::Show { .. } | Action::Offer { .. }));
        self.expect(
            drawn || self.armed == (self.shown.is_some() || self.offer.is_some()),
            "armed state differs from shown state",
        );
        self.expect(
            self.shown.is_none() || self.offer.is_none(),
            "a suggestion and a correction at once",
        );
        if let (Some(inserted), Some(pre)) = (&inserted, &pre) {
            self.expect(
                pre.starts_with(inserted.as_str()),
                "inserted text is not a prefix of the suggestion",
            );
            self.expect(is_boundary(pre, inserted.len()), "insert cuts a grapheme");
            let rest = &pre[inserted.len()..];
            let expected = (!rest.trim().is_empty()
                && self.sent.as_ref().is_some_and(|f| f.placement.is_some()))
            .then_some(rest);
            self.expect(
                self.engine.suggestion() == expected,
                "remainder after insert is wrong",
            );
        }
        if accepting && pre.is_none() && pre_offer.is_none() {
            self.expect(actions.is_empty(), "accept without suggestion acted");
        }
        if accepting && pre_offer.is_some() {
            self.expect(
                replacing && self.offer.is_none(),
                "accepting a correction did not replace it",
            );
        }
        match &event {
            Event::Completed { generation, .. } => {
                if self.live != Some(*generation) {
                    self.stats.stale += 1;
                    self.expect(actions.is_empty(), "stale completion had an effect");
                } else {
                    self.flight = None;
                    self.live = None;
                }
            }
            Event::Field(Some(_))
            | Event::Dismiss
            | Event::SetConfig(_)
            | Event::AcceptAll
            | Event::AcceptWord
            | Event::Selected { .. }
            | Event::Field(None) => {
                let changed = match &event {
                    Event::Field(Some(_)) => before_key != context_key(&self.sent),
                    Event::AcceptAll | Event::AcceptWord => pre.is_some(),
                    Event::Selected { .. } => answered.is_some(),
                    _ => true,
                };
                if changed && !scheduled_now {
                    self.sched = None;
                }
            }
            Event::DebounceElapsed(_)
            | Event::Resume(_)
            | Event::Trigger
            | Event::Drawn(_)
            | Event::Spelled { .. }
            | Event::Style(_)
            | Event::ReplaceTimedOut(_) => {}
        }
        self.check_dismissal(&event, scheduled_now);
        if matches!(event, Event::Field(None)) {
            self.expect(
                self.shown.is_none() && self.flight.is_none(),
                "focus loss left state behind",
            );
        }
        if drawn {
            let verdict = if self.offer.is_some() && self.rng.chance(20) {
                Drawn::Nothing
            } else {
                Drawn::Shown(0)
            };
            let mut actions = actions;
            actions.extend(self.send(Event::Drawn(verdict)));
            return actions;
        }
        actions
    }

    fn check_dismissal(&mut self, event: &Event, scheduled: bool) {
        if matches!(event, Event::Dismiss) {
            self.stats.dismissals += 1;
            self.dismissed = self.sent.as_ref().map(|f| (f.field_id, f.before.clone()));
            return;
        }
        let Some((id, before)) = self.dismissed.clone() else {
            return;
        };
        let Some(sent) = &self.sent else { return };
        if sent.field_id != id {
            return;
        }
        if sent.before != before && matches!(event, Event::Field(Some(_))) {
            self.dismissed = None;
            return;
        }
        if sent.before == before {
            self.expect(!scheduled, "scheduled again for dismissed text");
        }
    }
}

fn is_collapse(action: &Action) -> bool {
    matches!(action, Action::Collapse { .. })
}
