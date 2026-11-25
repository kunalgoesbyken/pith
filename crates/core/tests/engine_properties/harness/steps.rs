use pith_core::{
    Action, After, Completion, Config, Event, FieldSnapshot, SelectOutcome, Spelling, StopReason,
    TriggerMode, check,
};
use unicode_segmentation::UnicodeSegmentation;

use super::Harness;
use crate::common::text;
use crate::pools::{APPS, COMPLETIONS, LANGUAGE, STOPS, TYPOS, completion, placement, prose};

impl Harness {
    pub(crate) fn run_step(&mut self) {
        let roll = self.rng.below(100);
        match roll {
            0..=11 => self.type_text(),
            12..=14 => self.backspace(),
            15..=16 => self.replace_text(),
            17..=18 => self.switch_field(),
            19..=21 => self.toggle_placement(),
            22..=23 => self.toggle_flag(),
            24..=25 => self.set_after(),
            26..=33 => self.type_through(),
            34..=37 => self.resend(),
            38 => {
                self.send(Event::Field(None));
            }
            39..=40 => self.finish_word(),
            41..=44 => self.debounce(),
            45..=51 => self.complete(false),
            52..=76 => self.settle(),
            77..=86 => self.accept(),
            87..=89 => {
                self.send(Event::Dismiss);
            }
            90..=91 => {
                let config = Config {
                    enabled: self.rng.chance(60),
                    typo_fix: self.rng.chance(80),
                    trigger_mode: if self.rng.chance(15) {
                        TriggerMode::Manual
                    } else {
                        TriggerMode::Auto
                    },
                    min_chars_before: if self.rng.chance(50) { 3 } else { 6 },
                    ..self.config.clone()
                };
                self.send(Event::SetConfig(config));
            }
            92..=93 => self.spell(),
            94 => self.select(),
            95 => self.time_out(),
            _ => self.complete(false),
        }
    }

    fn settle(&mut self) {
        self.debounce();
        if self.flight.is_some() {
            self.complete(true);
        }
    }

    fn finish_word(&mut self) {
        let word = self.rng.pick(TYPOS).to_string();
        let placement = placement(&mut self.rng);
        if let Some(field) = &mut self.world {
            field.before.push(' ');
            field.before.push_str(&word);
            field.has_selection = false;
            field.composing = false;
            field.is_password = false;
            field.after = After::default();
            field.placement.get_or_insert(placement);
        }
        self.publish();
        if let Some(field) = &mut self.world {
            field.before.push(' ');
        }
        self.publish();
        if self.rng.chance(70) {
            self.spell();
        }
    }

    pub(super) fn replace_ends(&self, event: &Event) -> bool {
        let (Some((id, _)), Some((field_id, app, _))) = (&self.selecting, &self.selecting_key)
        else {
            return false;
        };
        match event {
            Event::ReplaceTimedOut(elapsed) => elapsed == id,
            Event::Field(Some(field)) => field.field_id != *field_id || field.app != *app,
            _ => false,
        }
    }

    fn select(&mut self) {
        let id = match &self.selecting {
            Some((id, _)) if self.rng.chance(90) => *id,
            Some((id, _)) => id + 1,
            None => self.rng.below(5) as u64,
        };
        let outcome = match self.rng.below(10) {
            0..=6 => SelectOutcome::Selected {
                field_id: 1,
                text: "teh ".into(),
            },
            7 => SelectOutcome::Unsupported,
            _ => SelectOutcome::Lost,
        };
        let outcome = match (&self.selecting, outcome) {
            (Some((_, correction)), SelectOutcome::Selected { field_id, .. }) => {
                SelectOutcome::Selected {
                    field_id,
                    text: correction.current(),
                }
            }
            (_, outcome) => outcome,
        };
        let actions = self.send(Event::Selected { id, outcome });
        if actions
            .iter()
            .any(|action| matches!(action, Action::Overwrite(_)))
        {
            self.world = self.sent.clone();
            if self.rng.chance(60) {
                self.publish();
            }
        }
    }

