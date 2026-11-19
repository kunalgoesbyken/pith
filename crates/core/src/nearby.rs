use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;

use crate::budget::{clip_head, clip_tail, estimate_tokens};
use crate::policy::is_prose;
use crate::suffix::is_ignorable;

pub struct WalkLimits {
    pub deadline: Duration,
    pub levels: usize,
    pub siblings: usize,
    pub raw_chars: usize,
    pub descent: usize,
}

pub const WALK: WalkLimits = WalkLimits {
    deadline: Duration::from_millis(60),
    levels: 6,
    siblings: 12,
    raw_chars: 600,
    descent: 3,
};

const ECHO_GRAPHEMES: usize = 12;
const ECHO_TAIL_GRAPHEMES: usize = 200;
const COLONS: [char; 3] = [':', '\u{FF1A}', '\u{FE55}'];
const JOINERS: [char; 2] = ['\u{200C}', '\u{200D}'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueKind {
    FieldLabel,
    FormValue,
    Text,
    Message,
}

impl CueKind {
    fn cap(self) -> usize {
        match self {
            Self::FieldLabel => 24,
            Self::FormValue => 64,
            Self::Text => 48,
            Self::Message => 160,
        }
    }

    fn priority(self) -> u8 {
        match self {
            Self::FieldLabel => 0,
            Self::FormValue => 1,
            Self::Message => 2,
            Self::Text => 3,
        }
    }

    fn clip(self, text: &str) -> &str {
        match self {
            Self::Message => clip_tail(text, self.cap()),
            _ => clip_head(text, self.cap()),
        }
    }

    fn bound(self, text: &str) -> String {
        match self {
            Self::Message => bounded_tail(text),
            _ => bounded_head(text),
        }
    }
}

fn bounded_head(text: &str) -> String {
    text.graphemes(true).take(WALK.raw_chars).collect()
}

fn bounded_tail(text: &str) -> String {
    let mut tail: Vec<&str> = text.graphemes(true).rev().take(WALK.raw_chars).collect();
    tail.reverse();
    tail.concat()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawCue {
    kind: CueKind,
    label: Option<String>,
    text: String,
}

impl RawCue {
    pub fn new(kind: CueKind, label: Option<&str>, text: &str) -> Self {
        Self {
            kind,
            label: label.map(bounded_head),
            text: kind.bound(text),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cue {
    kind: CueKind,
    text: String,
    tokens: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Nearby(Vec<Cue>);

fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| JOINERS.contains(c) || !is_ignorable(*c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn render(label: Option<&str>, text: &str) -> String {
    match label.map(plain).filter(|label| !label.is_empty()) {
        Some(label) if label.ends_with(COLONS) => format!("{label} {text}"),
        Some(label) => format!("{label}: {text}"),
        None => text.to_string(),
    }
}

fn echo_tail(before: &str) -> String {
    let tail: Vec<&str> = before
        .trim()
        .graphemes(true)
        .rev()
        .take(ECHO_TAIL_GRAPHEMES)
        .collect();
    tail.into_iter().rev().collect::<String>().to_lowercase()
}

fn echoes(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.graphemes(true).count() >= ECHO_GRAPHEMES && long.contains(short)
}

#[derive(Default)]
pub struct NearbyCache {
    key: Option<(u64, String)>,
    was_empty: bool,
    cues: Vec<RawCue>,
}

impl NearbyCache {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn stale(&mut self, key: &(u64, String), before_empty: bool) -> bool {
        let stale = self.key.as_ref() != Some(key) || (before_empty && !self.was_empty);
        self.was_empty = before_empty;
        stale
    }

    pub fn fill(&mut self, key: (u64, String), cues: Vec<RawCue>) {
        self.key = Some(key);
        self.cues = cues;
    }

    pub fn cues(&self) -> &[RawCue] {
        &self.cues
    }
}

impl Nearby {
    pub fn shape(raw: Vec<RawCue>, title: &str, before: &str) -> Self {
        let tail = echo_tail(before);
        let mut seen: Vec<String> = vec![plain(title).to_lowercase()];
        seen.retain(|line| !line.is_empty());
        let mut cues = Vec::new();
        for cue in raw {
            let text = plain(&cue.text);
            let folded = text.to_lowercase();
            let repeated = seen.iter().any(|line| line.contains(&folded));
            if text.is_empty() || !is_prose(&text) || repeated || echoes(&folded, &tail) {
                continue;
            }
            let line = render(cue.label.as_deref(), &text);
            let line = cue.kind.clip(&line).trim();
            if line.is_empty() {
                continue;
            }
            seen.push(line.to_lowercase());
            cues.push(Cue {
                kind: cue.kind,
                text: line.to_string(),
                tokens: estimate_tokens(line) + 1,
            });
        }
        cues.reverse();
        Self(cues)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn fit(&self, tokens: usize) -> Vec<&str> {
        let mut by_priority: Vec<usize> = (0..self.0.len()).rev().collect();
        by_priority.sort_by_key(|index| self.0[*index].kind.priority());
        let mut left = tokens;
        let mut kept = vec![false; self.0.len()];
        for index in by_priority {
            let cost = self.0[index].tokens;
            if cost <= left {
                left -= cost;
                kept[index] = true;
            }
        }
        self.0
            .iter()
            .zip(kept)
            .filter(|(_, kept)| *kept)
            .map(|(cue, _)| cue.text.as_str())
            .collect()
    }
}
