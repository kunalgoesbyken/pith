use windows::Win32::Foundation::RECT;

use super::grid::{
    BRAND_GAP, BRAND_HEIGHT, NAV_GAP, NAV_HEIGHT, SIDEBAR_BOTTOM, SIDEBAR_INSET, SIDEBAR_TOP,
    SIDEBAR_WIDTH,
};
use super::{NAV_ID, Placed};
use crate::settings::action::Action;
use crate::settings::layout::Item;
use crate::settings::model::Model;
use crate::settings::page::Page;
use crate::ui::{Role, mark_width, rect};

const MARK_HEIGHT: i32 = 24;
const MARK_GAP: i32 = 12;

pub struct Sidebar {
    pub bounds: RECT,
    pub mark: RECT,
    pub brand: RECT,
    pub version: (RECT, String),
    pub controls: Vec<Placed>,
}

pub fn build(model: &Model) -> Sidebar {
    let scale = model.scale;
    let width = scale.px(SIDEBAR_WIDTH);
    let inset = scale.px(SIDEBAR_INSET);
    let top = scale.px(SIDEBAR_TOP);
    let mark_height = scale.px(MARK_HEIGHT);
    let mark = rect(
        inset + scale.px(MARK_GAP) / 2,
        top + (scale.px(BRAND_HEIGHT) - mark_height) / 2,
        mark_width(mark_height),
        mark_height,
    );
    let brand = rect(
        mark.right + scale.px(MARK_GAP),
        top,
        width - mark.right - scale.px(MARK_GAP) - inset,
        scale.px(BRAND_HEIGHT),
    );
    let first = top + scale.px(BRAND_HEIGHT) + scale.px(BRAND_GAP);
    let stride = scale.px(NAV_HEIGHT) + scale.px(NAV_GAP);
    let backdrop = model.theme.sidebar();
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
            item: Item::nav(
                page.label(),
                index == 0,
                page == model.page,
                Action::Go(page),
            ),
            backdrop,
        })
        .collect();
    let line = scale.px(Role::Caption.line());
    let version = rect(
        mark.left,
        model.viewport.1 - scale.px(SIDEBAR_BOTTOM) - line,
        width - mark.left - inset,
        line,
    );
    Sidebar {
        bounds: rect(0, 0, width, model.viewport.1),
        mark,
        brand,
        version: (version, format!("Pith {}", env!("CARGO_PKG_VERSION"))),
        controls,
    }
}