    fn time_out(&mut self) {
        let id = match &self.selecting {
            Some((id, _)) if self.rng.chance(80) => *id,
            _ => self.rng.below(5) as u64,
        };
        self.send(Event::ReplaceTimedOut(id));
    }

    fn spell(&mut self) {
        let Some((generation, word)) = self.checking.clone() else {
            return;
        };
        let (generation, expects_offer) = if self.rng.chance(85) {
            (generation, true)
        } else {
            (generation + 1 + self.rng.below(3) as u64, false)
        };
        let spelling = match self.rng.below(9) {
            0 => Spelling::Correct,
            1 => Spelling::Autocorrect(format!("{word}s")),
            2 => Spelling::Autocorrect(String::new()),
            7 => Spelling::Autocorrect(format!("{word} {word}")),
            8 => Spelling::Suggested("x".repeat(40)),
            3 => Spelling::Autocorrect(word.clone()),
            4 => Spelling::Autocorrect("a\nb".into()),
            5 => Spelling::Suggested(format!("{word}s")),
            _ => Spelling::Suggested(format!("{word}{word}")),
        };
        let actions = self.send(Event::Spelled {
            generation,
            spelling: spelling.clone(),
        });
        let offered = actions
            .iter()
            .any(|action| matches!(action, Action::Offer { .. }));
        if offered {
            self.expect(expects_offer, "a stale verdict offered a correction");
            let allowed = match &spelling {
                Spelling::Correct => false,
                Spelling::Autocorrect(replacement) => {
                    !replacement.is_empty()
                        && *replacement != word
                        && !replacement.contains(char::is_whitespace)
                }
                Spelling::Suggested(replacement) => *replacement == format!("{word}s"),
            };
            self.expect(allowed, "a verdict that must not be offered was offered");
            if self.rng.chance(50) {
                self.accept();
            }
        }
    }

    pub(crate) fn publish(&mut self) {
        let snapshot = self.world.clone();
        self.lagging = false;
        self.send(Event::Field(snapshot));
    }

    fn type_text(&mut self) {
        let piece = prose(&mut self.rng);
        if let Some(field) = &mut self.world {
            field.before.push_str(&piece);
        }
        self.publish();
    }

    fn backspace(&mut self) {
        if self.lagging {
            self.publish();
        }
        if let Some(field) = &mut self.world
            && let Some((start, _)) = field.before.grapheme_indices(true).next_back()
        {
            field.before.truncate(start);
        }
        self.publish();
    }

    fn replace_text(&mut self) {
        let replacement = text(&mut self.rng, 12);
        if let Some(field) = &mut self.world {
            field.before = replacement;
        }
        self.publish();
    }

    fn switch_field(&mut self) {
        let id = 1 + self.rng.below(3) as u64;
        let app = self.rng.pick(APPS).to_string();
        let before = if self.rng.chance(50) {
            "Please sign the".to_string()
        } else {
            text(&mut self.rng, 8)
        };
        let placement = Some(placement(&mut self.rng));
        self.world = Some(FieldSnapshot {
            field_id: id,
            app,
            before,
            placement,
            language: Some(LANGUAGE.into()),
            ..Default::default()
        });
        self.publish();
    }

    fn toggle_placement(&mut self) {
        let placement = self.rng.chance(50).then(|| placement(&mut self.rng));
        if let Some(field) = &mut self.world {
            field.placement = placement;
        }
        self.publish();
    }

    fn toggle_flag(&mut self) {
        let which = self.rng.below(4);
        if let Some(field) = &mut self.world {
            match which {
                0 => field.has_selection = !field.has_selection,
                1 => field.is_password = !field.is_password,
                2 => field.composing = !field.composing,
                _ => {
                    field.language = match field.language {
                        Some(_) => None,
                        None => Some(LANGUAGE.into()),
                    }
                }
            }
        }
        self.publish();
    }

