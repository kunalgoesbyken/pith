use std::ffi::c_void;

use pith_core::Rgb;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
    ETO_OPTIONS, ETO_RTLREADING, ExtTextOutW, HBITMAP, HDC, HGDIOBJ, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows::core::PCWSTR;

use super::face::Face;
use super::scene::Scene;

const BITS_PER_PIXEL: u16 = 32;
const CHANNEL_MASK: u32 = 0xFF;

pub struct Raster<'a> {
    face: &'a Face,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    pub size: SIZE,
}

impl<'a> Raster<'a> {
    pub fn render(scene: &'a Scene) -> Option<Self> {
        let raster = Self::allocate(&scene.face, scene.placed.size)?;
        raster.draw_text(scene);
        raster.shade(scene);
        Some(raster)
    }

    pub fn source(&self) -> HDC {
        self.face.memory
    }

    fn allocate(face: &'a Face, size: SIZE) -> Option<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.cx,
                biHeight: -size.cy,
                biPlanes: 1,
                biBitCount: BITS_PER_PIXEL,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        unsafe {
            let bitmap =
                CreateDIBSection(Some(face.memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                    .ok()?;
            Some(Self {
                face,
                bitmap,
                previous: SelectObject(face.memory, bitmap.into()),
                bits: bits.cast(),
                size,
            })
        }
    }

    fn draw_text(&self, scene: &Scene) {
        let memory = self.face.memory;
        let options = if scene.rtl {
            ETO_RTLREADING
        } else {
            ETO_OPTIONS(0)
        };
        unsafe {
            SetBkMode(memory, TRANSPARENT);
            SetTextColor(memory, COLORREF(Rgb::WHITE.bgr()));
            for line in &scene.placed.lines {
                let units: Vec<u16> = scene.text[line.start..line.end].encode_utf16().collect();
                let _ = ExtTextOutW(
                    memory,
                    line.x - scene.placed.origin.x,
                    line.y - scene.placed.origin.y,
                    options,
                    None,
                    PCWSTR(units.as_ptr()),
                    units.len() as u32,
                    None,
                );
            }
        }
    }

    fn shade(&self, scene: &Scene) {
        let (width, height) = (self.size.cx as usize, self.size.cy as usize);
        let pixels = unsafe { std::slice::from_raw_parts_mut(self.bits, width * height) };
        for (row, band) in pixels.chunks_exact_mut(width).enumerate() {
            for (column, pixel) in band.iter_mut().enumerate() {
                let coverage = ((*pixel >> 16) & CHANNEL_MASK)
                    .max((*pixel >> 8) & CHANNEL_MASK)
                    .max(*pixel & CHANNEL_MASK);
                *pixel = scene.shade(column as i32, row as i32, coverage);
            }
        }
    }
}

impl Drop for Raster<'_> {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.face.memory, self.previous);
            let _ = DeleteObject(self.bitmap.into());
        }
    }
}
