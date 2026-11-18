use crate::ui::{AMBER, CITRUS, CREAM, INK, Rgb};

pub const MARK_WIDTH: f32 = 50.0;
pub const MARK_HEIGHT: f32 = 75.0;
pub const HALF_STROKE: f32 = 7.0;
pub const STEM_X: f32 = 7.0;
pub const STEM_TOP: f32 = 7.0;
pub const STEM_BOTTOM: f32 = 68.0;
pub const BOWL_CENTER: (f32, f32) = (25.0, 31.0);
pub const BOWL_RADIUS: f32 = 18.0;

const PAUSED_GREY: Rgb = Rgb(0x8C8680);
const TRAY_FILL: f32 = 0.94;
const TILE_FILL: f32 = 0.6;
const TILE_RADIUS: f32 = 0.22;
const GHOST_ALPHA: f32 = 0.6;
const TILE_GHOST_ALPHA: f32 = 0.45;
const SAMPLES: u32 = 8;

#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub tile: Option<Rgb>,
    pub stem: Rgb,
    pub ghost: Rgb,
    pub ghost_alpha: f32,
    pub fill: f32,
}

pub const ACTIVE: Look = Look {
    tile: None,
    stem: CITRUS,
    ghost: CITRUS,
    ghost_alpha: GHOST_ALPHA,
    fill: TRAY_FILL,
};

pub const PAUSED: Look = Look {
    tile: None,
    stem: PAUSED_GREY,
    ghost: PAUSED_GREY,
    ghost_alpha: GHOST_ALPHA,
    fill: TRAY_FILL,
};

pub const APP: Look = Look {
    tile: Some(INK),
    stem: AMBER,
    ghost: CREAM,
    ghost_alpha: TILE_GHOST_ALPHA,
    fill: TILE_FILL,
};

#[derive(Clone, Copy)]
struct Premultiplied([f32; 4]);

impl Premultiplied {
    const CLEAR: Self = Self([0.0; 4]);

    fn over(self, color: Rgb, alpha: f32) -> Self {
        let [r, g, b] = color.unit_channels();
        let keep = 1.0 - alpha;
        let [dr, dg, db, da] = self.0;
        Self([
            r * alpha + dr * keep,
            g * alpha + dg * keep,
            b * alpha + db * keep,
            alpha + da * keep,
        ])
    }
}

struct Frame {
    size: f32,
    scale: f32,
    origin: (f32, f32),
}

impl Frame {
    fn of(size: u32, fill: f32) -> Self {
        let size = size as f32;
        let scale = size * fill / MARK_HEIGHT;
        Self {
            size,
            scale,
            origin: (
                ((size - MARK_WIDTH * scale) / 2.0).round(),
                ((size - MARK_HEIGHT * scale) / 2.0).round(),
            ),
        }
    }

    fn to_mark(&self, point: (f32, f32)) -> (f32, f32) {
        (
            (point.0 - self.origin.0) / self.scale,
            (point.1 - self.origin.1) / self.scale,
        )
    }
}

fn in_stem((x, y): (f32, f32)) -> bool {
    let along = y.clamp(STEM_TOP, STEM_BOTTOM);
    (x - STEM_X).hypot(y - along) <= HALF_STROKE
}

fn in_bowl((x, y): (f32, f32)) -> bool {
    ((x - BOWL_CENTER.0).hypot(y - BOWL_CENTER.1) - BOWL_RADIUS).abs() <= HALF_STROKE
}

fn in_tile((x, y): (f32, f32), size: f32) -> bool {
    let radius = size * TILE_RADIUS;
    let half = size / 2.0;
    let corner_x = ((x - half).abs() - (half - radius)).max(0.0);
    let corner_y = ((y - half).abs() - (half - radius)).max(0.0);
    corner_x.hypot(corner_y) <= radius
}

fn shade(look: &Look, frame: &Frame, point: (f32, f32)) -> Premultiplied {
    let mut pixel = Premultiplied::CLEAR;
    if let Some(tile) = look.tile
        && in_tile(point, frame.size)
    {
        pixel = pixel.over(tile, 1.0);
    }
    let local = frame.to_mark(point);
    if in_bowl(local) {
        pixel = pixel.over(look.ghost, look.ghost_alpha);
    }
    if in_stem(local) {
        pixel = pixel.over(look.stem, 1.0);
    }
    pixel
}

fn coverage(look: &Look, frame: &Frame, column: u32, row: u32) -> [u8; 4] {
    let mut total = [0.0f32; 4];
    for sample_y in 0..SAMPLES {
        for sample_x in 0..SAMPLES {
            let point = (
                column as f32 + (sample_x as f32 + 0.5) / SAMPLES as f32,
                row as f32 + (sample_y as f32 + 0.5) / SAMPLES as f32,
            );
            let shaded = shade(look, frame, point).0;
            for (sum, value) in total.iter_mut().zip(shaded) {
                *sum += value;
            }
        }
    }
    let [r, g, b, a] = total.map(|sum| sum / (SAMPLES * SAMPLES) as f32);
    if a <= 0.0 {
        return [0; 4];
    }
    [r / a, g / a, b / a, a].map(|channel| (channel * 255.0).round() as u8)
}

pub fn render(size: u32, look: &Look) -> Vec<u8> {
    let frame = Frame::of(size, look.fill);
    (0..size)
        .flat_map(|row| (0..size).map(move |column| (column, row)))
        .flat_map(|(column, row)| coverage(look, &frame, column, row))
        .collect()
}
