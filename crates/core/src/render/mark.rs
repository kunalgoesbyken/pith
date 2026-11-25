use crate::{CHANNEL_MAX, Rgb};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

impl Block {
    pub const fn capsule(x: f32, y: f32, width: f32, height: f32) -> Self {
        let radius = if width < height { width } else { height } / 2.0;
        Self {
            x,
            y,
            width,
            height,
            radius,
        }
    }

    pub const fn square(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
            radius: 0.0,
        }
    }

    pub fn scaled(self, scale: f32, origin: (f32, f32)) -> Self {
        Self {
            x: origin.0 + self.x * scale,
            y: origin.1 + self.y * scale,
            width: self.width * scale,
            height: self.height * scale,
            radius: self.radius * scale,
        }
    }

    pub fn contains(&self, (px, py): (f32, f32)) -> bool {
        let half = (self.width / 2.0, self.height / 2.0);
        let dx = (px - self.x - half.0).abs() - (half.0 - self.radius);
        let dy = (py - self.y - half.1).abs() - (half.1 - self.radius);
        dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) <= self.radius
    }
}

pub fn trail_colors(caret: Rgb, trail: Rgb) -> [Rgb; 3] {
    [caret, trail, trail]
}

pub const MARK_WIDTH: f32 = 174.0;
pub const MARK_HEIGHT: f32 = 164.0;

pub const TRAIL: [Block; 3] = [
    Block::capsule(0.0, 0.0, 28.0, 164.0),
    Block::capsule(50.0, 68.0, 80.0, 28.0),
    Block::capsule(146.0, 68.0, 28.0, 28.0),
];

const SNAPPED: [[Block; 3]; 6] = [
    [
        Block::square(0, 0, 2, 10),
        Block::square(3, 4, 4, 2),
        Block::square(8, 4, 2, 2),
    ],
    [
        Block::square(0, 0, 2, 12),
        Block::square(4, 5, 5, 2),
        Block::square(11, 5, 2, 2),
    ],
    [
        Block::square(0, 0, 3, 16),
        Block::square(5, 6, 6, 3),
        Block::square(13, 6, 3, 3),
    ],
    [
        Block::square(0, 0, 3, 18),
        Block::square(6, 7, 8, 3),
        Block::square(17, 7, 3, 3),
    ],
    [
        Block::square(0, 0, 4, 22),
        Block::square(7, 9, 9, 4),
        Block::square(19, 9, 4, 4),
    ],
    [
        Block::square(0, 0, 4, 24),
        Block::square(7, 10, 10, 4),
        Block::square(22, 10, 4, 4),
    ],
];

pub const PAUSED_GREY: Rgb = Rgb(0x8C8680);
pub const TRAY_FILL: f32 = 0.9;
const TILE_RADIUS: f32 = 0.22;
const SNAP_TOLERANCE: f32 = 0.85;
const SNAP_SLACK: f32 = 0.5;
const SAMPLES: u32 = 8;

#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub plate: Option<Rgb>,
    pub colors: [Rgb; 3],
    pub fill: f32,
}

fn extent(blocks: &[Block; 3]) -> (f32, f32) {
    blocks.iter().fold((0.0, 0.0), |(width, height), block| {
        (
            width.max(block.x + block.width),
            height.max(block.y + block.height),
        )
    })
}

fn centered(size: f32, extent: (f32, f32)) -> (f32, f32) {
    (
        ((size - extent.0) / 2.0).floor(),
        ((size - extent.1) / 2.0).floor(),
    )
}

fn snapped(size: f32, limit: f32) -> Option<[Block; 3]> {
    let (blocks, _) = SNAPPED
        .iter()
        .map(|blocks| (blocks, extent(blocks)))
        .filter(|(_, (width, _))| (limit * SNAP_TOLERANCE..=limit + SNAP_SLACK).contains(width))
        .max_by(|a, b| a.1.0.total_cmp(&b.1.0))?;
    let origin = centered(size, extent(blocks));
    Some(blocks.map(|block| block.scaled(1.0, origin)))
}

fn drawn(size: f32, fill: f32) -> [Block; 3] {
    let limit = size * fill;
    snapped(size, limit).unwrap_or_else(|| {
        let scale = limit / MARK_WIDTH;
        let origin = centered(size, (MARK_WIDTH * scale, MARK_HEIGHT * scale));
        TRAIL.map(|block| block.scaled(scale, (origin.0.round(), origin.1.round())))
    })
}

fn in_plate((x, y): (f32, f32), size: f32) -> bool {
    let radius = size * TILE_RADIUS;
    let half = size / 2.0;
    let corner_x = ((x - half).abs() - (half - radius)).max(0.0);
    let corner_y = ((y - half).abs() - (half - radius)).max(0.0);
    corner_x.hypot(corner_y) <= radius
}

fn shade(look: &Look, blocks: &[Block; 3], size: f32, point: (f32, f32)) -> Option<Rgb> {
    let top = blocks
        .iter()
        .zip(look.colors)
        .find_map(|(block, color)| block.contains(point).then_some(color));
    top.or_else(|| look.plate.filter(|_| in_plate(point, size)))
}

fn coverage(look: &Look, blocks: &[Block; 3], size: f32, column: u32, row: u32) -> [u8; 4] {
    let mut total = [0.0f32; 4];
    for sample_y in 0..SAMPLES {
        for sample_x in 0..SAMPLES {
            let point = (
                column as f32 + (sample_x as f32 + 0.5) / SAMPLES as f32,
                row as f32 + (sample_y as f32 + 0.5) / SAMPLES as f32,
            );
            if let Some(color) = shade(look, blocks, size, point) {
                let [r, g, b] = color.unit_channels();
                for (sum, value) in total.iter_mut().zip([r, g, b, 1.0]) {
                    *sum += value;
                }
            }
        }
    }
    let [r, g, b, a] = total.map(|sum| sum / (SAMPLES * SAMPLES) as f32);
    if a <= 0.0 {
        return [0; 4];
    }
    [r / a, g / a, b / a, a].map(|channel| (channel * CHANNEL_MAX as f32).round() as u8)
}

pub fn render_mark(size: u32, look: &Look) -> Vec<u8> {
    let extent = size as f32;
    let blocks = drawn(extent, look.fill);
    (0..size)
        .flat_map(|row| (0..size).map(move |column| (column, row)))
        .flat_map(|(column, row)| coverage(look, &blocks, extent, column, row))
        .collect()
}
