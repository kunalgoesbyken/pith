use crate::common;

const TAIL_CHARS: usize = 40;

pub(crate) fn tail(text: &str) -> String {
    common::tail(text, TAIL_CHARS)
}

pub(crate) fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

pub(crate) fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}
