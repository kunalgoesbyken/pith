use pith_core::HISTORY_DAYS;
use windows::Win32::Foundation::RECT;

use super::grid::{BAR_GAP, BAR_MIN, STRIP_GAP, TILE_TEXT_INSET};
use crate::ui::{Role, Scale, height, inset, rect, width};

#[derive(Clone, Copy, Debug)]
pub struct TileRects {
    pub number: RECT,
    pub caption: RECT,
}

#[derive(Clone, Debug)]
pub struct StripRects {
    pub caption: RECT,
    pub axis: RECT,
    pub bars: Vec<RECT>,
}

pub fn tile_rects(scale: Scale, local: RECT) -> TileRects {
    let area = inset(local, scale.px(TILE_TEXT_INSET));
    let number_line = scale.px(Role::Stat.line());
    let caption_line = scale.px(Role::Caption.line());
    let top = area.top + (height(area) - number_line - caption_line) / 2;
    let number = rect(area.left, top, width(area), number_line);
    let caption = rect(area.left, number.bottom, width(area), caption_line);
    TileRects { number, caption }
}

pub fn strip_rects(scale: Scale, local: RECT, days: &[u64; HISTORY_DAYS]) -> StripRects {
    let area = inset(local, scale.px(TILE_TEXT_INSET));
    let line = scale.px(Role::Caption.line());
    let caption = rect(area.left, area.top, width(area), line);
    let axis = rect(area.left, area.bottom - line, width(area), line);
    let plot = RECT {
        top: area.top + line + scale.px(STRIP_GAP),
        bottom: axis.top - scale.px(STRIP_GAP / 2),
        ..area
    };
    let gap = scale.px(BAR_GAP);
    let each = (width(plot) - gap * (HISTORY_DAYS as i32 - 1)) / HISTORY_DAYS as i32;
    let peak = days.iter().copied().max().unwrap_or(0).max(1);
    let bars = days
        .iter()
        .enumerate()
        .map(|(index, words)| {
            let tall = (height(plot) as u64 * words / peak) as i32;
            let bar_height = tall.max(scale.px(BAR_MIN));
            let left = plot.left + index as i32 * (each + gap);
            rect(left, plot.bottom - bar_height, each, bar_height)
        })
        .collect();
    StripRects {
        caption,
        axis,
        bars,
    }
}
