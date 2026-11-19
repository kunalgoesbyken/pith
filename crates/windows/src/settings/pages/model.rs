use pith_core::{Config, IDLE_UNLOAD_CHOICES, ModelPreset, Reach, Setting, thread_choices};

use super::super::UiEvent;
use super::super::action::Action;
use super::super::layout::{Item, Row, Sheet};
use super::super::model::Model;
use super::{choice, switch};
use crate::format::number;
use crate::ui::Tone;

const BYTES_PER_MB: u64 = 1_000_000;

pub fn model_name(config: &Config, recommended: ModelPreset) -> String {
    match (&config.remote_url, &config.model_path) {
        (Some(_), _) => "Remote model".to_string(),
        (None, Some(_)) => "Custom model".to_string(),
        (None, None) => format!("{} model", config.preset(recommended).label()),
    }
}

fn preset_row(model: &Model, preset: ModelPreset) -> Row {
    let in_use = model.config.preset(model.recommended) == preset;
    let size = (pith_infer::preset(preset).bytes + BYTES_PER_MB / 2) / BYTES_PER_MB;
    let caption = format!(
        "{} MB download, about {} MB of memory",
        number(size),
        number(preset.loaded_mib())
    );
    let label = if in_use { "In use" } else { "Use" };
    let name = format!("{label}: {} model", preset.label());
    let button = Item::button(label, Tone::Standard, Action::Apply(Setting::Model(preset)))
        .named(&name)
        .enabled(!in_use);
    let row = Row::titled(preset.label())
        .caption(&caption)
        .control(button);
    if preset == model.recommended {
        row.badge("Recommended for this PC")
    } else {
        row
    }
}

fn external_row(config: &Config) -> Row {
    let title = match config.reach() {
        Reach::Server(host) => format!("Using the server at {host}"),
        Reach::ThisPc if config.remote_url.is_some() => "Using a server on this PC".to_string(),
        Reach::ThisPc => "Using your own model file".to_string(),
    };
    Row::titled(&title)
        .caption("Change it in the settings file.")
        .control(
            Item::button(
                "Open file",
                Tone::Standard,
                Action::Emit(UiEvent::OpenConfigFile),
            )
            .named("Open settings file"),
        )
}

fn idle_label(seconds: u64) -> String {
    match seconds {
        0 => "Never".to_string(),
        seconds => format!("{} min", seconds / 60),
    }
}

fn thread_label(threads: usize) -> String {
    match threads {
        0 => "Auto".to_string(),
        threads => number(threads as u64),
    }
}

fn performance_rows(model: &Model) -> Vec<Row> {
    let config = &model.config;
    let idle = choice(
        IDLE_UNLOAD_CHOICES.map(|seconds| (idle_label(seconds), seconds)),
        Some(config.idle_unload_secs),
        Setting::IdleUnload,
    );
    let threads = choice(
        thread_choices(model.cores)
            .into_iter()
            .map(|threads| (thread_label(threads), threads)),
        Some(config.threads),
        Setting::Threads,
    );
    vec![
        Row::titled("Free memory when idle")
            .caption("Unloads the model after a quiet spell.")
            .segments(idle),
        Row::titled("Pause on battery saver")
            .caption("Stops suggesting while Windows battery saver is on.")
            .control(switch(
                "Pause on battery saver",
                config.pause_on_battery_saver,
                Setting::BatterySaver,
            )),
        Row::titled("CPU threads")
            .caption("Auto picks a sensible number for this PC.")
            .segments(threads),
    ]
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Model");
    if model.config.uses_preset() {
        let rows = ModelPreset::ALL
            .into_iter()
            .map(|preset| preset_row(model, preset))
            .collect();
        sheet.card(rows);
    } else {
        sheet.card(vec![external_row(&model.config)]);
    }
    sheet.heading("Performance");
    sheet.card(performance_rows(model));
}
