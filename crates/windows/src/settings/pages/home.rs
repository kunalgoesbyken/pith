use pith_core::{Access, Counts, KeyAction, Pause, PauseSpan, Setting};

use super::super::action::Action;
use super::super::layout::{Item, Row, Sheet, Voice};
use super::super::model::Model;
use super::super::page::Page;
use super::super::{StatsView, UiEvent};
use super::model::model_name;
use crate::format::{duration, number};
use crate::reach;
use crate::rival::Rival;
use crate::tray::{ForegroundApp, TrayCommand};
use crate::ui::Tone;

fn command(command: TrayCommand) -> Action {
    Action::Emit(UiEvent::Command(command))
}

fn headline(model: &Model) -> &'static str {
    match (model.config.enabled, &model.overview.pause) {
        (false, _) => "Pith is off",
        (true, Some(_)) => "Pith is paused",
        (true, None) => "Pith is on",
    }
}

fn status_row(model: &Model) -> Row {
    let config = &model.config;
    let detail = format!(
        "{} \u{b7} {}",
        model.overview.status,
        model_name(config, model.recommended)
    );
    let toggle = Item::toggle(
        "Suggest as I type",
        config.enabled,
        Action::Apply(Setting::Enabled(!config.enabled)),
    );
    Row::titled(headline(model))
        .caption(&detail)
        .control(toggle)
}

fn pause_row(model: &Model) -> Row {
    match &model.overview.pause {
        None => {
            let buttons = PauseSpan::CHOICES
                .into_iter()
                .filter(|span| *span != PauseSpan::UntilResumed)
                .map(|span| {
                    Item::button(
                        &span.short_label(),
                        Tone::Standard,
                        command(TrayCommand::Pause(span)),
                    )
                    .named(&span.label())
                })
                .collect();
            Row::titled("Pause")
                .caption("Stop suggesting for a while.")
                .buttons(buttons)
        }
        Some(Pause::BatterySaver) => Row::titled("Paused by battery saver")
            .caption("Pith starts again when battery saver turns off."),
        Some(pause) => Row::titled("Paused")
            .caption(match pause {
                Pause::UntilResumed => "Suggestions stay off until you resume.",
                _ => "Suggestions start again when the pause ends.",
            })
            .control(Item::button(
                "Resume",
                Tone::Accent,
                command(TrayCommand::Resume),
            )),
    }
}

fn app_caption(app: &ForegroundApp) -> &'static str {
    match (app.access, app.switchable) {
        (Access::On, _) => "Pith suggests here.",
        (Access::OptedIn, _) => "You turned Pith on in this editor.",
        (Access::OffByDefault, _) => "Code editors start turned off.",
        (Access::Off, true) => "You turned Pith off here.",
        (Access::Off, false) => "Pith never runs here.",
    }
}

fn app_row(model: &Model) -> Row {
    let Some(app) = &model.overview.app else {
        let toggle = Item::toggle("Suggest in the app you were using", false, Action::None);
        return Row::titled("Suggest in the app you were using")
            .caption("Switch to an app, then come back.")
            .control(toggle.disabled());
    };
    let title = format!("Suggest in {}", app.name);
    let on = matches!(app.access, Access::On | Access::OptedIn);
    let toggle = Item::toggle(&title, on, command(TrayCommand::ChangeApp)).enabled(app.switchable);
    Row::titled(&title)
        .caption(app_caption(app))
        .control(toggle)
}

fn rival_row(rival: &Rival) -> Row {
    match rival {
        Rival::TextSuggestions => Row::titled("Windows text suggestions are on")
            .caption("They can show a second suggestion in the same fields.")
            .control(Item::button(
                "Review",
                Tone::Standard,
                Action::Emit(UiEvent::OpenTypingSettings),
            )),
        Rival::App(name) => Row::titled(&format!("{name} is also suggesting text"))
            .caption("Two tools suggesting in one field can clash."),
    }
}

fn chords(model: &Model, binding: KeyAction) -> Vec<String> {
    (0..binding.slots())
        .filter_map(|slot| model.config.chord(binding, slot))
        .map(|chord| chord.to_string())
        .collect()
}

fn key_rows(model: &Model) -> Vec<Row> {
    let mut rows = vec![
        Row::titled("Keep the suggestion").keys(chords(model, KeyAction::AcceptAll)),
        Row::titled("Keep one word").keys(chords(model, KeyAction::AcceptWord)),
    ];
    let trigger = chords(model, KeyAction::Trigger);
    if !trigger.is_empty() {
        rows.push(Row::titled("Suggest now").keys(trigger));
    }
    rows.push(Row::titled("Hide the suggestion").keys(vec![pith_core::Chord::ESCAPE.to_string()]));
    rows.push(
        Row::titled("Use other keys").control(
            Item::button("Change", Tone::Standard, Action::Go(Page::Shortcuts))
                .named("Change shortcuts"),
        ),
    );
    rows
}

fn tiles(model: &Model, counts: &Counts) -> Vec<Item> {
    let kept = counts.accepted + counts.accepted_words;
    let rate = if counts.shown == 0 {
        "\u{2013}".to_string()
    } else {
        format!("{}%", kept * 100 / counts.shown)
    };
    let saved = duration(counts.seconds_saved(model.config.typing_wpm), number);
    [
        (number(counts.words), "Words added today"),
        (rate, "Suggestions kept"),
        (saved, "Time saved today"),
    ]
    .into_iter()
    .map(|(value, caption)| Item::tile(&format!("{caption}: {value}"), &value, caption))
    .collect()
}

fn activity(model: &Model, sheet: &mut Sheet) {
    let StatsView { today, total, days } = &model.overview.stats;
    sheet.heading("Today");
    if today.shown == 0 && total.shown == 0 {
        sheet.card(vec![Row::titled("Nothing yet").caption(
            "Click in any text box, type a sentence and pause for a second.",
        )]);
        return;
    }
    sheet.tiles(tiles(model, today));
    sheet.heading("Last 14 days");
    let caption = format!("Words added per day, {} in total", number(total.words));
    sheet.strip(Item::strip(
        "Words added per day over the last 14 days",
        &caption,
        *days,
    ));
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Status");
    sheet.card(vec![status_row(model), pause_row(model), app_row(model)]);
    if let Some(rival) = &model.rival {
        sheet.heading("Heads up");
        sheet.card(vec![rival_row(rival)]);
    }
    sheet.heading("Keys");
    sheet.card(key_rows(model));
    activity(model, sheet);
    sheet.note(&reach::footer(&model.config.reach()), Voice::Quiet);
}
