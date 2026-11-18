use pith_core::{FADE_ALPHAS, FADE_FRAME_MS};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION};
use windows::Win32::UI::WindowsAndMessaging::{
    GWLP_USERDATA, GetWindowLongPtrW, KillTimer, SetTimer, SetWindowLongPtrW, ULW_ALPHA,
    UpdateLayeredWindow,
};

pub const TIMER: usize = 1;
const LAST_STEP: usize = FADE_ALPHAS.len() - 1;

pub fn blend(step: usize) -> BLENDFUNCTION {
    BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: FADE_ALPHAS[step.min(LAST_STEP)],
        AlphaFormat: AC_SRC_ALPHA as u8,
    }
}

pub fn step(window: HWND) -> usize {
    unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as usize }
}

pub fn reset(window: HWND) {
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, 0);
    }
}

pub fn start(window: HWND) {
    unsafe {
        SetTimer(Some(window), TIMER, FADE_FRAME_MS, None);
    }
}

pub fn stop(window: HWND) {
    unsafe {
        let _ = KillTimer(Some(window), TIMER);
    }
}

pub fn advance(window: HWND) {
    let next = (step(window) + 1).min(LAST_STEP);
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, next as isize);
        let _ = UpdateLayeredWindow(
            window,
            None,
            None,
            None,
            None,
            None,
            COLORREF(0),
            Some(&blend(next)),
            ULW_ALPHA,
        );
    }
    if next == LAST_STEP {
        stop(window);
    }
}
