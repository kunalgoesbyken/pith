use pith_core::{PillStyle, Scheme};

use crate::ui::{Mode, Theme};

pub fn style(theme: &Theme) -> PillStyle {
    let palette = theme.palette;
    match theme.mode {
        Mode::Contrast => PillStyle {
            fill: palette.card,
            stroke: palette.text,
            text: palette.text,
            strong: palette.text,
            keycap_fill: palette.card,
            keycap_stroke: palette.text,
            keycap_text: palette.text,
        },
        Mode::Dark => PillStyle::of(Scheme::DARK),
        Mode::Light => PillStyle::of(Scheme::LIGHT),
    }
}
