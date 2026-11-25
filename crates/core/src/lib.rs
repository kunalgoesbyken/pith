mod apps;
mod color;
mod config;
mod context;
mod emphasis;
mod engine;
mod ghost;
mod ignore;
mod input;
mod log;
mod mark;
mod pause;
mod placement;
mod preferences;
mod replacing;
mod scale;
mod scene;
mod shading;
mod stats;
mod status;
mod style;
mod text;
mod tray;
mod types;

pub use apps::hosts::{entry_of, is_generic_entry, is_runtime, is_unlisted_id};
pub use apps::{Access, inserts_by_paste, same_app};
pub use context::budget::estimate_tokens;
pub use color::{CHANNEL_MAX, Rgb, argb};
pub use config::preset::{Hardware, ModelPreset};
pub use config::reach::Reach;
pub use config::setting::{
    AppChoice, Backend, IDLE_UNLOAD_CHOICES, Length, Setting, Suggestions, clamp_opacity,
    thread_choices,
};
pub use config::{Config, Keymap, MAX_GHOST_OPACITY, MIN_GHOST_OPACITY, TriggerMode, app_name};
pub use emphasis::Emphasis;
pub use engine::{Action, Engine, Event};
pub use context::field::{AFTER_CHARS, FieldParts, Surrounding, is_redacted};
pub use ghost::{
    Flow, Ink, Line, blend, fits, flow_bounds, ink, is_rtl, layout, lines_that_fit,
    median_by_luminance, next_word_alpha, opacity_alpha, strong_span,
};
pub use input::chord::{Chord, Key};
pub use input::keys::{Binding, KeyAction, Keys, Press, PressQueue, Registrar};
pub use log::{init_log, log};
pub use mark::{
    Block, Look, MARK_HEIGHT, MARK_WIDTH, PAUSED_GREY, TRAIL, TRAY_FILL, render_mark, trail_colors,
};
pub use context::nearby::{CueKind, Nearby, NearbyCache, RawCue, WALK, WalkLimits};
pub use pause::{Pause, PauseSpan, Pauses, clock_after};
pub use placement::{Placement, anchor_pill, place};
pub use context::policy::{SkipReason, check, check_forced};
pub use preferences::Preferences;
pub use context::prompt::{CompletionRequest, build, suggestion};
pub use replacing::SelectOutcome;
pub use scale::{BASE_DPI, Scale};
pub use scene::{
    Form, KEYCAP_FONT_PX, KeycapLabel, PILL_ELEVATION, PILL_FONT_PX, Placed, Point, Size, Surface,
    fit_cuts, inline_anchor, inline_font_px, keycap_label_origin,
};
pub use shading::{Pill, PillStyle, Scheme, Shading};
pub use stats::{Counts, Day, HISTORY_DAYS, Ledger, Stats};
pub use status::{Model, Status, skip_label};
pub use style::{
    PlainVault, Primer, STYLE_TOKENS, Sample, Sentence, StyleMemory, StyleStore, Vault,
};
pub use text::suffix::{After, Suffix};
pub use text::typo::{Correction, Label, Spelling};
pub use text::{
    LINE_BREAKS, SentenceEnd, clean_completion, continue_window, ends_spaced_word, exceeds_words,
    next_word, sentence_end, split_partial, tail_chars,
};
pub use tray::{
    ForegroundApp, QUIT_LABEL, RESUME_LABEL, TrayCommand, TrayView, auto_paused_label, menu_line,
};
pub use types::{Completion, Drawn, FieldSnapshot, Rect, StopReason, TextStyle, average_logprob};

pub trait Completer {
    fn complete(
        &mut self,
        request: &CompletionRequest,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Completion, String>;
}
