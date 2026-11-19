use icu_properties::props::Script;
use icu_properties::{CodePointMapData, CodePointMapDataBorrowed};
use unicode_segmentation::UnicodeSegmentation;

use crate::nearby::plain;
use crate::policy::is_prose;

const MIN_GRAPHEMES: usize = 12;
const MAX_GRAPHEMES: usize = 160;
const MAX_DIGITS_PER_WORD: usize = 2;
const MIXED_WORD_CHARS: usize = 24;
const SCRIPTS: CodePointMapDataBorrowed<'static, Script> = CodePointMapData::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentence(String);

fn is_secret_shaped(word: &str) -> bool {
    let digits = word.chars().filter(|c| c.is_numeric()).count();
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    digits > MAX_DIGITS_PER_WORD
        || (word.chars().count() >= MIXED_WORD_CHARS && digits > 0 && letters > 0)
}

impl Sentence {
    pub fn learnable(text: &str) -> Option<Self> {
        let text = plain(text);
        let length = text.graphemes(true).count();
        let sound = (MIN_GRAPHEMES..=MAX_GRAPHEMES).contains(&length)
            && is_prose(&text)
            && !text.contains('@')
            && !text.contains("://")
            && !text.unicode_words().any(is_secret_shaped);
        sound.then_some(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn family(script: Script) -> Script {
    match script {
        Script::Hiragana | Script::Katakana | Script::Bopomofo => Script::Han,
        other => other,
    }
}

pub fn script_of(text: &str) -> Option<Script> {
    let mut counts: Vec<(Script, usize)> = Vec::new();
    for script in text
        .chars()
        .filter(|c| c.is_alphabetic())
        .map(|c| family(SCRIPTS.get(c)))
        .filter(|script| {
            !matches!(
                *script,
                Script::Common | Script::Inherited | Script::Unknown
            )
        })
    {
        match counts.iter_mut().find(|(seen, _)| *seen == script) {
            Some((_, count)) => *count += 1,
            None => counts.push((script, 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(script, _)| script)
}
