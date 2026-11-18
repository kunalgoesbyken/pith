#![allow(dead_code)]

use unicode_segmentation::UnicodeSegmentation;

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const FRAGMENTS: &[&str] = &[
    "hello",
    "the",
    "contract",
    "don't",
    "x",
    "3.14",
    "a_b",
    "Friday",
    "你好",
    "世界",
    "こんにちは",
    "カタカナ",
    "한국어",
    "नमस्ते",
    "क्षत्रिय",
    "दोस्त",
    "कल",
    "e\u{301}",
    "n\u{303}",
    "ệ",
    "a\u{301}\u{323}",
    "مرحبا",
    "שלום",
    "\u{200f}",
    "\u{200e}",
    "👋",
    "👨\u{200d}👩\u{200d}👧",
    "🇮🇳",
    "👍🏽",
    "\u{200d}",
    "\u{fe0f}",
    "\u{301}",
    "\u{94d}",
    "\u{200b}",
    ".",
    "!",
    "?",
    "。",
    "！",
    "।",
    "॥",
    "؟",
    "…",
    "...",
    ",",
    ";",
    "-",
    "(",
    ")",
    "\"",
    "'",
    " ",
    "  ",
    "\t",
    "\u{a0}",
    "\u{2003}",
    "\u{3000}",
    "\n",
    "\r\n",
    "\r",
    "\n\n",
    "\u{2028}",
    "0",
    "42",
    "١٢٣",
    "",
];

pub fn text(rng: &mut Rng, max_pieces: usize) -> String {
    (0..rng.below(max_pieces + 1))
        .map(|_| *rng.pick(FRAGMENTS))
        .collect()
}

pub fn fragment(rng: &mut Rng) -> &'static str {
    rng.pick(FRAGMENTS)
}

pub fn is_boundary(text: &str, index: usize) -> bool {
    index == text.len() || text.grapheme_indices(true).any(|(start, _)| start == index)
}
