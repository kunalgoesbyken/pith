use pith_core::{Length, SLIDER_OPACITY, Setting, Suggestions, TriggerMode};

use super::super::UiEvent;
use super::super::action::Action;
use super::super::layout::{Item, Row, Sample, Sheet};
use super::super::model::Model;
use super::{choice, switch};
use crate::format::number;

fn frequency_label(level: Suggestions) -> &'static str {
    match level {
        Suggestions::Fewer => "Fewer",
        Suggestions::Normal => "Normal",
        Suggestions::More => "More",
    }
}

fn length_label(length: Length) -> &'static str {
    match length {
        Length::Short => "Short",
        Length::Normal => "Normal",
        Length::Long => "Long",
    }
}

fn trigger_label(mode: TriggerMode) -> &'static str {
    match mode {
        TriggerMode::Auto => "As I type",
        TriggerMode::Manual => "Only when I ask",
    }
}

fn suggestion_rows(model: &Model) -> Vec<Row> {
    let config = &model.config;
    let triggers = choice(
        [TriggerMode::Auto, TriggerMode::Manual]
            .map(|mode| (trigger_label(mode).to_string(), mode)),
        Some(config.trigger_mode),
        Setting::TriggerMode,
    );
    let frequency = choice(
        Suggestions::ALL.map(|level| (frequency_label(level).to_string(), level)),
        Suggestions::of(config),
        Setting::Suggestions,
    );
    let length = choice(
        Length::ALL.map(|length| (length_label(length).to_string(), length)),
        Length::of(config),
        Setting::Length,
    );
    vec![
        Row::titled("Suggest as I type")
            .caption("Show the rest of your sentence in grey.")
            .control(switch(
                "Suggest as I type",
                config.enabled,
                Setting::Enabled,
            )),
        Row::titled("When to suggest")
            .caption("Only when I ask waits for your suggest shortcut.")
            .segments(triggers),
        Row::titled("How often")
            .caption("More shows suggestions sooner.")
            .segments(frequency),
        Row::titled("Suggestion length")
            .caption("Shorter or longer suggestions.")
            .segments(length),
    ]
}

fn writing_rows(model: &Model) -> Vec<Row> {
    let config = &model.config;
    vec![
        Row::titled("Fix typos")
            .caption("Offer the right spelling after a misspelled word.")
            .control(switch("Fix typos", config.typo_fix, Setting::TypoFix)),
        Row::titled("Add a space after I keep a word")
            .caption("Saves a keystroke.")
            .control(switch(
                "Add a space after I keep a word",
                config.smart_space,
                Setting::SmartSpace,
            )),
        Row::titled("Pause after I ignore suggestions")
            .caption("Stops suggesting in an app you keep ignoring.")
            .control(switch(
                "Pause after I ignore suggestions",
                config.auto_pause_on(),
                Setting::AutoPause,
            )),
    ]
}

fn ghost_rows(model: &Model) -> Vec<Row> {
    let opacity = model.opacity();
    let (low, high) = SLIDER_OPACITY;
    let shown = opacity.clamp(low, high);
    vec![
        Row::titled("Ghost text opacity")
            .caption("Make suggestions fainter or clearer.")
            .control(Item::slider(
                "Ghost text opacity",
                (i32::from(shown), i32::from(low), i32::from(high)),
                Action::Opacity,
            ))
            .value(&number(u64::from(shown))),
        Row::sample(Sample {
            typed: "Thanks for the update. I will send the files".to_string(),
            ghost: " over by Friday afternoon.".to_string(),
            opacity,
        }),
    ]
}

fn system_rows(model: &Model) -> Vec<Row> {
    let on = model.overview.start_with_windows;
    let toggle = Item::toggle(
        "Start with Windows",
        on,
        Action::Emit(UiEvent::StartWithWindows(!on)),
    );
    vec![
        Row::titled("Start with Windows")
            .caption("Open Pith in the tray when you sign in.")
            .control(toggle),
    ]
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Suggestions");
    sheet.card(suggestion_rows(model));
    sheet.heading("Writing");
    sheet.card(writing_rows(model));
    sheet.heading("Ghost text");
    sheet.card(ghost_rows(model));
    sheet.heading("System");
    sheet.card(system_rows(model));
}
