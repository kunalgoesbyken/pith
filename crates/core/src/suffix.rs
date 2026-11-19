use icu_properties::props::{
    GeneralCategory, GeneralCategoryGroup, QuotationMark, TerminalPunctuation,
};
use icu_properties::{CodePointMapData, CodePointSetData};
use unicode_segmentation::UnicodeSegmentation;

use crate::LINE_BREAKS;
use crate::text::is_word_like;

const CLOSING: GeneralCategoryGroup =
    GeneralCategoryGroup::ClosePunctuation.union(GeneralCategoryGroup::FinalPunctuation);
const OPENING: GeneralCategoryGroup =
    GeneralCategoryGroup::OpenPunctuation.union(GeneralCategoryGroup::InitialPunctuation);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Suffix {
    #[default]
    Blank,
    Closing(String),
    Text,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct After {
    text: String,
    suffix: Suffix,
}

impl After {
    pub fn new(text: String) -> Self {
        let suffix = Suffix::of(&text);
        Self { text, suffix }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn suffix(&self) -> &Suffix {
        &self.suffix
    }

    pub fn ends_text(&self) -> bool {
        self.text.trim().is_empty()
    }
}

impl From<&str> for After {
    fn from(text: &str) -> Self {
        Self::new(text.to_string())
    }
}

impl From<String> for After {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

fn category(c: char) -> GeneralCategory {
    CodePointMapData::<GeneralCategory>::new().get(c)
}

fn is_closer(c: char) -> bool {
    let category = category(c);
    CLOSING.contains(category)
        || CodePointSetData::new::<TerminalPunctuation>().contains(c)
        || (CodePointSetData::new::<QuotationMark>().contains(c) && !OPENING.contains(category))
}

fn is_opener(c: char) -> bool {
    category(c) == GeneralCategory::OpenPunctuation
}

fn closes_unopened(text: &str) -> bool {
    let mut open = 0usize;
    text.chars().any(|c| match category(c) {
        GeneralCategory::OpenPunctuation => {
            open += 1;
            false
        }
        GeneralCategory::ClosePunctuation => match open.checked_sub(1) {
            Some(rest) => {
                open = rest;
                false
            }
            None => true,
        },
        _ => false,
    })
}

impl Suffix {
    fn of(after: &str) -> Self {
        let line = after.split(LINE_BREAKS).next().unwrap_or_default();
        if line.chars().all(char::is_whitespace) {
            Self::Blank
        } else if line.chars().all(|c| c.is_whitespace() || is_closer(c)) {
            Self::Closing(line.trim().to_string())
        } else {
            Self::Text
        }
    }
}

fn spans_sentences(text: &str) -> bool {
    text.trim().unicode_sentences().nth(1).is_some()
}

pub(crate) fn fit(candidate: &str, after: &After) -> Option<String> {
    let run = match after.suffix() {
        Suffix::Blank => return (!candidate.is_empty()).then(|| candidate.to_string()),
        Suffix::Closing(run) => run,
        Suffix::Text => return None,
    };
    let candidate = candidate.trim_end();
    let candidate = match candidate.strip_suffix(run.as_str()) {
        Some(rest) if !candidate.chars().any(is_opener) => rest.trim_end(),
        Some(_) => return None,
        None => candidate,
    };
    let repeats = candidate
        .chars()
        .next_back()
        .is_some_and(|c| is_closer(c) && run.contains(c));
    let leads_with_closer = candidate.trim_start().starts_with(is_closer);
    let acceptable = is_word_like(candidate)
        && !repeats
        && !leads_with_closer
        && !closes_unopened(candidate)
        && !spans_sentences(candidate);
    acceptable.then(|| candidate.to_string())
}
