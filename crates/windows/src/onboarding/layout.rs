use windows::Win32::Foundation::RECT;

use super::model::Model;
use super::step::Glyph;
use crate::ui::{BUTTON_HEIGHT, Role, Text, expand, inset, measure, rect};

pub const TITLE: &str = "Welcome to Pith";
pub const HINT: &str = "Try it. Click at the end of the sentence below and pause for a second. The rest of the sentence appears in grey.";
pub const KEYS_NOTE: &str = "With no suggestion showing, Tab is a normal Tab.";
pub const KEYS_NAME: &str = "Tab keeps the suggestion. Shift+Tab or Ctrl+Right keeps one word. Esc hides it. With no suggestion showing, Tab is a normal Tab.";
pub const KEY_HINTS: [(&[&str], &str); 3] = [
    (&["Tab"], "keeps the suggestion"),
    (&["Ctrl", "\u{2192}"], "keeps one word"),
    (&["Esc"], "hides it"),
];
pub const DONE_LABEL: &str = "Done";

const WIDTH: i32 = 560;
const MARGIN: i32 = 28;
const PAD: i32 = 16;
const GAP: i32 = 16;
const HERO_GAP: i32 = 20;
const PROMISE_GAP: i32 = 4;
const NOTE_GAP: i32 = 8;
const GLYPH: i32 = 20;
const GLYPH_GAP: i32 = 12;
const BAR: i32 = 4;
const BAR_GAP: i32 = 12;
const STATUS_MIN: i32 = 52;
const KEYS_HEIGHT: i32 = 84;
const KEYS_INSET: i32 = 12;
const EDIT_HEIGHT: i32 = 72;
const EDIT_GAP: i32 = 8;
const FOOTER: i32 = 64;
pub const DONE_WIDTH: i32 = 96;

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub client: RECT,
    pub title: RECT,
    pub promise: RECT,
    pub status_card: RECT,
    pub glyph: RECT,
    pub status: RECT,
    pub progress: RECT,
    pub keys_card: RECT,
    pub keys: RECT,
    pub note: RECT,
    pub practice_card: RECT,
    pub hint: RECT,
    pub practice: RECT,
    pub footer: RECT,
    pub done: RECT,
}

pub fn build(model: &Model) -> Layout {
    let scale = model.scale;
    let px = |logical| scale.px(logical);
    let left = px(MARGIN);
    let inner = px(WIDTH) - 2 * left;
    let mut y = px(MARGIN);
    let title = rect(left, y, inner, px(Role::Title.line()));
    y = title.bottom;
    let (_, promise_height) = measure(
        scale,
        &model.promise,
        Text::new(Role::Body, model.theme.palette.text_secondary).wrapping(),
        inner,
    );
    y += px(PROMISE_GAP);
    let promise = rect(left, y, inner, promise_height);
    y = promise.bottom + px(HERO_GAP);

    let text_left = left + px(PAD + GLYPH + GLYPH_GAP);
    let text_width = left + inner - px(PAD) - text_left;
    let wraps = model.view.glyph == Glyph::Failed;
    let style = Text::new(Role::BodyStrong, model.theme.palette.text);
    let text_height = if wraps {
        measure(scale, &model.view.status, style.wrapping(), text_width).1
    } else {
        px(Role::BodyStrong.line())
    };
    let bar = model.view.permille.is_some();
    let bar_extra = if bar { px(BAR_GAP + BAR) } else { 0 };
    let card_height = (2 * px(PAD) + text_height + bar_extra).max(px(STATUS_MIN));
    let status_card = rect(left, y, inner, card_height);
    let status = rect(text_left, y + px(PAD), text_width, text_height);
    let glyph_size = px(GLYPH);
    let glyph = rect(left + px(PAD), y + px(PAD), glyph_size, glyph_size);
    let progress = rect(
        text_left,
        status.bottom + px(BAR_GAP),
        text_width,
        if bar { px(BAR) } else { 0 },
    );
    y = status_card.bottom + px(GAP);

    let keys_card = rect(left, y, inner, px(KEYS_HEIGHT));
    let keys = inset(keys_card, px(KEYS_INSET));
    y = keys_card.bottom + px(NOTE_GAP);
    let note = rect(left, y, inner, px(Role::Caption.line()));
    y = note.bottom + px(GAP);

    let pad = px(PAD);
    let hint_width = inner - 2 * pad;
    let (_, hint_height) = measure(
        scale,
        HINT,
        Text::new(Role::Caption, model.theme.palette.text_secondary).wrapping(),
        hint_width,
    );
    let hint = rect(left + pad, y + pad, hint_width, hint_height);
    let practice = rect(
        left + pad,
        hint.bottom + px(EDIT_GAP),
        hint_width,
        px(EDIT_HEIGHT),
    );
    let practice_card = rect(left, y, inner, practice.bottom + pad - y);
    y = practice_card.bottom + px(GAP);

    let footer = rect(0, y, px(WIDTH), px(FOOTER));
    let button_height = px(BUTTON_HEIGHT);
    let done = expand(
        rect(
            left + inner - px(DONE_WIDTH),
            footer.top + (px(FOOTER) - button_height) / 2,
            px(DONE_WIDTH),
            button_height,
        ),
        scale,
    );
    Layout {
        client: rect(0, 0, px(WIDTH), footer.bottom),
        title,
        promise,
        status_card,
        glyph,
        status,
        progress,
        keys_card,
        keys,
        note,
        practice_card,
        hint,
        practice,
        footer,
        done,
    }
}
