use unicode_segmentation::UnicodeSegmentation;

use super::memory::Sample;
use super::sentence::Sentence;
use crate::field::FieldKey;
use crate::{Config, FieldSnapshot, LINE_BREAKS, SentenceEnd, check_forced, sentence_end};

struct Pending {
    key: FieldKey,
    anchor: String,
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

fn open_sentence(before: &str) -> &str {
    if is_finished(before) {
        return "";
    }
    before.unicode_sentences().last().unwrap_or_default()
}

fn may_learn(field: &FieldSnapshot, config: &Config) -> bool {
    config.uses_style() && check_forced(field, config).is_ok()
}

impl Harvest {
    pub(crate) fn clear(&mut self) {
        self.0 = None;
    }

    pub(crate) fn accepted(&mut self, field: &FieldSnapshot, config: &Config, text: &str) {
        self.0 = (!text.trim().is_empty() && may_learn(field, config)).then(|| Pending {
            key: FieldKey::of(field),
            anchor: format!("{}{}", open_sentence(&field.before), text.trim_end()),
        });
    }

    pub(crate) fn observe(&mut self, field: &FieldSnapshot, config: &Config) -> Option<Sample> {
        let pending = self.0.as_ref()?;
        if !pending.key.matches(field) || !field.before.contains(&pending.anchor) {
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
        if !last.starts_with(&pending.anchor) {
            return None;
        }
        Some(Sample::new(Sentence::learnable(last)?, &field.app))
    }
}
