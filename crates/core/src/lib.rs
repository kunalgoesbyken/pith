mod chord;
mod config;
mod engine;
mod ghost;
mod ignore;
mod log;
mod policy;
mod prompt;
mod text;
mod types;

pub use chord::{Chord, Key};
pub use config::{Config, Keymap, ModelPreset, TriggerMode, app_name};
pub use engine::{Action, Engine, Event};
pub use ghost::{
    FADE_ALPHAS, FADE_FRAME_MS, Flow, Ink, Line, flow_bounds, ink, is_rtl, layout, lines_that_fit,
    median_by_luminance, next_word_alpha, opacity_alpha, strong_span,
};
pub use log::{init_log, log};
pub use policy::{SkipReason, check, check_forced};
pub use prompt::{CompletionRequest, build, suggestion};
pub use text::{
    SentenceEnd, clean_completion, continue_window, ends_spaced_word, exceeds_words, next_word,
    sentence_end, split_partial, tail_chars,
};
pub use types::{Completion, FieldSnapshot, Rect, StopReason};

pub trait Completer {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String>;
}
