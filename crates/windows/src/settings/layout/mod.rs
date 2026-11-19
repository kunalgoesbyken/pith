mod builder;
mod grid;
mod item;
mod panel;
mod row;
mod sidebar;

use windows::Win32::Foundation::RECT;

use super::model::Model;
use super::pages;
use crate::ui::{Rgb, Role, Scale, rect};

pub use builder::{SEPARATOR, Sheet, Start};
pub use grid::{BADGE_RADIUS, BAR_RADIUS, DEFAULT_CLIENT, MIN_CLIENT};
pub use item::{Class, Face, Item};
pub use row::{PlacedRow, PlacedSample, Row, Sample, Voice};

use grid::{COLUMN_MAX, MIN_THUMB, PANE_BOTTOM, PANE_PAD, SIDEBAR_WIDTH, THUMB_INSET, THUMB_WIDTH};
pub use panel::{strip_rects, tile_rects};

pub const NAV_ID: i32 = 100;
pub const PAGE_ID: i32 = 200;

#[derive(Clone, Debug)]
pub struct Placed {
    pub id: i32,
    pub bounds: RECT,
    pub item: Item,
    pub backdrop: Rgb,
}

#[derive(Clone, Debug)]
pub enum Block {
    Heading(RECT, String),
    Note(RECT, String, Voice),
    Panel(RECT),
    Card(RECT, Vec<PlacedRow>),
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub sidebar: RECT,
    pub mark: RECT,
    pub brand: RECT,
    pub version: (RECT, String),
    pub title: (RECT, String),
    pub blocks: Vec<Block>,
    pub controls: Vec<Placed>,
    pub content_height: i32,
    pub thumb: Option<RECT>,
}

impl Layout {
    pub fn control(&self, id: i32) -> Option<&Placed> {
        self.controls.iter().find(|placed| placed.id == id)
    }
}

fn pane_size(scale: Scale, client_width: i32) -> (i32, i32) {
    let left = scale.px(SIDEBAR_WIDTH) + scale.px(PANE_PAD);
    let width = client_width.max(scale.px(MIN_CLIENT.0));
    let span = (width - left - scale.px(PANE_PAD)).clamp(0, scale.px(COLUMN_MAX));
    (left, span)
}

fn scroll_thumb(
    scale: Scale,
    viewport: (i32, i32),
    content_height: i32,
    scroll: i32,
) -> Option<RECT> {
    let room = content_height - viewport.1;
    if room <= 0 {
        return None;
    }
    let inset = scale.px(THUMB_INSET);
    let track = viewport.1 - 2 * inset;
    let length = (track * viewport.1 / content_height).max(scale.px(MIN_THUMB));
    let top = inset + (track - length) * scroll / room;
    let left = viewport.0 - inset - scale.px(THUMB_WIDTH);
    Some(rect(left, top, scale.px(THUMB_WIDTH), length))
}

pub fn build(model: &Model) -> Layout {
    let scale = model.scale;
    let viewport = model.viewport;
    let palette = &model.theme.palette;
    let (left, span) = pane_size(scale, viewport.0);
    let title_bounds = rect(
        left,
        scale.px(PANE_PAD) - model.scroll,
        span,
        scale.px(Role::Subtitle.line()),
    );
    let mut sheet = Sheet::new(
        scale,
        palette.card,
        (left, span),
        Start {
            top: title_bounds.bottom,
            first_id: PAGE_ID,
        },
    );
    pages::build(model, &mut sheet);
    let built = sheet.finish();
    let content_height = built.bottom + model.scroll + scale.px(PANE_BOTTOM);
    let side = sidebar::build(model);
    let controls = side.controls.into_iter().chain(built.controls).collect();
    Layout {
        sidebar: side.bounds,
        mark: side.mark,
        brand: side.brand,
        version: side.version,
        title: (title_bounds, model.page.label().to_string()),
        blocks: built.blocks,
        controls,
        content_height,
        thumb: scroll_thumb(scale, viewport, content_height, model.scroll),
    }
}
