use pith_core::{HISTORY_DAYS, blend, ink, next_word_alpha};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;

use super::layout::{Block, Face, Layout, Placed, PlacedRow, Sample, Voice};
use super::model::Model;
use crate::ui::{
    self, Align, CARD_RADIUS, Canvas, Look, Rgb, Role, STROKE, Text, height, inset, key_labels,
    keycaps, measure, paint_backdrop, paint_mark, rect, width,
};

const BADGE_RADIUS: i32 = 10;
const CARET_WIDTH: i32 = 1;
const TILE_TEXT_INSET: i32 = 12;
const BAR_GAP: i32 = 4;
const BAR_MIN: i32 = 2;
const BAR_RADIUS: i32 = 2;
const STRIP_GAP: i32 = 8;

fn voice_color(model: &Model, voice: Voice) -> Rgb {
    let palette = &model.theme.palette;
    match voice {
        Voice::Quiet => palette.text_secondary,
        Voice::Alert => palette.danger,
    }
}

fn paint_sample(canvas: &Canvas, model: &Model, area: RECT, sample: &Sample) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    let plain = Text::new(Role::Body, palette.text);
    let typed_width = measure(scale, &sample.typed, plain, 0).0;
    canvas.text(area, &sample.typed, plain);
    let ghost = ink(Some(palette.text), palette.card, sample.opacity);
    let lead = sample.ghost.len() - sample.ghost.trim_start().len();
    let end = sample.ghost[lead..]
        .find(char::is_whitespace)
        .map_or(sample.ghost.len(), |index| lead + index);
    let (first, rest) = sample.ghost.split_at(end);
    let first_alpha = u32::from(next_word_alpha(ghost.alpha));
    let mut left = area.left + typed_width;
    let line = scale.px(Role::Body.line());
    let caret = rect(
        left,
        area.top + (height(area) - line) / 2,
        scale.px(CARET_WIDTH),
        line,
    );
    canvas.fill_round(caret, 0, palette.text);
    for (text, alpha) in [(first, first_alpha), (rest, u32::from(ghost.alpha))] {
        let style = Text::new(Role::Body, blend(ghost.color, palette.card, alpha));
        let run = RECT { left, ..area };
        canvas.text(run, text, style);
        left += measure(scale, text, style, 0).0;
    }
}

fn paint_row(canvas: &Canvas, model: &Model, row: &PlacedRow) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    if row.divided {
        let line = rect(
            row.bounds.left + scale.px(super::layout::ROW_PAD_X),
            row.bounds.top,
            width(row.bounds) - scale.px(super::layout::ROW_PAD_X),
            scale.px(STROKE),
        );
        canvas.fill_round(line, 0, palette.card_stroke);
    }
    if let Some((bounds, title)) = &row.title {
        canvas.text(*bounds, title, Text::new(Role::Body, palette.text));
    }
    if let Some((bounds, caption, voice)) = &row.caption {
        let style = Text::new(Role::Caption, voice_color(model, *voice)).wrapping();
        canvas.text(*bounds, caption, style);
    }
    if let Some((bounds, badge)) = &row.badge {
        canvas.card(
            *bounds,
            scale.px(BADGE_RADIUS),
            palette.control,
            palette.control_stroke,
        );
        let style = Text::new(Role::Caption, palette.text_secondary).aligned(Align::Center);
        canvas.text(*bounds, badge, style);
    }
    for group in &row.keys {
        keycaps(
            canvas,
            (scale, palette),
            group.origin,
            key_labels(&group.chord),
        );
    }
    for bounds in &row.separators {
        let style = Text::new(Role::Caption, palette.text_secondary).aligned(Align::Center);
        canvas.text(*bounds, super::layout::SEPARATOR, style);
    }
    if let Some((bounds, value)) = &row.value {
        let style = Text::new(Role::Caption, palette.text).aligned(Align::End);
        canvas.text(*bounds, value, style);
    }
    if let Some((area, sample)) = &row.sample {
        paint_sample(canvas, model, *area, sample);
    }
}

fn paint_block(canvas: &Canvas, model: &Model, block: &Block) {
    let palette = &model.theme.palette;
    let radius = model.scale.px(CARD_RADIUS);
    match block {
        Block::Heading(bounds, text) => {
            canvas.text(*bounds, text, Text::new(Role::BodyStrong, palette.text));
        }
        Block::Note(bounds, text, voice) => {
            let style = Text::new(Role::Caption, voice_color(model, *voice)).wrapping();
            canvas.text(*bounds, text, style);
        }
        Block::Panel(bounds) => canvas.card(*bounds, radius, palette.card, palette.card_stroke),
        Block::Card(bounds, rows) => {
            canvas.card(*bounds, radius, palette.card, palette.card_stroke);
            for row in rows {
                paint_row(canvas, model, row);
            }
        }
    }
}

