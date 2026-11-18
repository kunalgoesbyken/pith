use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;

use super::layout::{
    Block, DATA_ROWS, FIGURE_COLUMN, GENERAL_ROWS, Layout, OPACITY_TITLE, Row, RowText,
    STATS_RESET, TITLE,
};
use super::model::Model;
use super::{
    ID_DELETE_DATA, ID_ENABLED, ID_OPACITY_DOWN, ID_OPACITY_UP, ID_OPEN_DATA, ID_REMOVE_FIRST,
    ID_RESET_STATS, ID_START, MAX_APP_ROWS, segments,
};
use crate::ui::{
    self, Align, CARD_RADIUS, Canvas, Look, Role, STROKE, Text, Tone, height, measure, rect, width,
};

const MINUS: &str = "\u{2212}";
const PLUS: &str = "+";

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

fn remove_slot(id: i32) -> Option<usize> {
    let slot = usize::try_from(id - ID_REMOVE_FIRST).ok()?;
    (slot < MAX_APP_ROWS).then_some(slot)
}

fn look(model: &Model, id: i32) -> Option<Look<'static>> {
    let toggle = |on| Look::Toggle {
        on,
        label: on_off(on),
    };
    let button = |tone, label| Look::Button { tone, label };
    match id {
        ID_ENABLED => Some(toggle(model.enabled)),
        ID_START => Some(toggle(model.start_with_windows)),
        ID_OPEN_DATA => Some(button(Tone::Standard, "Open")),
        ID_DELETE_DATA => Some(button(Tone::Danger, "Delete")),
        ID_RESET_STATS => Some(button(Tone::Danger, "Reset")),
        ID_OPACITY_DOWN => Some(button(Tone::Standard, MINUS)),
        ID_OPACITY_UP => Some(button(Tone::Standard, PLUS)),
        _ => segments()
            .find(|(segment, ..)| *segment == id)
            .map(|(_, choice, label)| Look::Segment {
                selected: model.selected(*choice),
                label,
            })
            .or_else(|| remove_slot(id).map(|_| button(Tone::Standard, "Remove"))),
    }
}

pub fn name(model: &Model, id: i32) -> String {
    let state = |title: &str, on| format!("{title}, {}", on_off(on).to_lowercase());
    match id {
        ID_ENABLED => state(GENERAL_ROWS[0].0, model.enabled),
        ID_START => state(GENERAL_ROWS[2].0, model.start_with_windows),
        ID_OPEN_DATA => DATA_ROWS[0].0.to_string(),
        ID_DELETE_DATA => DATA_ROWS[1].0.to_string(),
        ID_RESET_STATS => STATS_RESET.0.to_string(),
        ID_OPACITY_DOWN => format!("Lower {}", OPACITY_TITLE.to_lowercase()),
        ID_OPACITY_UP => format!("Raise {}", OPACITY_TITLE.to_lowercase()),
        _ => segments()
            .find(|(segment, ..)| *segment == id)
            .map(|(_, choice, label)| {
                if model.selected(*choice) {
                    format!("{label}, selected")
                } else {
                    label.to_string()
                }
            })
            .or_else(|| {
                let app = model.visible_apps().get(remove_slot(id)?)?;
                Some(format!("Remove {app}"))
            })
            .unwrap_or_default(),
    }
}

pub fn draw_item(item: &DRAWITEMSTRUCT, model: &Model) {
    if let Some(look) = look(model, item.CtlID as i32) {
        ui::draw(item, &model.surface(), look);
    }
}

fn figure_columns(bounds: RECT, column: i32) -> (RECT, RECT) {
    let total = RECT {
        left: bounds.right - column,
        ..bounds
    };
    let today = RECT {
        left: total.left - column,
        right: total.left,
        ..bounds
    };
    (today, total)
}

fn paint_row(canvas: &Canvas, model: &Model, row: &Row, divided: bool) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    if divided {
        let line = rect(
            row.bounds.left,
            row.bounds.top,
            width(row.bounds),
            scale.px(STROKE),
        );
        canvas.fill_round(line, 0, palette.card_stroke);
    }
    let bounds = row.text_bounds;
    match &row.text {
        RowText::Title(title) => {
            canvas.text(bounds, title, Text::new(Role::Body, palette.text));
        }
        RowText::Note(note) => {
            let style = Text::new(Role::Caption, palette.text_secondary).wrapping();
            let (_, note_height) = measure(scale, note, style, width(bounds));
            let top = bounds.top + (height(bounds) - note_height) / 2;
            let centered = RECT {
                top,
                bottom: top + note_height,
                ..bounds
            };
            canvas.text(centered, note, style);
        }
        RowText::Heading(today, total) => {
            let style = Text::new(Role::Caption, palette.text_secondary).aligned(Align::End);
            let (today_bounds, total_bounds) = figure_columns(bounds, scale.px(FIGURE_COLUMN));
            canvas.text(today_bounds, today, style);
            canvas.text(total_bounds, total, style);
        }
        RowText::Figures(title, today, total) => {
            let style = Text::new(Role::Body, palette.text).aligned(Align::End);
            let (today_bounds, total_bounds) = figure_columns(bounds, scale.px(FIGURE_COLUMN));
            let title_bounds = RECT {
                right: today_bounds.left,
                ..bounds
            };
            canvas.text(title_bounds, title, Text::new(Role::Body, palette.text));
            canvas.text(today_bounds, today, style);
            canvas.text(total_bounds, total, style);
        }
        RowText::Pair(title, caption) => {
            let title_line = scale.px(Role::Body.line());
            let caption_line = scale.px(Role::Caption.line());
            let top = bounds.top + (height(bounds) - title_line - caption_line) / 2;
            let title_bounds = RECT {
                top,
                bottom: top + title_line,
                ..bounds
            };
            let caption_bounds = RECT {
                top: title_bounds.bottom,
                bottom: title_bounds.bottom + caption_line,
                ..bounds
            };
            canvas.text(title_bounds, title, Text::new(Role::Body, palette.text));
            canvas.text(
                caption_bounds,
                caption,
                Text::new(Role::Caption, palette.text_secondary),
            );
        }
    }
}

fn paint_block(canvas: &Canvas, model: &Model, block: &Block) {
    let palette = &model.theme.palette;
    if let Some((bounds, label)) = block.label {
        canvas.text(bounds, label, Text::new(Role::BodyStrong, palette.text));
    }
    let radius = model.scale.px(CARD_RADIUS);
    canvas.card(block.card, radius, palette.card, palette.card_stroke);
    for (index, row) in block.rows.iter().enumerate() {
        paint_row(canvas, model, row, index > 0);
    }
}

pub fn paint_page(canvas: &Canvas, model: &Model, layout: &Layout) {
    let palette = &model.theme.palette;
    canvas.clear(model.page());
    canvas.text(layout.title, TITLE, Text::new(Role::Subtitle, palette.text));
    canvas.text(
        layout.status,
        &model.status,
        Text::new(Role::Caption, palette.text_secondary),
    );
    for block in &layout.blocks {
        paint_block(canvas, model, block);
    }
    canvas.text(
        layout.footer,
        &model.footer,
        Text::new(Role::Caption, palette.text_secondary),
    );
}
