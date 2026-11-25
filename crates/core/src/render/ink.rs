use crate::Rgb;

pub const CONTRAST_FLOOR: f32 = 3.0;
pub const MAX_ALPHA: u32 = 230;

const NEXT_WORD_BOOST_PERCENT: u32 = 135;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ink {
    pub color: Rgb,
    pub alpha: u8,
}

pub fn opacity_alpha(percent: u8) -> u32 {
    (u32::from(percent.min(100)) * 255 / 100).clamp(1, MAX_ALPHA)
}

pub fn luminance(color: Rgb) -> f32 {
    let [red, green, blue] = color.unit_channels().map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

pub fn blend(foreground: Rgb, background: Rgb, alpha: u32) -> Rgb {
    let weight = alpha as f32 / 255.0;
    let [front, back] = [foreground, background].map(Rgb::channels);
    Rgb::from_channels(std::array::from_fn(|index| {
        (front[index] as f32 * weight + back[index] as f32 * (1.0 - weight)).round() as u32
    }))
}

pub fn best_plain(background: Rgb) -> Rgb {
    if contrast(Rgb::WHITE, background) > contrast(Rgb::BLACK, background) {
        Rgb::WHITE
    } else {
        Rgb::BLACK
    }
}

fn raise_to_floor(foreground: Rgb, background: Rgb, start: u32) -> Option<u32> {
    (start..=MAX_ALPHA)
        .find(|alpha| contrast(blend(foreground, background, *alpha), background) >= CONTRAST_FLOOR)
}

pub fn ink(text_color: Option<Rgb>, background: Rgb, opacity_percent: u8) -> Ink {
    let start = opacity_alpha(opacity_percent);
    let preferred = text_color.unwrap_or_else(|| best_plain(background));
    if let Some(alpha) = raise_to_floor(preferred, background, start) {
        return Ink {
            color: preferred,
            alpha: alpha as u8,
        };
    }
    let plain = best_plain(background);
    Ink {
        color: plain,
        alpha: raise_to_floor(plain, background, start).unwrap_or(MAX_ALPHA) as u8,
    }
}

pub fn next_word_alpha(alpha: u8) -> u8 {
    (u32::from(alpha) * NEXT_WORD_BOOST_PERCENT / 100)
        .min(MAX_ALPHA)
        .max(u32::from(alpha)) as u8
}

pub fn median_by_luminance(mut colors: Vec<Rgb>) -> Option<Rgb> {
    colors.sort_by(|a, b| luminance(*a).total_cmp(&luminance(*b)));
    colors.get(colors.len() / 2).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_picks_a_readable_colour_and_stays_within_the_alpha_cap() {
        let picks = [
            (Some(Rgb::BLACK), Rgb::WHITE, 70, Rgb::BLACK),
            (None, Rgb::WHITE, 50, Rgb::BLACK),
            (None, Rgb(0x10_1010), 50, Rgb::WHITE),
            (None, Rgb(0x00_00FF), 50, Rgb::WHITE),
            (None, Rgb(0x00_FF00), 50, Rgb::BLACK),
        ];
        for row in picks {
            let (foreground, background, percent, expected) = row;
            assert_eq!(
                ink(foreground, background, percent).color,
                expected,
                "{row:?}"
            );
        }
        assert_eq!(
            u32::from(ink(Some(Rgb::BLACK), Rgb::WHITE, 70).alpha),
            opacity_alpha(70)
        );
        for background in [
            Rgb::BLACK,
            Rgb::WHITE,
            Rgb(0x80_8080),
            Rgb(0xFF_00FF),
            Rgb(0xFF_FF00),
            Rgb(0x30_2010),
        ] {
            for percent in [1u8, 10, 35, 50, 65, 100, 255] {
                let result = ink(None, background, percent);
                assert!(u32::from(result.alpha) <= MAX_ALPHA);
                let shown = blend(result.color, background, u32::from(result.alpha));
                assert!(
                    contrast(shown, background) >= CONTRAST_FLOOR,
                    "{background:?} {percent}"
                );
            }
        }
    }

    #[test]
    fn faint_opacity_is_raised_until_the_floor_is_met() {
        for (foreground, background) in [
            (Rgb::BLACK, Rgb::WHITE),
            (Rgb::WHITE, Rgb::BLACK),
            (Rgb(0x00_55AA), Rgb(0xF0_F0F0)),
        ] {
            let result = ink(Some(foreground), background, 10);
            assert!(u32::from(result.alpha) > opacity_alpha(10));
            let shown = blend(result.color, background, u32::from(result.alpha));
            assert!(
                contrast(shown, background) >= CONTRAST_FLOOR,
                "{foreground:?} on {background:?}"
            );
        }
    }

    #[test]
    fn text_colour_that_cannot_reach_the_floor_is_replaced_by_black_or_white() {
        let grey_on_grey = ink(Some(Rgb(0x80_8080)), Rgb(0x88_8888), 50);
        assert!(grey_on_grey.color == Rgb::BLACK || grey_on_grey.color == Rgb::WHITE);
        let shown = blend(
            grey_on_grey.color,
            Rgb(0x88_8888),
            u32::from(grey_on_grey.alpha),
        );
        assert!(contrast(shown, Rgb(0x88_8888)) >= CONTRAST_FLOOR);
    }

    #[test]
    fn luminance_and_contrast_match_wcag_reference_values() {
        assert!((contrast(Rgb::BLACK, Rgb::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Rgb::WHITE, Rgb::WHITE) - 1.0).abs() < 0.001);
        assert!((contrast(Rgb(0x77_7777), Rgb::WHITE) - 4.48).abs() < 0.05);
    }
}
