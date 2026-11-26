use pith_core::{HISTORY_DAYS, blend, ink, next_word, next_word_alpha};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;

use super::layout::{
    Block, Face, Layout, Placed, PlacedRow, PlacedSample, SEPARATOR, Voice, strip_rects, tile_rects,
};
use super::model::Model;
use crate::text::format::number;
use crate::ui::{
    self, Align, Canvas, Corner, Look, Rgb, Role, Text, key_labels, keycaps, measure,
    paint_backdrop, paint_wordmark,
};

fn voice_color(model: &Model, voice: Voice) -> Rgb {
    let palette = &model.theme.palette;
    match voice {
        Voice::Quiet => palette.text_secondary,
        Voice::Alert => palette.danger,
    }
}

fn paint_sample(canvas: &Canvas, model: &Model, placed: &PlacedSample) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    let sample = &placed.sample;
    canvas.text(
        placed.area,
        &sample.typed,
        Text::new(Role::Body, palette.text),
    );
    canvas.fill(placed.caret, palette.text);
    let ghost = ink(Some(palette.text), palette.card, sample.opacity);
    let (first, rest) = sample.ghost.split_at(next_word(&sample.ghost).len());
    let first_alpha = u32::from(next_word_alpha(ghost.alpha));
    let mut left = placed.caret.left;
    for (text, alpha) in [(first, first_alpha), (rest, u32::from(ghost.alpha))] {
        let style = Text::new(Role::Body, blend(ghost.color, palette.card, alpha));
        let run = RECT {
            left,
            ..placed.area
        };
        canvas.text(run, text, style);
        left += measure(scale, text, style, 0).0;
    }
}

fn paint_row(canvas: &Canvas, model: &Model, row: &PlacedRow) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    if let Some(line) = row.divider {
        canvas.fill(line, palette.card_stroke);
    }
    if let Some((bounds, title)) = &row.title {
        canvas.text(*bounds, title, Text::new(Role::Body, palette.text));
    }
    if let Some((bounds, caption, voice)) = &row.caption {
        let style = Text::new(Role::Caption, voice_color(model, *voice)).wrapping();
        canvas.text(*bounds, caption, style);
    }
    if let Some((bounds, badge)) = &row.badge {
        let pill = canvas.shape(*bounds, Corner::Pill);
        canvas.card(pill, palette.well, palette.control_stroke);
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
        canvas.text(*bounds, SEPARATOR, style);
    }
    if let Some((bounds, value)) = &row.value {
        let style = Text::new(Role::Caption, palette.text).aligned(Align::End);
        canvas.text(*bounds, value, style);
    }
    if let Some(sample) = &row.sample {
        paint_sample(canvas, model, sample);
    }
}

fn paint_block(canvas: &Canvas, model: &Model, block: &Block) {
    let palette = &model.theme.palette;
    match block {
        Block::Heading(bounds, text) => {
            canvas.text(*bounds, text, Text::new(Role::BodyStrong, palette.text));
        }
        Block::Note(bounds, text, voice) => {
            let style = Text::new(Role::Caption, voice_color(model, *voice)).wrapping();
            canvas.text(*bounds, text, style);
        }
        Block::Panel(bounds) => {
            let shape = canvas.shape(*bounds, Corner::Card);
            canvas.card(shape, palette.card, palette.card_stroke);
        }
        Block::Tile(bounds) => {
            let shape = canvas.shape(*bounds, Corner::Card);
            canvas.shadow(shape, model.theme.shadow_over(palette.page));
            canvas.card(shape, palette.card, palette.card_stroke);
        }
        Block::Card(bounds, rows) => {
            let shape = canvas.shape(*bounds, Corner::Card);
            canvas.card(shape, palette.card, palette.card_stroke);
            for row in rows {
                paint_row(canvas, model, row);
            }
        }
    }
}

fn paint_sidebar(canvas: &Canvas, model: &Model, layout: &Layout) {
    let palette = &model.theme.palette;
    canvas.fill(layout.sidebar, palette.sidebar);
    paint_wordmark(canvas, model.scale, layout.brand, palette);
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
    canvas.clear(model.theme.palette.page);
    let (bounds, title) = &layout.title;
    canvas.text(*bounds, title, Text::new(Role::Display, palette.text));
    for block in &layout.blocks {
        paint_block(canvas, model, block);
    }
    paint_sidebar(canvas, model, layout);
    if let Some(thumb) = layout.thumb {
        canvas.fill_shape(canvas.shape(thumb, Corner::Pill), palette.strong_stroke);
    }
}

fn paint_tile(canvas: &Canvas, model: &Model, local: RECT, (value, caption): (&str, &str)) {
    let palette = &model.theme.palette;
    let rects = tile_rects(model.scale, local);
    canvas.text(rects.number, value, Text::new(Role::Stat, palette.text));
    canvas.text(
        rects.caption,
        caption,
        Text::new(Role::Caption, palette.text_secondary),
    );
}

fn paint_strip(
    canvas: &Canvas,
    model: &Model,
    local: RECT,
    caption: &str,
    days: &[u64; HISTORY_DAYS],
) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    let rects = strip_rects(scale, local, days);
    let quiet = Text::new(Role::Caption, palette.text_secondary);
    canvas.text(rects.caption, caption, quiet);
    let oldest = format!("{} days ago", number(HISTORY_DAYS as u64));
    canvas.text(rects.axis, &oldest, quiet);
    canvas.text(rects.axis, "Today", quiet.aligned(Align::End));
    for (index, (bar, words)) in rects.bars.iter().zip(days).enumerate() {
        let today = index + 1 == days.len();
        let color = match (today, *words) {
            (true, _) => palette.accent.base,
            (false, 0) => palette.card_stroke,
            (false, _) => palette.track,
        };
        canvas.fill_shape(canvas.shape(*bar, Corner::Bar), color);
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
        Face::Nav { icon, .. } => Some(Look::Nav {
            selected: item.checked,
            label,
            icon: *icon,
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
