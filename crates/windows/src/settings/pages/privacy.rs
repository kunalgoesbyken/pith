use pith_core::Setting;

use super::super::UiEvent;
use super::super::action::Action;
use super::super::layout::{Item, Row, Sheet};
use super::super::model::Model;
use super::switch;
use crate::text::reach;
use crate::ui::Tone;

const OPEN: (&str, Tone) = ("Open", Tone::Standard);

const READS: &str = "Pith reads the text around your caret in the field you are typing in, to write a suggestion. It skips password fields and terminals, and keeps counts, never your text.";

const NEARBY: &str = "Reads the subject, labels and last message beside the box you type in. Never password boxes. Stays on this PC.";

pub(super) fn event_row(
    title: &str,
    caption: &str,
    (label, tone): (&str, Tone),
    event: UiEvent,
) -> Row {
    Row::titled(title)
        .caption(caption)
        .control(Item::button(label, tone, Action::Emit(event)).named(title))
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Privacy");
    sheet.card(vec![
        Row::titled("Where your text goes").caption(&reach::footer(&model.config.reach())),
        Row::titled("What Pith reads").caption(READS),
        Row::titled("Use nearby text")
            .caption(NEARBY)
            .control(switch(
                "Use nearby text",
                model.config.nearby_context,
                Setting::NearbyContext,
            )),
    ]);
    super::style::build(model, sheet);
    sheet.heading("Your data");
    sheet.card(vec![
        event_row(
            "Open data folder",
            "The model and log live on this PC.",
            OPEN,
            UiEvent::OpenDataFolder,
        ),
        event_row(
            "Open settings file",
            "Every setting, including the ones not shown here.",
            OPEN,
            UiEvent::OpenConfigFile,
        ),
        event_row(
            "Reset stats",
            "The counts never leave this PC.",
            ("Reset", Tone::Danger),
            UiEvent::ResetStats,
        ),
        event_row(
            "Delete model and data",
            "Settings are kept. The model downloads again.",
            ("Delete", Tone::Danger),
            UiEvent::DeleteData,
        ),
    ]);
}
