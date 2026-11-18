use std::ffi::c_void;

use windows::Win32::Graphics::Gdi::{
    BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, SRCCOPY, SelectObject,
};

const BYTES_PER_PIXEL: usize = 4;
const ALPHA: usize = 3;

pub struct Buffer {
    pub memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut c_void,
    size: (i32, i32),
}

impl Buffer {
    pub fn new(size: (i32, i32)) -> Option<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0,
                biHeight: -size.1,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        unsafe {
            let memory = CreateCompatibleDC(None);
            let mut bits = std::ptr::null_mut();
            let Ok(bitmap) =
                CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                let _ = DeleteDC(memory);
                return None;
            };
            let previous = SelectObject(memory, bitmap.into());
            Some(Self {
                memory,
                bitmap,
                previous,
                bits,
                size,
            })
        }
    }

    pub fn make_opaque(&self) {
        let count = self.size.0 as usize * self.size.1 as usize;
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.bits.cast::<[u8; BYTES_PER_PIXEL]>(), count)
        };
        for pixel in pixels {
            pixel[ALPHA] = u8::MAX;
        }
    }

    pub fn copy_to(&self, target: HDC, at: (i32, i32)) {
        unsafe {
            let _ = BitBlt(
                target,
                at.0,
                at.1,
                self.size.0,
                self.size.1,
                Some(self.memory),
                0,
                0,
                SRCCOPY,
            );
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.memory);
        }
    }
}
