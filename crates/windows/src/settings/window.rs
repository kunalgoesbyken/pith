use pith_core::{Setting, Suggestions};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, HDC, PAINTSTRUCT, RDW_ALLCHILDREN, RDW_INVALIDATE, RedrawWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::layout::{Layout, build};
use super::model::{self, Model, current, update};
use super::view::{draw_item, name, paint_page};
use super::{
    ID_DELETE_DATA, ID_ENABLED, ID_OPEN_DATA, ID_REMOVE_FIRST, ID_START, MAX_APP_ROWS, SEGMENTS,
    UiEvent, emit,
};
use crate::ui::{self, BASE_DPI, Parent, Scale, Theme, width};
use crate::util::wide;

pub const WINDOW: WINDOW_STYLE = WINDOW_STYLE(
    WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0,
);
const WHEEL_STEP: i32 = 120;
const CLASS: PCWSTR = w!("PithSettings");

fn item(window: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(window), id) }.unwrap_or_default()
}

fn set_text(control: HWND, text: &str) {
    let text = wide(text);
    unsafe {
        let _ = SetWindowTextW(control, PCWSTR(text.as_ptr()));
    }
}

fn client(window: HWND) -> RECT {
    let mut area = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut area);
    }
    area
}

fn sync_names(window: HWND, model: &Model) {
    let slots = (0..MAX_APP_ROWS as i32).map(|slot| ID_REMOVE_FIRST + slot);
    let others = [ID_ENABLED, ID_START, ID_OPEN_DATA, ID_DELETE_DATA]
        .into_iter()
        .chain(SEGMENTS.iter().map(|(id, ..)| *id));
    for id in others.chain(slots) {
        set_text(item(window, id), &name(model, id));
    }
}

fn arrange(window: HWND, model: &Model, layout: &Layout) {
    for placed in &layout.controls {
        let control = item(window, placed.id);
        let flags = if placed.visible {
            SWP_SHOWWINDOW
        } else {
            SWP_HIDEWINDOW
        };
        unsafe {
            let _ = SetWindowPos(
                control,
                None,
                placed.bounds.left,
                placed.bounds.top,
                width(placed.bounds),
                ui::height(placed.bounds),
                flags | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    let (frame_width, frame_height) = ui::frame_size(layout.client, WINDOW, model.scale.0);
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            0,
            0,
            frame_width,
            frame_height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = RedrawWindow(Some(window), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

pub fn relayout(window: HWND) {
    let Some(model) = current() else { return };
    sync_names(window, &model);
    arrange(window, &model, &build(&model));
}

fn repaint_all(window: HWND) {
    unsafe {
        let _ = RedrawWindow(Some(window), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

fn retheme(window: HWND) {
    let theme = Theme::detect();
    let mica = ui::apply_chrome(window, &theme);
    update(|model| {
        model.theme = theme;
        model.mica = mica;
    });
    repaint_all(window);
}

fn paint_client(window: HWND, hdc: HDC) {
    let Some(model) = current() else { return };
    let layout = build(&model);
    ui::paint(hdc, client(window), model.scale, |canvas| {
        paint_page(canvas, &model, &layout);
    });
}

fn flip(window: HWND, change: impl FnOnce(&mut Model) -> UiEvent) {
    if let Some(event) = update(change) {
        emit(event);
    }
    if let Some(model) = current() {
        sync_names(window, &model);
        repaint_all(window);
    }
}

fn command(window: HWND, id: i32, code: u32) {
    if code != BN_CLICKED {
        return;
    }
    match id {
        ID_ENABLED => flip(window, |model| {
            model.enabled = !model.enabled;
            UiEvent::Apply(Setting::Enabled(model.enabled))
        }),
        ID_START => flip(window, |model| {
            model.start_with_windows = !model.start_with_windows;
            UiEvent::StartWithWindows(model.start_with_windows)
        }),
        ID_OPEN_DATA => emit(UiEvent::OpenDataFolder),
        ID_DELETE_DATA => emit(UiEvent::DeleteData),
        _ => segment_or_remove(window, id),
    }
}

fn segment_or_remove(window: HWND, id: i32) {
    if let Some((_, level, _)) = SEGMENTS.iter().find(|(segment, ..)| *segment == id) {
        let level: Suggestions = *level;
        flip(window, |model| {
            model.level = Some(level);
            UiEvent::Apply(Setting::Suggestions(level))
        });
        return;
    }
    let app = current().and_then(|model| {
        let slot = usize::try_from(id - ID_REMOVE_FIRST).ok()?;
        model.visible_apps().get(slot).cloned()
    });
    if let Some(app) = app {
        emit(UiEvent::Apply(Setting::Unblock(app)));
    }
}

fn scroll(window: HWND, wheel: i32) {
    let rows = -(wheel / WHEEL_STEP) as isize;
    if update(|model| model.scroll(rows)).is_some() {
        relayout(window);
    }
}

fn dpi_changed(window: HWND, wparam: WPARAM, lparam: LPARAM) {
    let dpi = (wparam.0 & 0xFFFF) as u32;
    update(|model| model.scale = Scale(dpi.max(BASE_DPI)));
    relayout(window);
    let suggested = unsafe { *(lparam.0 as *const RECT) };
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            suggested.left,
            suggested.top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ui::is_theme_change(message, wparam, lparam) {
        retheme(window);
        return LRESULT(0);
    }
    match message {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            if id == IDCANCEL.0 {
                unsafe {
                    let _ = DestroyWindow(window);
                }
            } else {
                command(window, id, ((wparam.0 >> 16) & 0xFFFF) as u32);
            }
            return LRESULT(0);
        }
        WM_DRAWITEM => {
            if let Some(model) = current() {
                draw_item(unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) }, &model);
            }
            return LRESULT(1);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(window, &mut paint) };
            paint_client(window, hdc);
            unsafe {
                let _ = EndPaint(window, &paint);
            }
            return LRESULT(0);
        }
        WM_PRINTCLIENT => {
            paint_client(window, HDC(wparam.0 as _));
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            scroll(window, ((wparam.0 >> 16) as u16 as i16).into());
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            dpi_changed(window, wparam, lparam);
            return LRESULT(0);
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(window);
            }
            return LRESULT(0);
        }
        WM_DESTROY => emit(UiEvent::Closed),
        WM_NCDESTROY => model::clear(),
        _ => {}
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn register(instance: HINSTANCE) {
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        hIcon: ui::app_icon(instance),
        lpszClassName: CLASS,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&class);
    }
}

pub fn create(
    config: &pith_core::Config,
    start_with_windows: bool,
    status: &str,
) -> windows::core::Result<HWND> {
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
    register(instance);
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("Pith settings"),
            WINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(instance),
            None,
        )?
    };
    let theme = Theme::detect();
    let mica = ui::apply_chrome(window, &theme);
    let scale = Scale(unsafe { GetDpiForWindow(window) }.max(BASE_DPI));
    let model = Model::new(config, start_with_windows, status, (theme, mica, scale));
    model::install(model.clone());
    let layout = build(&model);
    let parent = Parent {
        hwnd: window,
        instance,
    };
    for placed in &layout.controls {
        parent.button(placed.id, "", placed.bounds)?;
    }
    sync_names(window, &model);
    arrange(window, &model, &layout);
    Ok(window)
}
