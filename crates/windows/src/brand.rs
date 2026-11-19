pub use pith_core::{Look, render_mark as render};
use pith_core::{MARK_WIDTH, PAUSED_GREY, Rgb, Scheme, TRAY_FILL, trail_colors};

use crate::ui::{INK, Taskbar};

const TILE_WORD: Rgb = Rgb(0xDAD2C6);
const TILE_DOT: Rgb = Rgb(0x8C867D);
const TILE_GRID: f32 = 256.0;
const TILE_FILL: f32 = MARK_WIDTH / TILE_GRID;

pub const APP: Look = Look {
    plate: Some(INK),
    colors: [Scheme::DARK.accent, TILE_WORD, TILE_DOT],
    fill: TILE_FILL,
};

pub fn tray(taskbar: Taskbar, active: bool) -> Look {
    let (caret, trail) = taskbar.glyph();
    let colors = match (active, taskbar) {
        (true, _) => trail_colors(caret, trail),
        (false, Taskbar::Contrast { dim, .. }) => [dim; 3],
        (false, _) => [PAUSED_GREY; 3],
    };
    Look {
        plate: None,
        colors,
        fill: TRAY_FILL,
    }
}
