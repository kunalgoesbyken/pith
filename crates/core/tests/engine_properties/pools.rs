use pith_core::{Completion, Placement, Rect, StopReason};

use crate::common::{Rng, fragment, text};

pub(crate) const WORDS: &[&str] = &[
    "Please ",
    "sign ",
    "the ",
    "contract ",
    "by ",
    "Friday",
    "नमस्ते ",
    "दोस्त ",
    "你好世界",
    "hello ",
    "مرحبا ",
    "שלום ",
    "e\u{301}cole ",
    "👨\u{200d}👩\u{200d}👧 ",
    "tomorrow ",
    ". ",
    " ",
];

pub(crate) const TYPOS: &[&str] = &[
    "teh",
    "recieve",
    "Teh",
    "NASA",
    "myVar",
    "école",
    "नमस्ते",
    "abc123",
    "ab",
    "wich",
];

pub(crate) const LANGUAGE: &str = "en-US";

pub(crate) const COMPLETIONS: &[&str] = &[
    " contract by Friday.",
    " world and more words here",
    "act",
    " 世界和平",
    " दोस्त कल आऊँगा।",
    "\u{301}x",
    " \u{301}y",
    "  ",
    ".",
    " ok\nnext line",
    "\r\n",
    "",
];

pub(crate) const LOGPROBS: &[Option<f32>] = &[
    Some(-0.5),
    Some(0.0),
    Some(-5.0),
    Some(f32::NEG_INFINITY),
    Some(f32::NAN),
    Some(-1.6),
    None,
];

pub(crate) const STOPS: &[StopReason] = &[
    StopReason::Newline,
    StopReason::Sentence,
    StopReason::MaxTokens,
    StopReason::EndOfText,
    StopReason::LowConfidence,
    StopReason::Cancelled,
];

pub(crate) const APPS: &[&str] = &["notepad", "WINWORD.EXE", "Code", "C:\\x\\mail.exe"];

pub(crate) fn placement(rng: &mut Rng) -> Placement {
    let rect = Rect {
        x: rng.below(200) as i32,
        y: rng.below(200) as i32,
        width: 1,
        height: 16,
    };
    if rng.chance(50) {
        Placement::Inline(rect)
    } else {
        Placement::Anchored(rect)
    }
}

pub(crate) fn prose(rng: &mut Rng) -> String {
    if rng.chance(70) {
        rng.pick(WORDS).to_string()
    } else {
        fragment(rng).to_string()
    }
}

pub(crate) fn completion(rng: &mut Rng) -> Completion {
    let body = if rng.chance(60) {
        rng.pick(COMPLETIONS).to_string()
    } else {
        text(rng, 8)
    };
    Completion {
        text: body,
        mean_logprob: *rng.pick(LOGPROBS),
        tokens: rng.below(20),
        prompt_tokens_evaluated: rng.below(100),
        stop: *rng.pick(STOPS),
    }
}
