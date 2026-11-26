use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;

use super::layout::{DONE_LABEL, HINT, KEY_HINTS, KEYS_NOTE, Layout, TITLE};
use super::model::Model;
use super::step::Glyph;
use super::{DONE_ID, HINT_ID, KEYS_ID, PROGRESS_ID, STATUS_ID};
use crate::ui::{
    self, Align, Canvas, Corner, KEYCAP_HEIGHT, Look, Role, STROKE, Text, Tone, height, keycaps,
    paint_backdrop, rect, width,
};

const KEY_CAPTION_GAP: i32 = 8;
const RING: i32 = 2;
const MARK: f32 = 0.22;

fn glyph(canvas: &Canvas, model: &Model, bounds: RECT) {
    let palette = &model.theme.palette;
    let half = width(bounds) as f32 / 2.0;
    let center = (
        bounds.left as f32 + half,
        bounds.top as f32 + height(bounds) as f32 / 2.0,
    );
    let disc = canvas.shape(bounds, Corner::Pill);
    match model.step.glyph() {
        Glyph::Busy => {
            let ring = model.scale.px(RING);
            canvas.stroke_shape(disc, palette.accent.base, ring);
        }
        Glyph::Ready => {
            canvas.fill_shape(disc, palette.success);
            let unit = half * 2.0 * MARK;
            let tick = [
                (center.0 - unit, center.1),
                (center.0 - unit * 0.3, center.1 + unit * 0.7),
                (center.0 + unit, center.1 - unit * 0.7),
            ];
            let thickness = model.scale.size(1.6);
            canvas.line(tick[0], tick[1], thickness, palette.card);
            canvas.line(tick[1], tick[2], thickness, palette.card);
        }
        Glyph::Failed => {
            canvas.fill_shape(disc, palette.danger);
            let mark = Text::new(Role::BodyStrong, palette.card).aligned(Align::Center);
            canvas.text(bounds, "!", mark);
        }
    }
}

pub fn paint_page(canvas: &Canvas, model: &Model, layout: &Layout) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    canvas.clear(palette.page);
    canvas.text(layout.title, TITLE, Text::new(Role::Display, palette.text));
    canvas.text(
        layout.promise,
        &model.promise,
        Text::new(Role::Body, palette.text_secondary).wrapping(),
    );
    let card = |bounds| canvas.shape(bounds, Corner::Card);
    canvas.card(card(layout.status_card), palette.card, palette.card_stroke);
    glyph(canvas, model, layout.glyph);
    canvas.card(card(layout.keys_card), palette.card, palette.card_stroke);
    canvas.text(
        layout.note,
        KEYS_NOTE,
        Text::new(Role::Caption, palette.text_secondary),
    );
    if model.step.practice_enabled() {
        canvas.card(
            card(layout.practice_card),
            palette.card,
            palette.card_stroke,
        );
    }
    canvas.fill(layout.footer, palette.page);
    let divider = rect(
        layout.footer.left,
        layout.footer.top,
        width(layout.footer),
        scale.px(STROKE),
    );
    canvas.fill(divider, palette.card_stroke);
}

fn paint_keys(canvas: &Canvas, model: &Model, bounds: RECT) {
    let palette = &model.theme.palette;
    let scale = model.scale;
    let columns = KEY_HINTS.len() as i32;
    let column = width(bounds) / columns;
    let content = scale.px(KEYCAP_HEIGHT + KEY_CAPTION_GAP + Role::Caption.line());
    let top = bounds.top + (height(bounds) - content) / 2;
    for (index, (keys, caption)) in KEY_HINTS.iter().enumerate() {
        let left = bounds.left + index as i32 * column;
        keycaps(canvas, (scale, palette), (left, top), keys.iter().copied());
        let caption_bounds = rect(
            left,
            top + scale.px(KEYCAP_HEIGHT + KEY_CAPTION_GAP),
            column,
            scale.px(Role::Caption.line()),
        );
        canvas.text(
            caption_bounds,
            caption,
            Text::new(Role::Caption, palette.text_secondary),
        );
    }
}

fn paint_progress(canvas: &Canvas, model: &Model, bounds: RECT) {
    let palette = &model.theme.palette;
    canvas.fill_shape(canvas.shape(bounds, Corner::Pill), palette.track);
    let permille = model.step.permille().unwrap_or(0) as i32;
    let filled = RECT {
        right: bounds.left + width(bounds) * permille / 1000,
        ..bounds
    };
    if width(filled) > 0 {
        canvas.fill_shape(canvas.shape(filled, Corner::Pill), palette.accent.base);
    }
}

pub fn draw_item(item: &DRAWITEMSTRUCT, model: &Model) {
    let palette = &model.theme.palette;
    let card = model.card_surface();
    match item.CtlID as usize {
        DONE_ID => ui::draw(
            item,
            &model.page_surface(),
            Look::Button {
                tone: Tone::Accent,
                label: DONE_LABEL,
            },
        ),
        STATUS_ID => paint_backdrop(item, &card, |canvas, local| {
            let style = Text::new(Role::BodyStrong, palette.text);
            let style = if model.step.glyph() == Glyph::Failed {
                style.wrapping()
            } else {
                style
            };
            canvas.text(local, &model.step.status(), style);
        }),
        PROGRESS_ID => paint_backdrop(item, &card, |canvas, local| {
            paint_progress(canvas, model, local)
        }),
        HINT_ID => paint_backdrop(item, &card, |canvas, local| {
            canvas.text(
                local,
                HINT,
                Text::new(Role::Caption, palette.text_secondary).wrapping(),
            );
        }),
        KEYS_ID => paint_backdrop(item, &card, |canvas, local| {
            paint_keys(canvas, model, local)
        }),
        _ => {}
    }
}