fn paint_sidebar(canvas: &Canvas, model: &Model, layout: &Layout) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    canvas.fill_round(layout.sidebar, 0, model.theme.sidebar());
    let edge = rect(
        layout.sidebar.right - scale.px(STROKE),
        0,
        scale.px(STROKE),
        height(layout.sidebar),
    );
    canvas.fill_round(edge, 0, palette.card_stroke);
    paint_mark(
        canvas,
        (layout.mark.left, layout.mark.top),
        height(layout.mark),
        palette.accent.base,
        palette.mark_ghost,
    );
    canvas.text(
        layout.brand,
        "Pith",
        Text::new(Role::BodyStrong, palette.text),
    );
    let (bounds, version) = &layout.version;
    canvas.text(
        *bounds,
        version,
        Text::new(Role::Caption, palette.text_secondary),
    );
}

pub fn paint_page(canvas: &Canvas, model: &Model) {
    let palette = &model.theme.palette;
    let layout = &model.layout;
    canvas.clear(model.page_fill());
    let (bounds, title) = &layout.title;
    canvas.text(*bounds, title, Text::new(Role::Subtitle, palette.text));
    for block in &layout.blocks {
        paint_block(canvas, model, block);
    }
    paint_sidebar(canvas, model, layout);
    if let Some(thumb) = layout.thumb {
        canvas.fill_round(thumb, width(thumb) / 2, palette.strong_stroke);
    }
}

fn paint_tile(canvas: &Canvas, model: &Model, local: RECT, (value, caption): (&str, &str)) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    let area = inset(local, scale.px(TILE_TEXT_INSET));
    let number_line = scale.px(Role::Title.line());
    let caption_line = scale.px(Role::Caption.line());
    let top = area.top + (height(area) - number_line - caption_line) / 2;
    let number = rect(area.left, top, width(area), number_line);
    canvas.text(number, value, Text::new(Role::Title, palette.text));
    let label = rect(area.left, number.bottom, width(area), caption_line);
    canvas.text(
        label,
        caption,
        Text::new(Role::Caption, palette.text_secondary),
    );
}

fn paint_strip(canvas: &Canvas, model: &Model, local: RECT, caption: &str, days: &[u64]) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    let area = inset(local, scale.px(TILE_TEXT_INSET));
    let line = scale.px(Role::Caption.line());
    let quiet = Text::new(Role::Caption, palette.text_secondary);
    canvas.text(rect(area.left, area.top, width(area), line), caption, quiet);
    let axis = rect(area.left, area.bottom - line, width(area), line);
    canvas.text(axis, "14 days ago", quiet);
    canvas.text(axis, "Today", quiet.aligned(Align::End));
    let bars = RECT {
        top: area.top + line + scale.px(STRIP_GAP),
        bottom: axis.top - scale.px(STRIP_GAP / 2),
        ..area
    };
    let gap = scale.px(BAR_GAP);
    let each = (width(bars) - gap * (HISTORY_DAYS as i32 - 1)) / HISTORY_DAYS as i32;
    let peak = days.iter().copied().max().unwrap_or(0).max(1);
    for (index, words) in days.iter().enumerate() {
        let tall = (height(bars) as u64 * words / peak) as i32;
        let bar_height = tall.max(scale.px(BAR_MIN));
        let left = bars.left + index as i32 * (each + gap);
        let bar = rect(left, bars.bottom - bar_height, each, bar_height);
        let today = index + 1 == days.len();
        let color = match (today, *words) {
            (true, _) => palette.accent.base,
            (false, 0) => palette.track,
            (false, _) => palette.strong_stroke,
        };
        canvas.fill_round(bar, scale.px(BAR_RADIUS), color);
    }
}

fn look(placed: &Placed) -> Option<Look<'_>> {
    let item = &placed.item;
    let label = item.label.as_str();
    match &item.face {
        Face::Toggle => Some(Look::Toggle { on: item.checked }),
        Face::Segment(seam) => Some(Look::Segment {
            selected: item.checked,
            label,
            seam: *seam,
        }),
        Face::Button(tone) => Some(Look::Button { tone: *tone, label }),
        Face::Nav { .. } => Some(Look::Nav {
            selected: item.checked,
            label,
        }),
        Face::Chip(state) => Some(Look::Chip {
            state: *state,
            label,
        }),
        Face::Slider { value, low, high } => Some(Look::Slider {
            fraction: (value - low) as f32 / (high - low).max(1) as f32,
        }),
        Face::Tile(_) | Face::Strip(_) => None,
    }
}

pub fn draw_item(item: &DRAWITEMSTRUCT, model: &Model) {
    let Some(placed) = model.layout.control(item.CtlID as i32) else {
        return;
    };
    let surface = model.surface(placed.backdrop);
    if let Some(look) = look(placed) {
        ui::draw(item, &surface, look);
        return;
    }
    paint_backdrop(item, &surface, |canvas, local| match &placed.item.face {
        Face::Tile(value) => paint_tile(canvas, model, local, (value, &placed.item.label)),
        Face::Strip(days) => paint_strip(canvas, model, local, &placed.item.label, days),
        _ => {}
    });
}
