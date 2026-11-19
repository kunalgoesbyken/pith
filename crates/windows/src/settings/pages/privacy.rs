use super::super::UiEvent;
use super::super::action::Action;
use super::super::layout::{Item, Row, Sheet};
use super::super::model::Model;
use crate::reach;
use crate::ui::Tone;

const READS: &str = "Pith reads the text around your caret in the field you are typing in, to write a suggestion. It skips password fields and terminals, and keeps counts, never your text.";

fn open_row(title: &str, caption: &str, event: UiEvent) -> Row {
    Row::titled(title)
        .caption(caption)
        .control(Item::button("Open", Tone::Standard, Action::Emit(event)).named(title))
}

fn danger_row(title: &str, caption: &str, label: &str, event: UiEvent) -> Row {
    Row::titled(title)
        .caption(caption)
        .control(Item::button(label, Tone::Danger, Action::Emit(event)).named(title))
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Privacy");
    sheet.card(vec![
        Row::titled("Where your text goes").caption(&reach::footer(&model.config.reach())),
        Row::titled("What Pith reads").caption(READS),
    ]);
    sheet.heading("Your data");
    sheet.card(vec![
        open_row(
            "Open data folder",
            "The model and log live on this PC.",
            UiEvent::OpenDataFolder,
        ),
        open_row(
            "Open settings file",
            "Every setting, including the ones not shown here.",
            UiEvent::OpenConfigFile,
        ),
        danger_row(
            "Reset stats",
            "The counts never leave this PC.",
            "Reset",
            UiEvent::ResetStats,
        ),
        danger_row(
            "Delete model and data",
            "Settings are kept. The model downloads again.",
            "Delete",
            UiEvent::DeleteData,
        ),
    ]);
}
