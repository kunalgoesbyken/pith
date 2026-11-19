use std::path::Path;

use pith_windows::brand::{self, Look};

mod common;

use common::png;

const ICON_PATH: &str = "assets/pith.ico";
const SHOTS_DIR: &str = "target/brand-shots";
const ICON_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];
const TRAY_SIZES: [u32; 4] = [16, 20, 24, 32];
const APP_SIZES: [u32; 7] = [16, 20, 24, 32, 40, 48, 64];
const LIGHT: [u8; 4] = [0xF3, 0xF3, 0xF3, 0xFF];
const DARK: [u8; 4] = [0x20, 0x20, 0x20, 0xFF];
const MAGNIFIED: u32 = 128;
const GAP: u32 = 8;
const ICO_HEADER: usize = 6;
const ICO_ENTRY: usize = 16;
const ICO_PLANES: u16 = 1;
const ICO_DEPTH: u16 = 32;

fn ico(look: &Look) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = ICON_SIZES
        .iter()
        .map(|&size| (size, png(size, size, &brand::render(size, look))))
        .collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend((images.len() as u16).to_le_bytes());
    let mut offset = ICO_HEADER + ICO_ENTRY * images.len();
    for (size, data) in &images {
        out.extend([*size as u8, *size as u8, 0, 0]);
        out.extend(ICO_PLANES.to_le_bytes());
        out.extend(ICO_DEPTH.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in images {
        out.extend(data);
    }
    out
}

fn blend(pixel: &[u8], background: [u8; 4]) -> [u8; 4] {
    let alpha = u32::from(pixel[3]);
    let mixed = |top: u8, under: u8| {
        ((u32::from(top) * alpha + u32::from(under) * (255 - alpha)) / 255) as u8
    };
    [
        mixed(pixel[0], background[0]),
        mixed(pixel[1], background[1]),
        mixed(pixel[2], background[2]),
        255,
    ]
}

fn sheet(name: &str, rows: &[([u8; 4], &Look)], sizes: &[u32]) {
    let zoom = |size: u32| (MAGNIFIED / size).max(1);
    let cell = sizes.iter().map(|&size| size * zoom(size)).max().unwrap() + GAP;
    let width = cell * sizes.len() as u32;
    let height = cell * rows.len() as u32;
    let mut canvas = vec![0u8; (width * height * 4) as usize];
    for (row, (background, look)) in rows.iter().enumerate() {
        for (column, &size) in sizes.iter().enumerate() {
            let rgba = brand::render(size, look);
            for y in 0..cell {
                for x in 0..cell {
                    let (sx, sy) = (x / zoom(size), y / zoom(size));
                    let pixel = if sx < size && sy < size {
                        let at = ((sy * size + sx) * 4) as usize;
                        blend(&rgba[at..at + 4], *background)
                    } else {
                        *background
                    };
                    let (px, py) = (column as u32 * cell + x, row as u32 * cell + y);
                    let at = ((py * width + px) * 4) as usize;
                    canvas[at..at + 4].copy_from_slice(&pixel);
                }
            }
        }
    }
    let path = Path::new(SHOTS_DIR).join(format!("{name}.png"));
    std::fs::write(&path, png(width, height, &canvas)).unwrap();
    println!("{}", path.display());
}

fn main() {
    std::fs::create_dir_all(SHOTS_DIR).unwrap();
    std::fs::write(ICON_PATH, ico(&brand::APP)).unwrap();
    println!("{ICON_PATH}");
    let tray = [
        (LIGHT, &brand::ACTIVE),
        (DARK, &brand::ACTIVE),
        (LIGHT, &brand::PAUSED),
        (DARK, &brand::PAUSED),
    ];
    sheet("tray", &tray, &TRAY_SIZES);
    sheet(
        "app",
        &[(LIGHT, &brand::APP), (DARK, &brand::APP)],
        &APP_SIZES,
    );
    sheet("app-256", &[(LIGHT, &brand::APP)], &[256]);
}