    fn set_after(&mut self) {
        let after = text(&mut self.rng, 3);
        if let Some(field) = &mut self.world {
            field.after = after.into();
        }
        self.publish();
    }

    fn resend(&mut self) {
        self.publish();
    }

    fn type_through(&mut self) {
        let (Some(suggestion), Some(sent)) = (
            self.engine.suggestion().map(str::to_string),
            self.world.clone(),
        ) else {
            return self.type_text();
        };
        let graphemes = suggestion.graphemes(true).count();
        let take = 1 + self.rng.below(graphemes);
        let typed: String = suggestion.graphemes(true).take(take).collect();
        let mut next = sent.clone();
        next.before.push_str(&typed);
        self.world = Some(next.clone());
        self.send(Event::Field(Some(next.clone())));
        let rest = &suggestion[typed.len()..];
        let eligible = check(&next, &self.config).is_ok() && next.placement.is_some();
        let expected = (eligible && !rest.trim().is_empty()).then_some(rest);
        self.expect(
            self.engine.suggestion() == expected,
            "typing through the suggestion did not shrink it",
        );
    }

    fn debounce(&mut self) {
        let Some(latest) = self.scheduled.last().copied() else {
            return;
        };
        let generation = if self.rng.chance(75) {
            latest
        } else {
            *self.rng.pick(&self.scheduled)
        };
        self.send(Event::DebounceElapsed(generation));
    }

    fn complete(&mut self, sane: bool) {
        let generation = match self.flight {
            Some(flight) if self.rng.chance(85) => flight,
            _ => self.last_start.saturating_sub(self.rng.below(3) as u64),
        };
        let result = if sane {
            Ok(Completion {
                text: self.rng.pick(COMPLETIONS).to_string(),
                mean_logprob: Some(-0.5),
                tokens: 4,
                prompt_tokens_evaluated: 4,
                stop: *self.rng.pick(&STOPS[..5]),
            })
        } else if self.rng.chance(10) {
            Err("boom".to_string())
        } else {
            Ok(completion(&mut self.rng))
        };
        let shown_before = self.shown.clone();
        let live = self.live == Some(generation);
        let before = self.sent.as_ref().map(|f| f.before.clone());
        let words = self.config.max_words;
        let actions = self.send(Event::Completed {
            generation,
            result: result.clone(),
        });
        if !live {
            return;
        }
        let Some(Action::Show { cuts, .. }) = actions.first() else {
            return;
        };
        let shown = &cuts[0];
        let Ok(done) = result else {
            self.fail("error result was shown");
        };
        self.expect(
            shown_before.is_none(),
            "completion replaced a visible suggestion",
        );
        self.expect(
            done.stop != StopReason::Cancelled,
            "cancelled result was shown",
        );
        self.expect(
            done.mean_logprob
                .is_none_or(|mean| mean >= self.config.min_mean_logprob),
            "weak result was shown",
        );
        self.expect(
            done.text.trim_start().starts_with(shown.trim_start()),
            "shown text is not a prefix of the completion",
        );
        let count = shown
            .split_word_bounds()
            .filter(|s| s.chars().any(char::is_alphanumeric))
            .count();
        self.expect(count <= words, "more words than max_words");
        if before.is_some_and(|b| b.ends_with(char::is_whitespace)) {
            self.expect(!shown.starts_with(char::is_whitespace), "doubled space");
        }
    }

    fn accept(&mut self) {
        let event = if self.rng.chance(50) {
            Event::AcceptAll
        } else {
            Event::AcceptWord
        };
        let actions = self.send(event);
        if matches!(actions.first(), Some(Action::Replace { .. })) && self.rng.chance(85) {
            self.select();
        }
        if matches!(actions.first(), Some(Action::Insert(_))) {
            self.world = self.sent.clone();
            if self.rng.chance(60) {
                self.publish();
            } else {
                self.lagging = true;
            }
        }
    }
}
