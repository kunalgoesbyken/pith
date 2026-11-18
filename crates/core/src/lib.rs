mod config;
mod engine;
mod log;
mod policy;
mod prompt;
mod text;
mod types;

pub use config::Config;
pub use engine::{Action, Engine, Event};
pub use log::{init_log, log};
pub use policy::{SkipReason, check};
pub use prompt::{CompletionRequest, build};
pub use text::{clean_completion, next_word, split_partial, tail_chars};
pub use types::{Completion, FieldSnapshot, Rect, StopReason};

pub trait Completer {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String>;
}
