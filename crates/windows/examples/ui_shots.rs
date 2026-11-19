use std::path::PathBuf;

use pith_core::{Config, Counts, ModelPreset, Reach};
use pith_windows::{
    Onboarding, OnboardingEvent, Rival, SettingsWindow, StatsView, force_dark_theme,
};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODA_DRAWENTIRE, ODS_FLAGS, ODT_BUTTON};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::*;

mod common;

use common::{bgra_to_rgba, bitmap_info, png, window_rect};

const OUT_DIR: &str = "target/ui-shots";
const DPIS: [u32; 2] = [96, 144];
const THEMES: [(&str, bool, [u8; 3], [u8; 3]); 2] = [
    ("light", false, [0xF3, 0xF3, 0xF3], [0xFF, 0xFF, 0xFF]),
    ("dark", true, [0x20, 0x20, 0x20], [0x2B, 0x2B, 0x2B]),
];
const PARKED_X: i32 = -12000;
const PARKED_Y: i32 = 100;
const PRINT_RENDER_FULL_CONTENT: u32 = 2;

fn pump() {
    let mut message = MSG::default();
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn composite(bgra: &mut [u8], fallback: [u8; 3]) {
    for pixel in bgra.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for (channel, base) in pixel.iter_mut().take(3).zip(fallback.iter().rev()) {
            let covered = u32::from(*channel) * alpha + u32::from(*base) * (255 - alpha);
            *channel = (covered / 255) as u8;
        }
        pixel[3] = 255;
    }
}

fn save(name: &str, width: i32, height: i32, bgra: &[u8]) {
    let path = PathBuf::from(OUT_DIR).join(format!("{name}.png"));
    let mut rgba = bgra.to_vec();
    bgra_to_rgba(&mut rgba);
    std::fs::write(&path, png(width as u32, height as u32, &rgba)).unwrap();
    println!("{}", path.display());
}

fn capture(hwnd: HWND, name: &str, fallback: [u8; 3]) {
    pump();
    let rect = window_rect(hwnd);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let info = bitmap_info(width, height);
    let mut bits = std::ptr::null_mut();
    unsafe {
        let dc = CreateCompatibleDC(None);
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let previous = SelectObject(dc, bitmap.into());
        let _ = PrintWindow(hwnd, dc, PRINT_WINDOW_FLAGS(PRINT_RENDER_FULL_CONTENT));
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
        composite(pixels, fallback);
        save(name, width, height, pixels);
        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
    }
}

fn park(hwnd: HWND, dpi: u32) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            PARKED_X,
            PARKED_Y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if dpi != 96 {
            let at = window_rect(hwnd);
            SendMessageW(
                hwnd,
                WM_DPICHANGED,
                Some(WPARAM((dpi | (dpi << 16)) as usize)),
                Some(LPARAM(&at as *const RECT as isize)),
            );
        }
    }
    pump();
}

fn sample_stats() -> StatsView {
    StatsView {
        today: Counts {
            shown: 48,
            accepted: 17,
            accepted_words: 9,
            words: 112,
            chars: 640,
        },
        total: Counts {
            shown: 12_480,
            accepted: 3_912,
            accepted_words: 2_207,
            words: 41_530,
            chars: 236_400,
        },
    }
}

fn settings(name: &str, apps: &[&str], rival: Option<Rival>, dpi: u32, fallback: [u8; 3]) {
    let config = Config {
        blocked_apps: apps.iter().map(|app| app.to_string()).collect(),
        ghost_opacity: 70,
        max_words: 10,
        ..Config::default()
    };
    let window = SettingsWindow::open(
        &config,
        ModelPreset::Standard,
        true,
        "Ready",
        &sample_stats(),
    )
    .unwrap();
    window.set_rival(rival);
    park(window.hwnd(), dpi);
    capture(window.hwnd(), name, fallback);
}

