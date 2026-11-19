use std::cell::RefCell;

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};

const LINE_HEIGHT_RATIO: f32 = 1.25;
const INITIAL_PX: f32 = 14.0;
const FULL: u8 = u8::MAX;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Font<'a> {
    pub(crate) family: Option<&'a str>,
    pub(crate) px: i32,
}

impl Font<'_> {
    pub(crate) fn line_height(&self) -> i32 {
        (self.px as f32 * LINE_HEIGHT_RATIO).ceil() as i32
    }

    fn metrics(&self) -> Metrics {
        Metrics::new(self.px as f32, self.line_height() as f32)
    }
}

pub(crate) struct Mask {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) coverage: Vec<u8>,
}

impl Mask {
    pub(crate) fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            coverage: vec![0; (width * height) as usize],
        }
    }

    pub(crate) fn at(&self, column: i32, row: i32) -> u8 {
        self.coverage[(row * self.width + column) as usize]
    }

    fn cover(&mut self, x: i32, y: i32, width: u32, height: u32, alpha: u8) {
        for row in y.max(0)..(y + height as i32).min(self.height) {
            for column in x.max(0)..(x + width as i32).min(self.width) {
                let cell = &mut self.coverage[(row * self.width + column) as usize];
                *cell = (*cell).max(alpha);
            }
        }
    }
}

struct Shaper {
    fonts: FontSystem,
    cache: SwashCache,
    buffer: Buffer,
}

pub(crate) struct Typesetter(RefCell<Shaper>);

impl Typesetter {
    pub(crate) fn new() -> Self {
        let mut fonts = FontSystem::new();
        let mut buffer = Buffer::new(
            &mut fonts,
            Metrics::new(INITIAL_PX, INITIAL_PX * LINE_HEIGHT_RATIO),
        );
        buffer.set_wrap(Wrap::None);
        buffer.set_size(None, None);
        Self(RefCell::new(Shaper {
            fonts,
            cache: SwashCache::new(),
            buffer,
        }))
    }

    pub(crate) fn measure(&self, font: &Font, text: &str) -> i32 {
        let shaper = &mut *self.0.borrow_mut();
        shaper.shape(font, text);
        shaper
            .buffer
            .layout_runs()
            .map(|run| run.line_w)
            .sum::<f32>()
            .ceil() as i32
    }

    pub(crate) fn draw(&self, font: &Font, text: &str, mask: &mut Mask, x: i32, y: i32) {
        let shaper = &mut *self.0.borrow_mut();
        shaper.shape(font, text);
        shaper.buffer.draw(
            &mut shaper.fonts,
            &mut shaper.cache,
            Color::rgb(FULL, FULL, FULL),
            |dx, dy, width, height, color| mask.cover(x + dx, y + dy, width, height, color.a()),
        );
    }
}

impl Shaper {
    fn shape(&mut self, font: &Font, text: &str) {
        let family = font.family.map_or(Family::SansSerif, Family::Name);
        self.buffer.set_metrics(font.metrics());
        self.buffer
            .set_text(text, &Attrs::new().family(family), Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.fonts, false);
    }
}
