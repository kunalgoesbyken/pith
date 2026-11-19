use icu_properties::props::{
    DefaultIgnorableCodePoint, GeneralCategory, GeneralCategoryGroup, QuotationMark,
    SentenceTerminal, TerminalPunctuation,
};
use icu_properties::{
    CodePointMapData, CodePointMapDataBorrowed, CodePointSetData, CodePointSetDataBorrowed,
};
use serde::{Serialize, Serializer};
use unicode_segmentation::UnicodeSegmentation;

use crate::LINE_BREAKS;
use crate::text::is_word_like;

pub(crate) const CATEGORIES: CodePointMapDataBorrowed<'static, GeneralCategory> =
    CodePointMapData::new();
const TERMINAL: CodePointSetDataBorrowed<'static> = CodePointSetData::new::<TerminalPunctuation>();
pub(crate) const SENTENCE: CodePointSetDataBorrowed<'static> =
    CodePointSetData::new::<SentenceTerminal>();
const QUOTES: CodePointSetDataBorrowed<'static> = CodePointSetData::new::<QuotationMark>();
const IGNORABLE: CodePointSetDataBorrowed<'static> =
    CodePointSetData::new::<DefaultIgnorableCodePoint>();
const CLOSING: GeneralCategoryGroup =
    GeneralCategoryGroup::ClosePunctuation.union(GeneralCategoryGroup::FinalPunctuation);
pub(crate) const OPENING: GeneralCategoryGroup =
    GeneralCategoryGroup::OpenPunctuation.union(GeneralCategoryGroup::InitialPunctuation);
const MARKS: GeneralCategoryGroup = GeneralCategoryGroup::Mark;
const JOINER: char = '\u{200D}';

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
        self.text.chars().all(is_skippable)
    }
}

impl Serialize for After {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
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

fn is_closer(c: char) -> bool {
    let category = CATEGORIES.get(c);
    CLOSING.contains(category)
        || TERMINAL.contains(c)
        || (QUOTES.contains(c) && !OPENING.contains(category))
}

pub(crate) fn is_ignorable(c: char) -> bool {
    IGNORABLE.contains(c)
}

fn is_invisible(c: char) -> bool {
    is_ignorable(c)
        || matches!(
            CATEGORIES.get(c),
            GeneralCategory::NonspacingMark | GeneralCategory::EnclosingMark
        )
}

fn is_skippable(c: char) -> bool {
    c.is_whitespace() || is_invisible(c)
}

pub(crate) fn joins_previous(c: char) -> bool {
    c == JOINER || MARKS.contains(CATEGORIES.get(c))
}

impl Suffix {
    fn of(after: &str) -> Self {
        let line = after.split(LINE_BREAKS).next().unwrap_or_default();
        if line.chars().next().is_some_and(joins_previous) {
            Self::Text
        } else if line.chars().all(is_skippable) {
            Self::Blank
        } else if line.chars().all(|c| is_skippable(c) || is_closer(c)) {
            Self::Closing(
                line.chars()
                    .filter(|c| !is_invisible(*c))
                    .collect::<String>()
                    .trim()
                    .to_string(),
            )
        } else {
            Self::Text
        }
    }
}

struct Brackets {
    unopened: usize,
    unclosed: usize,
}

fn brackets(text: &str) -> Brackets {
    let (mut open, mut unopened) = (0usize, 0usize);
    for c in text.chars() {
        match CATEGORIES.get(c) {
            GeneralCategory::OpenPunctuation => open += 1,
            GeneralCategory::ClosePunctuation => match open.checked_sub(1) {
                Some(rest) => open = rest,
                None => unopened += 1,
            },
            _ => {}
        }
    }
    Brackets {
        unopened,
        unclosed: open,
    }
}

fn spans_sentences(text: &str) -> bool {
    text.trim().unicode_sentences().nth(1).is_some()
}

pub(crate) fn candidates(text: &str, after: &After) -> Vec<String> {
    let words = text
        .split_word_bound_indices()
        .rev()
        .map(|(start, segment)| (start + segment.len(), segment))
        .filter(|(end, segment)| *end < text.len() && is_word_like(segment))
        .map(|(end, _)| end);
    let mut cuts: Vec<String> = std::iter::once(text.len())
        .chain(words)
        .filter_map(|end| fit(&text[..end], after))
        .collect();
    cuts.dedup();
    cuts
}

pub(crate) fn fit(candidate: &str, after: &After) -> Option<String> {
    let run = match after.suffix() {
        Suffix::Blank => return (!candidate.is_empty()).then(|| candidate.to_string()),
        Suffix::Closing(run) => run,
        Suffix::Text => return None,
    };
    let candidate = candidate.trim_end();
    let candidate = candidate
        .strip_suffix(run.as_str())
        .map(str::trim_end)
        .filter(|stripped| brackets(stripped).unclosed <= brackets(candidate).unclosed)
        .unwrap_or(candidate);
    let shape = brackets(candidate);
    let closes = run
        .chars()
        .filter(|c| CATEGORIES.get(*c) == GeneralCategory::ClosePunctuation)
        .count();
    let last = candidate.chars().next_back();
    let stacks = last.is_some_and(|c| {
        (TERMINAL.contains(c) && !SENTENCE.contains(c))
            || (SENTENCE.contains(c) && run.chars().any(|c| TERMINAL.contains(c)))
    });
    let acceptable = is_word_like(candidate)
        && !stacks
        && !candidate.trim_start().starts_with(is_closer)
        && shape.unopened == 0
        && shape.unclosed <= closes
        && !spans_sentences(candidate);
    acceptable.then(|| candidate.to_string())
}
