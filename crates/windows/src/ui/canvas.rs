use std::cell::RefCell;

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::HDC;
use windows::core::{BOOL, PCWSTR, w};
use windows_numerics::Vector2;

use super::buffer::Buffer;
use super::metrics::{BASE_DPI, Role, STROKE, Scale, height, width};
use super::theme::Rgb;

const FAMILIES: [PCWSTR; 2] = [w!("Segoe UI Variable Text"), w!("Segoe UI")];
const TRANSPARENT: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};
const UNBOUNDED: f32 = 1.0e6;

#[derive(Clone)]
struct Resources {
    dwrite: IDWriteFactory,
    target: ID2D1DCRenderTarget,
    family: PCWSTR,
}

thread_local! {
    static RESOURCES: RefCell<Option<Resources>> = const { RefCell::new(None) };
}

fn installed(dwrite: &IDWriteFactory, family: PCWSTR) -> bool {
    let mut collection = None;
    let mut index = 0u32;
    let mut exists = BOOL(0);
    unsafe {
        dwrite.GetSystemFontCollection(&mut collection, false).ok();
        collection
            .and_then(|fonts| fonts.FindFamilyName(family, &mut index, &mut exists).ok())
            .is_some()
            && exists.as_bool()
    }
}

fn create_resources() -> windows::core::Result<Resources> {
    unsafe {
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: BASE_DPI as f32,
            dpiY: BASE_DPI as f32,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target = d2d.CreateDCRenderTarget(&properties)?;
        target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let family = FAMILIES
            .into_iter()
            .find(|family| installed(&dwrite, *family))
            .unwrap_or(FAMILIES[1]);
        Ok(Resources {
            dwrite,
            target,
            family,
        })
    }
}

fn resources() -> Option<Resources> {
    RESOURCES.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = create_resources().ok();
        }
        slot.clone()
    })
}

