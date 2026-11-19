use unicode_segmentation::UnicodeSegmentation;

use super::memory::Sample;
use super::sentence::Sentence;
use crate::field::FieldKey;
use crate::{Config, FieldSnapshot, LINE_BREAKS, SentenceEnd, check_forced, sentence_end};

struct Pending {
    key: FieldKey,
    accepted: String,
}

#[derive(Default)]
pub(crate) struct Harvest(Option<Pending>);

fn is_finished(before: &str) -> bool {
    before.ends_with(LINE_BREAKS)
        || match sentence_end(before) {
            Some(SentenceEnd::Closed) => true,
            Some(SentenceEnd::NeedsSpace) => before.ends_with(char::is_whitespace),
            None => false,
        }
}

fn may_learn(field: &FieldSnapshot, config: &Config) -> bool {
    config.uses_style() && check_forced(field, config).is_ok()
}

impl Harvest {
    pub(crate) fn clear(&mut self) {
        self.0 = None;
    }

    pub(crate) fn accepted(&mut self, field: &FieldSnapshot, config: &Config, text: &str) {
        let accepted = text.trim();
        self.0 = (!accepted.is_empty() && may_learn(field, config)).then(|| Pending {
            key: FieldKey::of(field),
            accepted: accepted.to_string(),
        });
    }

    pub(crate) fn observe(&mut self, field: &FieldSnapshot, config: &Config) -> Option<Sample> {
        let pending = self.0.as_ref()?;
        if !pending.key.matches(field) {
            self.0 = None;
            return None;
        }
        if !is_finished(&field.before) {
            return None;
        }
        let pending = self.0.take()?;
        if !may_learn(field, config) {
            return None;
        }
        let last = field.before.trim_end().unicode_sentences().last()?;
        if !last.contains(&pending.accepted) {
            return None;
        }
        Some(Sample::new(Sentence::learnable(last)?, &field.app))
    }
}