fn onboarding(name: &str, events: &[OnboardingEvent], dpi: u32, fallback: [u8; 3]) {
    let marker = std::env::temp_dir().join("pith-ui-shots-marker");
    let mut window = Onboarding::create(marker, false, &Reach::ThisPc).unwrap();
    let hwnd = window.controls().window;
    park(hwnd, dpi);
    for event in events {
        window.event(event.clone());
    }
    capture(hwnd, name, fallback);
}

fn control_size(hwnd: HWND) -> (i32, i32) {
    let rect = window_rect(hwnd);
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn draw_state(window: HWND, id: i32, state: u32, dc: HDC, at: (i32, i32)) {
    let control = unsafe { GetDlgItem(Some(window), id) }.unwrap();
    let (width, height) = control_size(control);
    let item = DRAWITEMSTRUCT {
        CtlType: ODT_BUTTON,
        CtlID: id as u32,
        itemAction: ODA_DRAWENTIRE,
        itemState: ODS_FLAGS(state),
        hwndItem: control,
        hDC: dc,
        rcItem: RECT {
            left: at.0,
            top: at.1,
            right: at.0 + width,
            bottom: at.1 + height,
        },
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            window,
            WM_DRAWITEM,
            Some(WPARAM(id as usize)),
            Some(LPARAM(&item as *const DRAWITEMSTRUCT as isize)),
        );
    }
}

fn states(name: &str, card: [u8; 3]) {
    const CONTROLS: [i32; 9] = [101, 101, 103, 102, 120, 112, 113, 105, 109];
    const STATES: [u32; 4] = [0, 0x10, 0x1, 0x4];
    const CELL: (i32, i32) = (150, 52);
    let config = Config {
        blocked_apps: vec!["Slack".into()],
        ..Config::default()
    };
    let window = SettingsWindow::open(
        &config,
        ModelPreset::Standard,
        true,
        "Ready",
        &StatsView::default(),
    )
    .unwrap();
    park(window.hwnd(), 96);
    let (width, height) = (CELL.0 * STATES.len() as i32, CELL.1 * CONTROLS.len() as i32);
    let info = bitmap_info(width, height);
    let mut bits = std::ptr::null_mut();
    unsafe {
        let dc = CreateCompatibleDC(None);
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let previous = SelectObject(dc, bitmap.into());
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[..3].copy_from_slice(&[card[2], card[1], card[0]]);
        }
        for (row, id) in CONTROLS.iter().enumerate() {
            if row == 1 {
                let toggle = GetDlgItem(Some(window.hwnd()), *id).unwrap();
                SendMessageW(toggle, BM_CLICK, None, None);
            }
            for (column, state) in STATES.iter().enumerate() {
                let at = (column as i32 * CELL.0, row as i32 * CELL.1);
                draw_state(window.hwnd(), *id, *state, dc, at);
            }
        }
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        save(name, width, height, pixels);
        SelectObject(dc, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
    }
}

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
    let many = ["Slack", "Figma", "Notion", "Zoom", "Teams"];
    for (theme, dark, fallback, card) in THEMES {
        force_dark_theme(Some(dark));
        for dpi in DPIS {
            let tag = format!("{theme}-{dpi}");
            settings(&format!("settings-{tag}"), &many[..3], None, dpi, fallback);
            onboarding(&format!("onboarding-practice-{tag}"), &ready, dpi, fallback);
        }
        states(&format!("states-{theme}"), card);
        settings(
            &format!("settings-empty-{theme}-96"),
            &[],
            None,
            96,
            fallback,
        );
        settings(
            &format!("settings-many-{theme}-96"),
            &many,
            None,
            96,
            fallback,
        );
        for (kind, rival) in [
            ("os", Rival::TextSuggestions),
            ("app", Rival::App("Lightkey")),
        ] {
            let name = format!("settings-rival-{kind}-{theme}-96");
            settings(&name, &many[..3], Some(rival), 96, fallback);
        }
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
