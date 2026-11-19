use std::path::PathBuf;

use pith_core::{Config, Counts, ModelPreset, Placement, Reach, Rect, Rgb, TextStyle, is_rtl};
use pith_windows::{
    Onboarding, OnboardingEvent, Picture, Rival, Sample, SettingsWindow, StatsView,
    force_dark_theme, preview,
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
const APP_BACKDROPS: [[u8; 3]; 2] = [[0xFF; 3], [0x1E; 3]];
const OVERLAY_DPIS: [u32; 3] = [96, 144, 192];
const OVERLAY_WIDTH: i32 = 560;
const OVERLAY_PAD: i32 = 20;
const CARET_HEIGHT: i32 = 20;
const CARET_WIDTH: i32 = 2;
const NARROW_FIELD: i32 = 120;
const FONT_PX: i32 = 15;
const GHOST_OPACITY: u8 = 50;
const LATIN: &str = "the quick brown fox jumps over the lazy dog";
const LONG: &str =
    "the quick brown fox jumps over the lazy dog and then keeps running past the bridge";
const ARABIC: &str = "مرحبا بك في هذا التطبيق الجديد";
const CJK: &str = "こんにちは、今日はいい天気ですね。";
const COMBINING: &str = "cafe\u{301} nai\u{308}ve resume\u{301} a\u{30a}ngstro\u{308}m";
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

enum Surface {
    Pill(&'static str, Option<&'static str>),
    Wrapped(&'static str, Option<&'static str>),
    Ghost(&'static str),
}

const OVERLAY_ROWS: [Surface; 9] = [
    Surface::Pill(LATIN, Some("Tab")),
    Surface::Pill(LATIN, None),
    Surface::Wrapped(LONG, Some("Tab")),
    Surface::Wrapped(LONG, None),
    Surface::Pill(ARABIC, Some("Tab")),
    Surface::Pill(CJK, Some("Tab")),
    Surface::Pill(COMBINING, None),
    Surface::Ghost(LATIN),
    Surface::Ghost(ARABIC),
];

fn paint_over(canvas: &mut [u8], width: i32, picture: &Picture) {
    for (index, pixel) in picture.pixels.iter().enumerate() {
        let column = picture.x + index as i32 % picture.width;
        let row = picture.y + index as i32 / picture.width;
        let at = ((row * width + column) * 4) as usize;
        let Some(slot) = canvas.get_mut(at..at + 3) else {
            continue;
        };
        let alpha = pixel >> 24;
        let source = [(pixel >> 16) & 0xFF, (pixel >> 8) & 0xFF, pixel & 0xFF];
        for (channel, value) in slot.iter_mut().zip(source) {
            *channel = (value + u32::from(*channel) * (255 - alpha) / 255).min(255) as u8;
        }
    }
}

fn overlay_row(surface: &Surface, dpi: u32, dark: bool, backdrop: [u8; 3]) -> (i32, Vec<u8>) {
    let unit = |logical: i32| logical * dpi as i32 / 96;
    let (text, placement_of, key_hint) = match surface {
        Surface::Pill(text, hint) => (*text, Placement::BelowCaret as fn(Rect) -> Placement, *hint),
        Surface::Wrapped(text, hint) => {
            (*text, Placement::Anchored as fn(Rect) -> Placement, *hint)
        }
        Surface::Ghost(text) => (*text, Placement::Inline as fn(Rect) -> Placement, None),
    };
    let side = if is_rtl(text) {
        OVERLAY_WIDTH - OVERLAY_PAD
    } else {
        OVERLAY_PAD
    };
    let caret = Rect {
        x: unit(side),
        y: unit(OVERLAY_PAD),
        width: unit(CARET_WIDTH),
        height: unit(CARET_HEIGHT),
    };
    let narrow = Rect {
        width: unit(NARROW_FIELD),
        ..caret
    };
    let ink = if dark { [0xE6; 3] } else { [0x1A; 3] };
    let style = TextStyle {
        family: Some("Segoe UI".into()),
        px: Some(unit(FONT_PX) as f32),
        color: Some(Rgb::from_channels(ink.map(u32::from))),
    };
    let placement = placement_of(if matches!(surface, Surface::Wrapped(..)) {
        narrow
    } else {
        caret
    });
    let picture = preview(&Sample {
        text,
        placement,
        style: &style,
        dpi,
        background: Rgb::from_channels(backdrop.map(u32::from)),
        opacity: GHOST_OPACITY,
        key_hint,
    })
    .unwrap();
    let width = unit(OVERLAY_WIDTH);
    let height = picture.y + picture.height + unit(OVERLAY_PAD);
    let mut pixels = vec![255u8; (width * height * 4) as usize];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[..3].copy_from_slice(&backdrop);
    }
    for row in caret.y..caret.y + caret.height {
        for column in caret.x..caret.x + caret.width {
            let at = ((row * width + column) * 4) as usize;
            pixels[at..at + 3].copy_from_slice(&ink);
        }
    }
    paint_over(&mut pixels, width, &picture);
    (height, pixels)
}

fn overlay(name: &str, dark: bool, dpi: u32, backdrop: [u8; 3]) {
    let rows: Vec<(i32, Vec<u8>)> = OVERLAY_ROWS
        .iter()
        .map(|surface| overlay_row(surface, dpi, dark, backdrop))
        .collect();
    let height: i32 = rows.iter().map(|(height, _)| height).sum();
    let pixels: Vec<u8> = rows.into_iter().flat_map(|(_, pixels)| pixels).collect();
    let width = (OVERLAY_WIDTH * dpi as i32 / 96) as u32;
    let path = PathBuf::from(OUT_DIR).join(format!("{name}.png"));
    std::fs::write(&path, png(width, height as u32, &pixels)).unwrap();
    println!("{}", path.display());
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
        for dpi in OVERLAY_DPIS {
            overlay(
                &format!("overlay-{theme}-{dpi}"),
                dark,
                dpi,
                APP_BACKDROPS[usize::from(dark)],
            );
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
