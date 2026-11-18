mod config;
mod engine;
mod log;
mod policy;
mod prompt;
mod text;
mod types;

pub use config::{Config, ModelPreset, app_name};
pub use engine::{Action, Engine, Event};
pub use log::{init_log, log};
pub use policy::{SkipReason, check};
pub use prompt::{CompletionRequest, build, suggestion};
pub use text::{
    SentenceEnd, clean_completion, continue_window, exceeds_words, next_word, sentence_end,
    split_partial, tail_chars,
};
pub use types::{Completion, FieldSnapshot, Rect, StopReason};

pub trait Completer {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String>;
}
