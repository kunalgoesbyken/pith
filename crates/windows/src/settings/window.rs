use pith_core::Setting;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, HDC, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MapWindowPoints, MonitorFromWindow, PAINTSTRUCT, PtInRect, RDW_ALLCHILDREN, RDW_INVALIDATE,
    RedrawWindow, ScreenToClient,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODA_FOCUS, ODS_FOCUS};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::layout::{Layout, build};
use super::model::{self, Model, current, update};
use super::view::{draw_item, name, paint_page};
use super::{
    ID_DELETE_DATA, ID_ENABLED, ID_OPACITY_DOWN, ID_OPACITY_UP, ID_OPEN_DATA, ID_REMOVE_FIRST,
    ID_RESET_STATS, ID_RIVAL_OPEN, ID_START, MAX_APP_ROWS, StatsView, UiEvent, emit, segments,
};
use crate::ui::{self, BASE_DPI, Parent, Scale, Theme, width};
use crate::util::wide;

pub const WINDOW: WINDOW_STYLE = WINDOW_STYLE(
    WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0,
);
const WHEEL_STEP: i32 = 120;
const PAGE_STEP: i32 = 96;
const REVEAL_MARGIN: i32 = 16;
const WM_REVEAL: u32 = WM_APP + 1;
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
    let others = [
        ID_ENABLED,
        ID_START,
        ID_OPEN_DATA,
        ID_DELETE_DATA,
        ID_RESET_STATS,
        ID_RIVAL_OPEN,
        ID_OPACITY_DOWN,
        ID_OPACITY_UP,
    ]
    .into_iter()
    .chain(segments().map(|(id, ..)| *id));
    for id in others.chain(slots) {
        set_text(item(window, id), &name(model, id));
    }
}

fn work_area(window: HWND) -> Option<RECT> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    unsafe { GetMonitorInfoW(monitor, &mut info) }
        .as_bool()
        .then_some(info.rcWork)
}

fn fit_to_work_area(window: HWND, scale: Scale, content: RECT) -> i32 {
    let (frame_width, full_height) = ui::frame_size(content, WINDOW, scale.0);
    let chrome = full_height - ui::height(content);
    let mut frame = RECT::default();
    unsafe {
        let _ = GetWindowRect(window, &mut frame);
    }
    let (frame_height, top) = match work_area(window) {
        Some(work) => {
            let frame_height = full_height.min(ui::height(work));
            let top = frame.top.min(work.bottom - frame_height).max(work.top);
            (frame_height, top)
        }
        None => (full_height, frame.top),
    };
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            frame.left,
            top,
            frame_width,
            frame_height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    frame_height - chrome
}

fn arrange(window: HWND) {
    let Some(model) = current() else { return };
    let content = build(&model).client;
    let visible = fit_to_work_area(window, model.scale, content);
    update(|model| model.set_page_room(ui::height(content) - visible));
    let Some(model) = current() else { return };
    place_controls(window, &build(&model));
    unsafe {
        let _ = RedrawWindow(Some(window), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

fn place_controls(window: HWND, layout: &Layout) {
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
}

pub fn relayout(window: HWND) {
    let Some(model) = current() else { return };
    sync_names(window, &model);
    arrange(window);
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

fn flip(window: HWND, change: impl FnOnce(&mut Model) -> Option<UiEvent>) {
    if let Some(event) = update(change).flatten() {
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
            Some(UiEvent::Apply(Setting::Enabled(model.enabled)))
        }),
        ID_START => flip(window, |model| {
            model.start_with_windows = !model.start_with_windows;
            Some(UiEvent::StartWithWindows(model.start_with_windows))
        }),
        ID_OPEN_DATA => emit(UiEvent::OpenDataFolder),
        ID_DELETE_DATA => emit(UiEvent::DeleteData),
        ID_RESET_STATS => emit(UiEvent::ResetStats),
        ID_RIVAL_OPEN => emit(UiEvent::OpenTypingSettings),
        ID_OPACITY_DOWN | ID_OPACITY_UP => flip(window, |model| {
            let percent = model.step_opacity(id == ID_OPACITY_UP)?;
            Some(UiEvent::Apply(Setting::GhostOpacity(percent)))
        }),
        _ => segment_or_remove(window, id),
    }
}

fn segment_or_remove(window: HWND, id: i32) {
    if let Some((_, choice, _)) = segments().find(|(segment, ..)| *segment == id) {
        let choice = *choice;
        flip(window, |model| {
            model.select(choice);
            Some(UiEvent::Apply(choice.setting()))
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

fn over_app_list(window: HWND, model: &Model, lparam: LPARAM) -> bool {
    let mut cursor = POINT {
        x: (lparam.0 & 0xFFFF) as i16 as i32,
        y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
    };
    unsafe {
        let _ = ScreenToClient(window, &mut cursor);
        PtInRect(&build(model).app_list, cursor).as_bool()
    }
}

fn scroll(window: HWND, wheel: i32, lparam: LPARAM) {
    let Some(model) = current() else { return };
    let rows = -(wheel / WHEEL_STEP) as isize;
    let list_moved = over_app_list(window, &model, lparam)
        && update(|model| model.scroll(rows)).unwrap_or(false);
    let page_by = -wheel * model.scale.px(PAGE_STEP) / WHEEL_STEP;
    if list_moved || update(|model| model.scroll_page(page_by)).unwrap_or(false) {
        relayout(window);
    }
}

fn reveal(window: HWND, control: HWND) {
    let mut bounds = RECT::default();
    let mut client = RECT::default();
    unsafe {
        let _ = GetWindowRect(control, &mut bounds);
        let corners = std::slice::from_raw_parts_mut(&mut bounds as *mut RECT as *mut POINT, 2);
        MapWindowPoints(None, Some(window), corners);
        let _ = GetClientRect(window, &mut client);
    }
    let Some(margin) = current().map(|model| model.scale.px(REVEAL_MARGIN)) else {
        return;
    };
    let by = if bounds.top < margin {
        bounds.top - margin
    } else if bounds.bottom > client.bottom - margin {
        bounds.bottom - client.bottom + margin
    } else {
        return;
    };
    if update(|model| model.scroll_page(by)).unwrap_or(false) {
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
            let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            if let Some(model) = current() {
                draw_item(item, &model);
            }
            let gained_focus =
                item.itemAction.0 & ODA_FOCUS.0 != 0 && item.itemState.0 & ODS_FOCUS.0 != 0;
            if gained_focus {
                unsafe {
                    let _ = PostMessageW(
                        Some(window),
                        WM_REVEAL,
                        WPARAM(item.hwndItem.0 as usize),
                        LPARAM(0),
                    );
                }
            }
            return LRESULT(1);
        }
        WM_REVEAL => {
            reveal(window, HWND(wparam.0 as _));
            return LRESULT(0);
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
            scroll(window, ((wparam.0 >> 16) as u16 as i16).into(), lparam);
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
    recommended: pith_core::ModelPreset,
    start_with_windows: bool,
    status: &str,
    stats: &StatsView,
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
    let model = Model::new(
        config,
        recommended,
        start_with_windows,
        status,
        stats,
        (theme, mica, scale),
    );
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
    arrange(window);
    Ok(window)
}
