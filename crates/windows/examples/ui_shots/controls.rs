use pith_core::Config;
use pith_windows::Page;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODA_DRAWENTIRE, ODS_FLAGS, ODT_BUTTON};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture::{park, pump, resize, save};
use crate::common;
use crate::common::{bitmap_info, window_rect};
use crate::settings::{FULL_PAGE, home, sample_overview};

fn control_size(hwnd: HWND) -> (i32, i32) {
    let rect = window_rect(hwnd);
    (rect.right - rect.left, rect.bottom - rect.top)
}

struct Search {
    name: Vec<u16>,
    found: HWND,
}

unsafe extern "system" fn match_name(child: HWND, data: LPARAM) -> windows::core::BOOL {
    let search = unsafe { &mut *(data.0 as *mut Search) };
    let title = common::window_title(child);
    let hit = title.encode_utf16().collect::<Vec<u16>>() == search.name;
    if hit {
        search.found = child;
    }
    windows::core::BOOL::from(!hit)
}

fn find_control(window: HWND, name: &str) -> HWND {
    let mut search = Search {
        name: name.encode_utf16().collect(),
        found: HWND::default(),
    };
    unsafe {
        let _ = EnumChildWindows(
            Some(window),
            Some(match_name),
            LPARAM(&mut search as *mut Search as isize),
        );
    }
    search.found
}

fn draw_state(window: HWND, control: HWND, state: u32, dc: HDC, at: (i32, i32)) {
    let id = unsafe { GetDlgCtrlID(control) };
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

pub(crate) fn states(name: &str, card: [u8; 3]) {
    const SAMPLES: [(Page, &str, bool); 10] = [
        (Page::Home, "Suggest as I type", false),
        (Page::Home, "Suggest as I type", true),
        (Page::General, "Fewer", false),
        (Page::General, "Normal", false),
        (Page::General, "Ghost text opacity", false),
        (Page::Shortcuts, "Keep the suggestion: Tab", false),
        (Page::Apps, "Turn Pith on in Slack", false),
        (Page::Privacy, "Delete model and data", false),
        (Page::Model, "Use: Large model", false),
        (Page::Home, "General", false),
    ];
    const STATES: [u32; 4] = [0, 0x10, 0x1, 0x4];
    const CELL: (i32, i32) = (260, 52);
    let config = Config {
        blocked_apps: vec!["Slack".into()],
        ..Config::default()
    };
    let window = home(&config, &sample_overview(), None);
    park(window.hwnd(), 96);
    resize(window.hwnd(), 96, FULL_PAGE);
    let (width, height) = (CELL.0 * STATES.len() as i32, CELL.1 * SAMPLES.len() as i32);
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
        for (row, (page, control_name, flip)) in SAMPLES.iter().enumerate() {
            window.show_page(*page);
            pump();
            let mut control = find_control(window.hwnd(), control_name);
            if *flip {
                SendMessageW(control, BM_CLICK, None, None);
                pump();
                control = find_control(window.hwnd(), control_name);
            }
            for (column, state) in STATES.iter().enumerate() {
                let at = (column as i32 * CELL.0, row as i32 * CELL.1);
                draw_state(window.hwnd(), control, *state, dc, at);
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
