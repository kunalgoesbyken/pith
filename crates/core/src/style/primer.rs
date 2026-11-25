use super::memory::StyleMemory;
use super::sentence::script_of;
use crate::context::budget::estimate_tokens;
use crate::{Config, FieldSnapshot, same_app, tail_chars};

pub const EXAMPLES: usize = 3;
pub const STYLE_TOKENS: usize = 96;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Primer(Vec<String>);

impl Primer {
    pub fn select(memory: &StyleMemory, field: &FieldSnapshot, config: &Config) -> Self {
        if !config.uses_style() {
            return Self::default();
        }
        let context = tail_chars(&field.before, config.context_chars);
        let Some(script) = script_of(context) else {
            return Self::default();
        };
        let mut recent: Vec<String> = memory
            .entries()
            .iter()
            .rev()
            .filter(|sample| same_app(&sample.app, &field.app))
            .filter(|sample| script_of(sample.sentence.as_str()) == Some(script))
            .filter(|sample| !context.contains(sample.sentence.as_str()))
            .take(EXAMPLES)
            .map(|sample| sample.sentence.as_str().to_string())
            .collect();
        recent.reverse();
        Self(recent)
    }

    pub fn fit(&self, tokens: usize) -> Vec<&str> {
        let mut left = tokens;
        let mut kept: Vec<&str> = Vec::new();
        for line in self.0.iter().rev() {
            let cost = estimate_tokens(line) + 1;
            if cost > left {
                break;
            }
            left -= cost;
            kept.push(line);
        }
        kept.reverse();
        kept
    }

    pub fn lines(&self) -> &[String] {
        &self.0
    }
}
