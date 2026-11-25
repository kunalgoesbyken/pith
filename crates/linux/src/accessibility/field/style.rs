use atspi::proxy::text::TextProxy;
use pith_core::{Rgb, TextStyle};

const FAMILY: &str = "family-name";
const SIZE: &str = "size";
const FOREGROUND: &str = "fg-color";
const POINTS_PER_INCH: f32 = 72.0;
const REFERENCE_DPI: f32 = 96.0;
const WIDE_CHANNEL_SHIFT: u32 = 8;

fn color(value: &str) -> Option<Rgb> {
    let mut channels = value.split(',').map(|channel| {
        channel
            .trim()
            .parse::<u16>()
            .ok()
            .map(|wide| u32::from(wide) >> WIDE_CHANNEL_SHIFT)
    });
    Some(Rgb::from_channels([
        channels.next()??,
        channels.next()??,
        channels.next()??,
    ]))
}

pub(crate) async fn read(text: &TextProxy<'_>, caret: i32) -> TextStyle {
    let Ok((attributes, ..)) = text
        .get_attribute_run(caret.saturating_sub(1).max(0), true)
        .await
    else {
        return TextStyle::default();
    };
    TextStyle {
        family: attributes
            .get(FAMILY)
            .filter(|family| !family.is_empty())
            .cloned(),
        px: attributes
            .get(SIZE)
            .and_then(|size| size.parse::<f32>().ok())
            .filter(|points| *points > 0.0)
            .map(|points| points * REFERENCE_DPI / POINTS_PER_INCH),
        color: attributes.get(FOREGROUND).and_then(|value| color(value)),
    }
}
