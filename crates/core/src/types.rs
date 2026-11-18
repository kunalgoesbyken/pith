use crate::placement::Placement;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub family: Option<String>,
    pub px: Option<f32>,
    pub color: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldSnapshot {
    pub field_id: u64,
    pub app: String,
    pub window_title: String,
    pub before: String,
    pub after: String,
    pub has_selection: bool,
    pub is_password: bool,
    pub composing: bool,
    pub placement: Option<Placement>,
    pub style: TextStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Newline,
    Sentence,
    WordLimit,
    MaxTokens,
    EndOfText,
    LowConfidence,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub text: String,
    pub mean_logprob: f32,
    pub tokens: usize,
    pub prompt_tokens_evaluated: usize,
    pub stop: StopReason,
}

impl Completion {
    pub fn cancelled(prompt_tokens_evaluated: usize) -> Self {
        Self {
            text: String::new(),
            mean_logprob: f32::NEG_INFINITY,
            tokens: 0,
            prompt_tokens_evaluated,
            stop: StopReason::Cancelled,
        }
    }
}
