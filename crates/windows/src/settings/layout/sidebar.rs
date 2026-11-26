use windows::Win32::Foundation::RECT;

use super::grid::{
    BRAND_GAP, NAV_GAP, NAV_HEIGHT, SIDEBAR_BOTTOM, SIDEBAR_INSET, SIDEBAR_TOP, SIDEBAR_WIDTH,
};
use super::{NAV_ID, Placed};
use crate::settings::layout::Item;
use crate::settings::model::Model;
use crate::settings::page::Page;
use crate::ui::{NAV_PAD, Role, rect};

pub struct Sidebar {
    pub bounds: RECT,
    pub brand: RECT,
    pub version: (RECT, String),
    pub controls: Vec<Placed>,
}

pub fn build(model: &Model) -> Sidebar {
    let scale = model.scale;
    let width = scale.px(SIDEBAR_WIDTH);
    let inset = scale.px(SIDEBAR_INSET);
    let top = scale.px(SIDEBAR_TOP);
    let text_left = inset + scale.px(NAV_PAD);
    let brand_height = scale.px(Role::Wordmark.line());
    let brand = rect(text_left, top, width - text_left - inset, brand_height);
    let first = brand.bottom + scale.px(BRAND_GAP);
    let stride = scale.px(NAV_HEIGHT) + scale.px(NAV_GAP);
    let backdrop = model.theme.palette.sidebar;
    let controls = Page::ALL
        .into_iter()
        .enumerate()
        .map(|(index, page)| Placed {
            id: NAV_ID + index as i32,
            bounds: rect(
                inset,
                first + index as i32 * stride,
                width - 2 * inset,
                scale.px(NAV_HEIGHT),
            ),
            item: Item::nav(page, index == 0, page == model.page),
            backdrop,
        })
        .collect();
    let line = scale.px(Role::Caption.line());
    let version = rect(
        text_left,
        model.viewport.1 - scale.px(SIDEBAR_BOTTOM) - line,
        width - text_left - inset,
        line,
    );
    Sidebar {
        bounds: rect(0, 0, width, model.viewport.1),
        brand,
        version: (version, format!("Pith {}", env!("CARGO_PKG_VERSION"))),
        controls,
    }
}
