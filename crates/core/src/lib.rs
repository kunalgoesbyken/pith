mod apps;
mod budget;
mod chord;
mod color;
mod config;
mod emphasis;
mod engine;
mod field;
mod ghost;
#[cfg(target_os = "linux")]
mod hosts;
#[cfg(not(target_os = "linux"))]
#[path = "hosts_none.rs"]
mod hosts;
mod ignore;
mod keybind;
mod keys;
mod log;
mod nearby;
mod pause;
mod placement;
mod policy;
mod preset;
mod prompt;
mod replacing;
mod scale;
mod scene;
mod setting;
mod shading;
mod stats;
mod status;
mod stored;
mod suffix;
mod text;
mod types;
mod typo;

pub use apps::{Access, inserts_by_paste, same_app};
pub use budget::estimate_tokens;
pub use chord::{Chord, Key};
pub use color::{CHANNEL_MAX, Rgb, argb};
pub use config::{
    Config, Keymap, MAX_GHOST_OPACITY, MIN_GHOST_OPACITY, Reach, TriggerMode, app_name,
};
pub use emphasis::Emphasis;
pub use engine::{Action, Engine, Event};
pub use field::{AFTER_CHARS, FieldParts, Surrounding, is_redacted};
pub use ghost::{
    Flow, Ink, Line, blend, fits, flow_bounds, ink, is_rtl, layout, lines_that_fit,
    median_by_luminance, next_word_alpha, opacity_alpha, strong_span,
};
pub use hosts::{entry_of, is_generic_entry, is_runtime, is_unlisted_id};
pub use keys::{Binding, KeyAction, Keys, Press, PressQueue, Registrar};
pub use log::{init_log, log};
pub use nearby::{CueKind, Nearby, NearbyCache, RawCue, WALK, WalkLimits};
pub use pause::{Pause, PauseSpan, Pauses, clock_after};
pub use placement::{Placement, anchor_pill, place};
pub use policy::{SkipReason, check, check_forced};
pub use preset::{Hardware, ModelPreset};
pub use prompt::{CompletionRequest, build, suggestion};
pub use replacing::SelectOutcome;
pub use scale::{BASE_DPI, Scale};
pub use scene::{
    Form, KEYCAP_FONT_PX, KeycapLabel, PILL_ELEVATION, PILL_FONT_PX, Placed, Point, Size, Surface,
    fit_cuts, inline_anchor, inline_font_px, keycap_label_origin,
};
pub use setting::{
    AppChoice, Backend, IDLE_UNLOAD_CHOICES, Length, Setting, Suggestions, clamp_opacity,
    thread_choices,
};
pub use shading::{Pill, PillStyle, Scheme, Shading};
pub use stats::{Counts, Day, HISTORY_DAYS, Ledger, Stats};
pub use status::{Model, Status, skip_label};
pub use suffix::{After, Suffix};
pub use text::{
    LINE_BREAKS, SentenceEnd, clean_completion, continue_window, ends_spaced_word, exceeds_words,
    next_word, sentence_end, split_partial, tail_chars,
};
pub use types::{Completion, Drawn, FieldSnapshot, Rect, StopReason, TextStyle, average_logprob};
pub use typo::{Correction, Label, Spelling};

pub trait Completer {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String>;
}
