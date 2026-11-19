use pith_core::{Rect, Rgb, median_by_luminance};
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

use super::display::Display;

const SAMPLE_REACH: i32 = 32;
const BYTES_PER_PIXEL: usize = 4;
const PLANES: u32 = u32::MAX;

impl Display {
    pub(crate) fn sample_background(&self, caret: Rect, anchor: i32, rtl: bool) -> Option<Rgb> {
        let connection = &self.connection;
        let focus = self.input_focus()?;
        let area = connection.get_geometry(focus).ok()?.reply().ok()?;
        let origin = connection
            .translate_coordinates(focus, self.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let (left, top) = (i32::from(origin.dst_x), i32::from(origin.dst_y));
        let from = if rtl { anchor - SAMPLE_REACH } else { anchor };
        let x = (from - left).clamp(0, i32::from(area.width));
        let y = (caret.y - top).clamp(0, i32::from(area.height));
        let width = SAMPLE_REACH.min(i32::from(area.width) - x);
        let height = caret.height.min(i32::from(area.height) - y);
        if width < 1 || height < 1 {
            return None;
        }
        let image = connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                focus,
                x as i16,
                y as i16,
                width as u16,
                height as u16,
                PLANES,
            )
            .ok()?
            .reply()
            .ok()?;
        let (pixels, _) = image.data.as_chunks::<BYTES_PER_PIXEL>();
        median_by_luminance(
            pixels
                .iter()
                .map(|&[blue, green, red, _]| {
                    Rgb::from_channels([red.into(), green.into(), blue.into()])
                })
                .collect(),
        )
    }
}
