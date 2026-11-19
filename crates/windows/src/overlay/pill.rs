use pith_core::PillStyle;

use crate::ui::Theme;

pub fn style(theme: &Theme) -> PillStyle {
    let palette = theme.palette;
    PillStyle {
        fill: palette.card,
        stroke: palette.accent.base,
        text: palette.text_secondary,
        strong: palette.text,
    }
}
