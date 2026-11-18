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

use crate::registry::{apps_use_dark_theme, read_dword};

const ACCENT_KEY: &str = r"Software\Microsoft\Windows\DWM";
const ACCENT_VALUE: &str = "AccentColor";
const DEFAULT_ACCENT: Rgb = Rgb(0x0067C0);
const WHITE: Rgb = Rgb(0xFFFFFF);
const BLACK: Rgb = Rgb(0x000000);
const ACCENT_HOVER_MIX: f32 = 0.12;
const ACCENT_PRESSED_MIX: f32 = 0.24;
const BRIGHT_ACCENT: f32 = 0.45;
const CHANNEL_SHIFTS: [u32; 3] = [16, 8, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u32);

impl Rgb {
    fn channel(self, shift: u32) -> u32 {
        (self.0 >> shift) & 0xFF
    }

    pub fn d2d(self) -> D2D1_COLOR_F {
        let [r, g, b] = CHANNEL_SHIFTS.map(|shift| self.channel(shift) as f32 / 255.0);
        D2D1_COLOR_F { r, g, b, a: 1.0 }
    }

    pub fn colorref(self) -> COLORREF {
        let [r, g, b] = CHANNEL_SHIFTS.map(|shift| self.channel(shift));
        COLORREF(r | (g << 8) | (b << 16))
    }

    pub fn mix(self, other: Rgb, amount: f32) -> Rgb {
        let blended = CHANNEL_SHIFTS.map(|shift| {
            let from = self.channel(shift) as f32;
            let to = other.channel(shift) as f32;
            ((from + (to - from) * amount).round() as u32) << shift
        });
        Rgb(blended.iter().sum())
    }

    pub fn luminance(self) -> f32 {
        let [r, g, b] = CHANNEL_SHIFTS.map(|shift| self.channel(shift) as f32 / 255.0);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fn from_colorref(color: u32) -> Rgb {
        Rgb(((color & 0xFF) << 16) | (color & 0xFF00) | ((color >> 16) & 0xFF))
    }

    fn system(index: SYS_COLOR_INDEX) -> Rgb {
        Rgb::from_colorref(unsafe { GetSysColor(index) })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accent {
    pub base: Rgb,
    pub hover: Rgb,
    pub pressed: Rgb,
    pub on: Rgb,
}

impl Accent {
    fn of(base: Rgb) -> Self {
        let (on, toward) = if base.luminance() > BRIGHT_ACCENT {
            (BLACK, WHITE)
        } else {
            (WHITE, BLACK)
        };
        Self {
            base,
            hover: base.mix(toward, ACCENT_HOVER_MIX),
            pressed: base.mix(toward, ACCENT_PRESSED_MIX),
            on,
        }
    }

    fn system() -> Self {
        let registered = read_dword(ACCENT_KEY, ACCENT_VALUE).map(Rgb::from_colorref);
        Self::of(registered.unwrap_or(DEFAULT_ACCENT))
    }

    fn highlight() -> Self {
        let base = Rgb::system(COLOR_HIGHLIGHT);
        Self {
            base,
            hover: base,
            pressed: base,
            on: Rgb::system(COLOR_HIGHLIGHTTEXT),
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
    pub danger: Rgb,
    pub success: Rgb,
}

impl Palette {
    fn light(accent: Accent) -> Self {
        Self {
            page: Rgb(0xF3F3F3),
            card: WHITE,
            card_stroke: Rgb(0xE5E5E5),
            text: Rgb(0x1A1A1A),
            text_secondary: Rgb(0x5D5D5D),
            control: Rgb(0xFBFBFB),
            control_hover: Rgb(0xF3F3F3),
            control_pressed: Rgb(0xEAEAEA),
            control_stroke: Rgb(0xD6D6D6),
            strong_stroke: Rgb(0x7A7A7A),
            track: Rgb(0xD0D0D0),
            accent,
            danger: Rgb(0xC42B1C),
            success: Rgb(0x0F7B0F),
        }
    }

    fn dark(accent: Accent) -> Self {
        Self {
            page: Rgb(0x202020),
            card: Rgb(0x2B2B2B),
            card_stroke: Rgb(0x3A3A3A),
            text: WHITE,
            text_secondary: Rgb(0xC5C5C5),
            control: Rgb(0x373737),
            control_hover: Rgb(0x3F3F3F),
            control_pressed: Rgb(0x323232),
            control_stroke: Rgb(0x4A4A4A),
            strong_stroke: Rgb(0x9B9B9B),
            track: Rgb(0x5C5C5C),
            accent,
            danger: Rgb(0xFF99A4),
            success: Rgb(0x6CCB5F),
        }
    }

    fn contrast() -> Self {
        let window = Rgb::system(COLOR_WINDOW);
        let text = Rgb::system(COLOR_WINDOWTEXT);
        let face = Rgb::system(COLOR_BTNFACE);
        let accent = Accent::highlight();
        Self {
            page: window,
            card: window,
            card_stroke: text,
            text,
            text_secondary: Rgb::system(COLOR_GRAYTEXT),
            control: face,
            control_hover: accent.base,
            control_pressed: accent.base,
            control_stroke: Rgb::system(COLOR_BTNTEXT),
            strong_stroke: text,
            track: face,
            accent,
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
            (false, true) => Palette::dark(Accent::system()),
            (false, false) => Palette::light(Accent::system()),
        };
        Self {
            palette,
            dark,
            contrast,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_convert_to_colorrefs_with_swapped_channels() {
        assert_eq!(Rgb(0x1A2B3C).colorref().0, 0x003C_2B1A);
        assert_eq!(Rgb::from_colorref(0x003C_2B1A), Rgb(0x1A2B3C));
    }

    #[test]
    fn mixing_moves_each_channel_toward_the_other_color() {
        assert_eq!(BLACK.mix(WHITE, 0.0), BLACK);
        assert_eq!(BLACK.mix(WHITE, 1.0), WHITE);
        assert_eq!(Rgb(0x102030).mix(Rgb(0x304050), 0.5), Rgb(0x203040));
    }

    #[test]
    fn text_on_accent_follows_the_accent_brightness() {
        assert_eq!(Accent::of(Rgb(0x0067C0)).on, WHITE);
        assert_eq!(Accent::of(Rgb(0x4CC2FF)).on, BLACK);
    }

    #[test]
    fn the_forced_theme_overrides_the_registry_until_cleared() {
        force_dark_theme(Some(true));
        let forced = Theme::detect();
        assert_eq!(forced.dark, !forced.contrast);
        force_dark_theme(Some(false));
        assert!(!Theme::detect().dark);
        force_dark_theme(None);
    }

    #[test]
    fn backgrounds_are_never_pure_white_or_black() {
        for palette in [
            Palette::light(Accent::of(DEFAULT_ACCENT)),
            Palette::dark(Accent::of(DEFAULT_ACCENT)),
        ] {
            assert!(palette.page != WHITE && palette.page != BLACK);
        }
    }
}
