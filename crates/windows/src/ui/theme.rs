use std::cell::Cell;

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT,
    COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX,
};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};

pub use pith_core::Rgb;
use pith_core::Scheme;

use crate::registry::apps_use_dark_theme;

const WHITE: Rgb = Rgb::WHITE;
pub const INK: Rgb = Rgb(0x1C1814);
pub const CITRUS: Rgb = Rgb(0xE0600A);
pub const AMBER: Rgb = Rgb(0xF5A04A);
pub const CREAM: Rgb = Rgb(0xFBF3E6);
pub fn d2d(color: Rgb) -> D2D1_COLOR_F {
    let [r, g, b] = color.unit_channels();
    D2D1_COLOR_F { r, g, b, a: 1.0 }
}

pub fn colorref(color: Rgb) -> COLORREF {
    COLORREF(color.bgr())
}

fn system_color(index: SYS_COLOR_INDEX) -> Rgb {
    Rgb::from_bgr(unsafe { GetSysColor(index) })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accent {
    pub base: Rgb,
    pub hover: Rgb,
    pub pressed: Rgb,
    pub on: Rgb,
}

impl Accent {
    const LIGHT: Accent = Accent {
        base: Scheme::LIGHT.accent,
        hover: Rgb(0xB84407),
        pressed: Rgb(0xA33C06),
        on: WHITE,
    };
    const DARK: Accent = Accent {
        base: Scheme::DARK.accent,
        hover: Rgb(0xF7AC5E),
        pressed: Rgb(0xE8913A),
        on: INK,
    };

    fn highlight() -> Self {
        let base = system_color(COLOR_HIGHLIGHT);
        Self {
            base,
            hover: base,
            pressed: base,
            on: system_color(COLOR_HIGHLIGHTTEXT),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub page: Rgb,
    pub card: Rgb,
    pub card_stroke: Rgb,
    pub text: Rgb,
    pub text_secondary: Rgb,
    pub control: Rgb,
    pub control_hover: Rgb,
    pub control_pressed: Rgb,
    pub control_stroke: Rgb,
    pub strong_stroke: Rgb,
    pub track: Rgb,
    pub accent: Accent,
    pub mark_ghost: Rgb,
    pub danger: Rgb,
    pub success: Rgb,
}

impl Palette {
    fn light() -> Self {
        Self {
            page: Rgb(0xF3F3F3),
            card: Scheme::LIGHT.card,
            card_stroke: Rgb(0xE5E5E5),
            text: Scheme::LIGHT.text,
            text_secondary: Scheme::LIGHT.text_secondary,
            control: Rgb(0xFBFBFB),
            control_hover: Rgb(0xF3F3F3),
            control_pressed: Rgb(0xEAEAEA),
            control_stroke: Rgb(0xD6D6D6),
            strong_stroke: Rgb(0x7A7A7A),
            track: Rgb(0xD0D0D0),
            accent: Accent::LIGHT,
            mark_ghost: Rgb(0xBBB2A6),
            danger: Rgb(0xC42B1C),
            success: Rgb(0x0F7B0F),
        }
    }

    fn dark() -> Self {
        Self {
            page: Rgb(0x202020),
            card: Scheme::DARK.card,
            card_stroke: Rgb(0x3A3A3A),
            text: Scheme::DARK.text,
            text_secondary: Scheme::DARK.text_secondary,
            control: Rgb(0x373737),
            control_hover: Rgb(0x3F3F3F),
            control_pressed: Rgb(0x323232),
            control_stroke: Rgb(0x4A4A4A),
            strong_stroke: Rgb(0x9B9B9B),
            track: Rgb(0x5C5C5C),
            accent: Accent::DARK,
            mark_ghost: Rgb(0x6B645C),
            danger: Rgb(0xFF99A4),
            success: Rgb(0x6CCB5F),
        }
    }

    fn contrast() -> Self {
        let window = system_color(COLOR_WINDOW);
        let text = system_color(COLOR_WINDOWTEXT);
        let face = system_color(COLOR_BTNFACE);
        let accent = Accent::highlight();
        Self {
            page: window,
            card: window,
            card_stroke: text,
            text,
            text_secondary: system_color(COLOR_GRAYTEXT),
            control: face,
            control_hover: accent.base,
            control_pressed: accent.base,
            control_stroke: system_color(COLOR_BTNTEXT),
            strong_stroke: text,
            track: face,
            accent,
            mark_ghost: system_color(COLOR_GRAYTEXT),
            danger: text,
            success: text,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub palette: Palette,
    pub dark: bool,
    pub contrast: bool,
}

thread_local! {
    static FORCED_DARK: Cell<Option<bool>> = const { Cell::new(None) };
}

pub fn force_dark_theme(dark: Option<bool>) {
    FORCED_DARK.with(|forced| forced.set(dark));
}

fn high_contrast() -> bool {
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_ok() && contrast.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0
}

impl Theme {
    pub fn detect() -> Self {
        let contrast = high_contrast();
        let dark = !contrast
            && FORCED_DARK
                .with(Cell::get)
                .unwrap_or_else(apps_use_dark_theme);
        let palette = match (contrast, dark) {
            (true, _) => Palette::contrast(),
            (false, true) => Palette::dark(),
            (false, false) => Palette::light(),
        };
        Self {
            palette,
            dark,
            contrast,
        }
    }
}