pub fn family_name() -> Vec<u16> {
    resources()
        .map(|resources| unsafe { resources.family.as_wide().to_vec() })
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

impl Align {
    fn dwrite(self) -> DWRITE_TEXT_ALIGNMENT {
        match self {
            Align::Start => DWRITE_TEXT_ALIGNMENT_LEADING,
            Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            Align::End => DWRITE_TEXT_ALIGNMENT_TRAILING,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text {
    pub role: Role,
    pub color: Rgb,
    pub align: Align,
    pub wrap: bool,
}

impl Text {
    pub fn new(role: Role, color: Rgb) -> Self {
        Self {
            role,
            color,
            align: Align::Start,
            wrap: false,
        }
    }

    pub fn aligned(self, align: Align) -> Self {
        Self { align, ..self }
    }

    pub fn wrapping(self) -> Self {
        Self { wrap: true, ..self }
    }
}

pub struct Canvas {
    resources: Resources,
    brush: ID2D1SolidColorBrush,
    scale: Scale,
}

fn float_rect(bounds: RECT) -> D2D_RECT_F {
    D2D_RECT_F {
        left: bounds.left as f32,
        top: bounds.top as f32,
        right: bounds.right as f32,
        bottom: bounds.bottom as f32,
    }
}

fn point(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

impl Canvas {
    pub fn clear(&self, color: Option<Rgb>) {
        let color = color.map_or(TRANSPARENT, Rgb::d2d);
        unsafe { self.resources.target.Clear(Some(&color)) };
    }

    fn paint_with(&self, color: Rgb) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&color.d2d()) };
        &self.brush
    }

    pub fn fill_round(&self, bounds: RECT, radius: i32, color: Rgb) {
        let shape = D2D1_ROUNDED_RECT {
            rect: float_rect(bounds),
            radiusX: radius as f32,
            radiusY: radius as f32,
        };
        unsafe {
            self.resources
                .target
                .FillRoundedRectangle(&shape, self.paint_with(color));
        }
    }

    pub fn stroke_round(&self, bounds: RECT, radius: i32, color: Rgb, thickness: i32) {
        let half = thickness as f32 / 2.0;
        let mut rect = float_rect(bounds);
        rect.left += half;
        rect.top += half;
        rect.right -= half;
        rect.bottom -= half;
        let shape = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius as f32 - half,
            radiusY: radius as f32 - half,
        };
        unsafe {
            self.resources.target.DrawRoundedRectangle(
                &shape,
                self.paint_with(color),
                thickness as f32,
                None,
            );
        }
    }

    pub fn card(&self, bounds: RECT, radius: i32, fill: Rgb, stroke: Rgb) {
        self.fill_round(bounds, radius, fill);
        self.stroke_round(bounds, radius, stroke, STROKE);
    }

    pub fn disc(&self, center: (f32, f32), radius: f32, color: Rgb) {
        let shape = D2D1_ELLIPSE {
            point: point(center.0, center.1),
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.resources
                .target
                .FillEllipse(&shape, self.paint_with(color));
        }
    }

    pub fn line(&self, from: (f32, f32), to: (f32, f32), thickness: f32, color: Rgb) {
        unsafe {
            self.resources.target.DrawLine(
                point(from.0, from.1),
                point(to.0, to.1),
                self.paint_with(color),
                thickness,
                None,
            );
        }
    }

    pub fn text(&self, bounds: RECT, text: &str, style: Text) {
        let Some(format) = format(&self.resources, self.scale, style) else {
            return;
        };
        let content: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            self.resources.target.DrawText(
                &content,
                &format,
                &float_rect(bounds),
                self.paint_with(style.color),
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
}

fn format(resources: &Resources, scale: Scale, style: Text) -> Option<IDWriteTextFormat> {
    unsafe {
        let format = resources
            .dwrite
            .CreateTextFormat(
                resources.family,
                None,
                DWRITE_FONT_WEIGHT(style.role.weight()),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                scale.size(style.role.size()),
                w!(""),
            )
            .ok()?;
        let _ = format.SetTextAlignment(style.align.dwrite());
        if style.wrap {
            let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            return Some(format);
        }
        let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            ..Default::default()
        };
        if let Ok(sign) = resources.dwrite.CreateEllipsisTrimmingSign(&format) {
            let _ = format.SetTrimming(&trimming, &sign);
        }
        Some(format)
    }
}

pub fn measure(scale: Scale, text: &str, style: Text, max_width: i32) -> (i32, i32) {
    let Some(resources) = resources() else {
        return (0, 0);
    };
    let Some(format) = format(&resources, scale, style) else {
        return (0, 0);
    };
    let content: Vec<u16> = text.encode_utf16().collect();
    let limit = if style.wrap {
        max_width as f32
    } else {
        UNBOUNDED
    };
    unsafe {
        let Ok(layout) = resources
            .dwrite
            .CreateTextLayout(&content, &format, limit, UNBOUNDED)
        else {
            return (0, 0);
        };
        let mut metrics = DWRITE_TEXT_METRICS::default();
        let _ = layout.GetMetrics(&mut metrics);
        (
            metrics.widthIncludingTrailingWhitespace.ceil() as i32,
            metrics.height.ceil() as i32,
        )
    }
}

pub fn paint(hdc: HDC, bounds: RECT, scale: Scale, draw: impl FnOnce(&Canvas)) {
    let size = (width(bounds), height(bounds));
    if size.0 <= 0 || size.1 <= 0 {
        return;
    }
    let Some(buffer) = Buffer::new(size) else {
        return;
    };
    let Some(resources) = resources() else {
        return;
    };
    unsafe {
        let area = RECT {
            left: 0,
            top: 0,
            right: size.0,
            bottom: size.1,
        };
        if resources.target.BindDC(buffer.memory, &area).is_err() {
            return;
        }
        let Ok(brush) = resources.target.CreateSolidColorBrush(&TRANSPARENT, None) else {
            return;
        };
        resources.target.BeginDraw();
        let target = resources.target.clone();
        draw(&Canvas {
            resources,
            brush,
            scale,
        });
        if target.EndDraw(None, None).is_ok() {
            buffer.copy_to(hdc, (bounds.left, bounds.top));
        }
    }
}
