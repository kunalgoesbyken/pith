use pith_core::{CHANNEL_MAX, Rgb, Scheme};

use crate::trail::{Block, MARK_HEIGHT, MARK_WIDTH, TRAIL};
use crate::ui::{INK, Taskbar};

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

const PAUSED_GREY: Rgb = Rgb(0x8C8680);
const TILE_WORD: Rgb = Rgb(0xDAD2C6);
const TILE_DOT: Rgb = Rgb(0x8C867D);
const TRAY_FILL: f32 = 0.9;
const TILE_GRID: f32 = 256.0;
const TILE_FILL: f32 = MARK_WIDTH / TILE_GRID;
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

pub const APP: Look = Look {
    plate: Some(INK),
    colors: [Scheme::DARK.accent, TILE_WORD, TILE_DOT],
    fill: TILE_FILL,
};

pub fn tray(taskbar: Taskbar, active: bool) -> Look {
    let (caret, trail) = taskbar.glyph();
    let colors = match (active, taskbar) {
        (true, _) => [caret, trail, trail],
        (false, Taskbar::Contrast { dim, .. }) => [dim; 3],
        (false, _) => [PAUSED_GREY; 3],
    };
    Look {
        plate: None,
        colors,
        fill: TRAY_FILL,
    }
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

pub fn render(size: u32, look: &Look) -> Vec<u8> {
    let extent = size as f32;
    let blocks = drawn(extent, look.fill);
    (0..size)
        .flat_map(|row| (0..size).map(move |column| (column, row)))
        .flat_map(|(column, row)| coverage(look, &blocks, extent, column, row))
        .collect()
}
