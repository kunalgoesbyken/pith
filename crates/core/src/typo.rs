use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::is_rtl;
use crate::suffix::{CATEGORIES, OPENING, SENTENCE, joins_previous};
use crate::text::{LINE_BREAKS, is_inline_space};

const MIN_WORD_GRAPHEMES: usize = 3;
const MAX_WORD_GRAPHEMES: usize = 32;
const MAX_DECLINED: usize = 64;
const ZERO_WIDTH_NON_JOINER: char = '\u{200C}';
const ARROW_FORWARD: char = '\u{2192}';
const ARROW_BACKWARD: char = '\u{2190}';

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spelling {
    Correct,
    Autocorrect(String),
    Suggested(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correction {
    pub word: String,
    pub replacement: String,
    pub boundary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub text: String,
    pub strong: Range<usize>,
}

impl Correction {
    pub fn current(&self) -> String {
        format!("{}{}", self.word, self.boundary)
    }

    pub fn corrected(&self) -> String {
        format!("{}{}", self.replacement, self.boundary)
    }

    pub fn label(&self) -> Label {
        let arrow = if is_rtl(&self.word) {
            ARROW_BACKWARD
        } else {
            ARROW_FORWARD
        };
        let text = format!("{} {arrow} {}", self.word, self.replacement);
        let start = text.len() - self.replacement.len();
        Label {
            strong: start..text.len(),
            text,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Finished {
    word: String,
    boundary: String,
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic() || joins_previous(c) || c == ZERO_WIDTH_NON_JOINER
}

fn opens_a_word(head: &str) -> bool {
    head.chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || OPENING.contains(CATEGORIES.get(c)))
}

fn starts_a_sentence(head: &str) -> bool {
    head.trim_end_matches(is_inline_space)
        .chars()
        .next_back()
        .is_none_or(|c| LINE_BREAKS.contains(&c) || SENTENCE.contains(c))
}

fn one_edit(a: &[&str], b: &[&str]) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    match long.len() - short.len() {
        0 => {
            let differing: Vec<usize> = (0..a.len()).filter(|&at| a[at] != b[at]).collect();
            match differing.as_slice() {
                [_] => true,
                [first, second] => {
                    second - first == 1 && a[*first] == b[*second] && a[*second] == b[*first]
                }
                _ => false,
            }
        }
        1 => {
            let shared = short.iter().zip(long).take_while(|(x, y)| x == y).count();
            short[shared..] == long[shared + 1..]
        }
        _ => false,
    }
}

impl Finished {
    pub(crate) fn of(previous: &str, current: &str) -> Option<Self> {
        let boundary = current.strip_prefix(previous)?;
        let mut graphemes = boundary.graphemes(true);
        let (Some(only), None) = (graphemes.next(), graphemes.next()) else {
            return None;
        };
        if !only.chars().all(is_inline_space) {
            return None;
        }
        let start = previous
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_letter(*c))
            .last()?
            .0;
        let (head, word) = previous.split_at(start);
        let mut letters = word.chars();
        let first = letters.next().filter(|c| c.is_alphabetic())?;
        let inside_caps = letters.any(char::is_uppercase);
        let graphemes = word.graphemes(true).count();
        let acceptable = opens_a_word(head)
            && (MIN_WORD_GRAPHEMES..=MAX_WORD_GRAPHEMES).contains(&graphemes)
            && !inside_caps
            && (!first.is_uppercase() || starts_a_sentence(head));
        acceptable.then(|| Self {
            word: word.to_string(),
            boundary: boundary.to_string(),
        })
    }

    pub(crate) fn word(&self) -> &str {
        &self.word
    }

    pub(crate) fn correct(self, spelling: Spelling) -> Option<Correction> {
        let replacement = match spelling {
            Spelling::Correct => return None,
            Spelling::Autocorrect(replacement) => replacement,
            Spelling::Suggested(suggestion) => {
                let typed: Vec<&str> = self.word.graphemes(true).collect();
                let offered: Vec<&str> = suggestion.graphemes(true).collect();
                if !one_edit(&typed, &offered) {
                    return None;
                }
                suggestion
            }
        };
        let usable = !replacement.is_empty()
            && replacement != self.word
            && !replacement.contains(LINE_BREAKS);
        usable.then_some(Correction {
            word: self.word,
            replacement,
            boundary: self.boundary,
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct Declined(Vec<String>);

impl Declined {
    pub(crate) fn contains(&self, word: &str) -> bool {
        self.0.iter().any(|declined| declined == word)
    }

    pub(crate) fn insert(&mut self, word: &str) {
        if self.contains(word) {
            return;
        }
        if self.0.len() == MAX_DECLINED {
            self.0.remove(0);
        }
        self.0.push(word.to_string());
    }
}
