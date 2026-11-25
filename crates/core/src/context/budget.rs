use unicode_segmentation::UnicodeSegmentation;

use crate::Config;

const HEADROOM_TOKENS: usize = 32;
const CHARS_PER_TOKEN: usize = 4;

fn is_spelled(c: char) -> bool {
    c.is_ascii_alphabetic() || c == ' '
}

pub fn estimate_tokens(text: &str) -> usize {
    let (spelled, wide, total) = text.chars().fold((0, 0, 0), |(spelled, wide, total), c| {
        (
            spelled + usize::from(is_spelled(c)),
            wide + usize::from(!c.is_ascii()),
            total + 1,
        )
    });
    spelled.div_ceil(CHARS_PER_TOKEN) + (total - spelled) + wide.div_ceil(2)
}

pub fn clip_head(text: &str, tokens: usize) -> &str {
    let mut end = 0;
    for (start, segment) in text.split_word_bound_indices() {
        let next = start + segment.len();
        if estimate_tokens(&text[..next]) > tokens {
            break;
        }
        end = next;
    }
    &text[..end]
}

pub fn clip_tail(text: &str, tokens: usize) -> &str {
    let mut start = text.len();
    for (index, _) in text.split_word_bound_indices().rev() {
        if estimate_tokens(&text[index..]) > tokens {
            break;
        }
        start = index;
    }
    &text[start..]
}

pub fn room(config: &Config, used_tokens: usize) -> usize {
    (config.context_tokens as usize)
        .saturating_sub(config.max_tokens + HEADROOM_TOKENS + used_tokens)
}
