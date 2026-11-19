use pith_core::Setting;
use unicode_segmentation::UnicodeSegmentation;

use super::super::UiEvent;
use super::super::action::Action;
use super::super::layout::{Item, Row, Sheet, Voice};
use super::super::model::Model;
use super::privacy::event_row;
use super::switch;
use crate::settings::StyleLine;
use crate::ui::Tone;

const ROWS_PER_PAGE: usize = 6;
const TITLE_GRAPHEMES: usize = 56;

const LEARN: &str = "Remembers sentences where you used a suggestion, so suggestions sound more like you. Encrypted on this PC, never sent, and only used in the app they came from. Never learned from password boxes, blocked apps or while Pith is paused.";

const CARRY: &str = "Sentences carry over between conversations in the same app. Turning this off stops learning and stops using what was learned. It stays here until you remove it.";

fn clipped(text: &str) -> String {
    let head: String = text.graphemes(true).take(TITLE_GRAPHEMES).collect();
    if head.len() == text.len() {
        head
    } else {
        format!("{head}\u{2026}")
    }
}

fn sentence_row(line: &StyleLine) -> Row {
    let title = clipped(&line.text);
    let forget = Item::button(
        "Forget",
        Tone::Standard,
        Action::Emit(UiEvent::ForgetSentence {
            app: line.app.clone(),
            text: line.text.clone(),
        }),
    )
    .named(&format!("Forget {title} in {}", line.app));
    Row::titled(&title).caption(&line.app).control(forget)
}

fn pager_row(page: usize, pages: usize) -> Row {
    let turn = |label: &str, target: usize, enabled: bool| {
        Item::button(label, Tone::Standard, Action::StylePage(target)).enabled(enabled)
    };
    Row::titled(&format!("Page {} of {pages}", page + 1)).buttons(vec![
        turn("Previous", page.saturating_sub(1), page > 0),
        turn("Next", page + 1, page + 1 < pages),
    ])
}

fn learned_rows(model: &Model) -> Vec<Row> {
    let lines = &model.overview.style;
    if lines.is_empty() {
        return vec![Row::titled("Nothing learned yet").caption(
            "Sentences appear here after you keep a suggestion and finish the sentence.",
        )];
    }
    let pages = lines.len().div_ceil(ROWS_PER_PAGE);
    let page = model.style_page.min(pages - 1);
    let mut rows: Vec<Row> = lines
        .iter()
        .skip(page * ROWS_PER_PAGE)
        .take(ROWS_PER_PAGE)
        .map(sentence_row)
        .collect();
    if pages > 1 {
        rows.push(pager_row(page, pages));
    }
    rows.push(event_row(
        "Forget everything",
        &format!("Removes all {} remembered sentences.", lines.len()),
        ("Forget all", Tone::Danger),
        UiEvent::ForgetStyle,
    ));
    rows
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Your style");
    sheet.card(vec![
        Row::titled("Learn my writing style")
            .caption(LEARN)
            .control(switch(
                "Learn my writing style",
                model.config.style_memory,
                Setting::StyleMemory,
            )),
    ]);
    sheet.heading("What Pith learned");
    sheet.card(learned_rows(model));
    sheet.note(CARRY, Voice::Quiet);
}
