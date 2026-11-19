use icu_properties::CodePointMapData;
use icu_properties::props::{GeneralCategory, GeneralCategoryGroup};

use crate::text::is_word_like;
use crate::{LINE_BREAKS, sentence_end};

const CLOSERS: GeneralCategoryGroup = GeneralCategoryGroup::ClosePunctuation
    .union(GeneralCategoryGroup::FinalPunctuation)
    .union(GeneralCategoryGroup::OtherPunctuation);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suffix {
    Blank,
    Closing,
    Text,
}

fn is_closer(c: char) -> bool {
    CLOSERS.contains(CodePointMapData::<GeneralCategory>::new().get(c))
}

impl Suffix {
    pub fn of(after: &str) -> Self {
        let line = rest_of_line(after);
        if line.chars().all(char::is_whitespace) {
            Self::Blank
        } else if line.chars().all(|c| c.is_whitespace() || is_closer(c)) {
            Self::Closing
        } else {
            Self::Text
        }
    }
}

fn rest_of_line(after: &str) -> &str {
    after.split(LINE_BREAKS).next().unwrap_or_default()
}

fn has_sentence_end(text: &str) -> bool {
    text.char_indices()
        .any(|(start, c)| sentence_end(&text[..start + c.len_utf8()]).is_some())
}

pub(crate) fn fit(candidate: &str, after: &str) -> Option<String> {
    let closers = match Suffix::of(after) {
        Suffix::Blank => return (!candidate.is_empty()).then(|| candidate.to_string()),
        Suffix::Closing => rest_of_line(after),
        Suffix::Text => return None,
    };
    let candidate = candidate
        .trim_end()
        .trim_end_matches(|c| is_closer(c) && closers.contains(c))
        .trim_end();
    let past_the_suffix =
        candidate.trim_start().starts_with(is_closer) || has_sentence_end(candidate);
    (is_word_like(candidate) && !past_the_suffix).then(|| candidate.to_string())
}
