use pith_windows::{OnboardingEvent, force_dark_theme};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

use crate::capture::OUT_DIR;
use crate::controls::states;
use crate::onboarding::onboarding;
use crate::overlay::overlay;
use crate::settings::{fits_small_laptop, pages, variants};

mod capture;
#[path = "../common/mod.rs"]
mod common;
mod controls;
mod onboarding;
mod overlay;
mod settings;

const DPIS: [u32; 3] = [96, 144, 192];

const APP_BACKDROPS: [[u8; 3]; 2] = [[0xFF; 3], [0x1E; 3]];
const OVERLAY_DPIS: [u32; 3] = [96, 144, 192];

const THEMES: [(&str, bool, [u8; 3], [u8; 3]); 2] = [
    ("light", false, [0xF3, 0xF3, 0xF3], [0xFF, 0xFF, 0xFF]),
    ("dark", true, [0x20, 0x20, 0x20], [0x2B, 0x2B, 0x2B]),
];

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    std::fs::create_dir_all(OUT_DIR).unwrap();
    let download = [OnboardingEvent::Progress(150_000_000, 397_000_000)];
    let ready = [OnboardingEvent::Ready];
    let failed = [OnboardingEvent::Failed(
        "the model could not be downloaded. Check your connection and start Pith again.".into(),
    )];
    for (theme, dark, fallback, card) in THEMES {
        force_dark_theme(Some(dark));
        for dpi in DPIS {
            pages(&format!("{theme}-{dpi}"), dpi, fallback);
        }
        for dpi in [96, 144] {
            onboarding(
                &format!("onboarding-practice-{theme}-{dpi}"),
                &ready,
                dpi,
                fallback,
            );
        }
        for dpi in OVERLAY_DPIS {
            overlay(
                &format!("overlay-{theme}-{dpi}"),
                dark,
                dpi,
                APP_BACKDROPS[usize::from(dark)],
            );
        }
        states(&format!("states-{theme}"), card);
        variants(theme, fallback);
        fits_small_laptop(theme, fallback);
        onboarding(
            &format!("onboarding-download-{theme}-96"),
            &download,
            96,
            fallback,
        );
        onboarding(
            &format!("onboarding-failed-{theme}-96"),
            &failed,
            96,
            fallback,
        );
    }
}
