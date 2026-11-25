use pith_core::Reach;
use pith_windows::{Onboarding, OnboardingEvent};

use crate::capture::{capture, park};

pub(crate) fn onboarding(name: &str, events: &[OnboardingEvent], dpi: u32, fallback: [u8; 3]) {
    let marker = std::env::temp_dir().join("pith-ui-shots-marker");
    let mut window = Onboarding::create(marker, false, &Reach::ThisPc).unwrap();
    let hwnd = window.controls().window;
    park(hwnd, dpi);
    for event in events {
        window.event(event.clone());
    }
    capture(hwnd, name, fallback);
}
