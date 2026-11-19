use pith_core::{Chord, KeyAction, Setting};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, HDC, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MapWindowPoints, MonitorFromWindow, PAINTSTRUCT, RDW_ALLCHILDREN, RDW_INVALIDATE, RedrawWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_BACK, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::action::Action;
use super::controls::{plan, reconcile};
use super::layout::{self, DEFAULT_CLIENT, MIN_CLIENT, NAV_ID, PAGE_ID};
use super::model::{self, Model, read, update};
use super::page::Page;
use super::recorder::{WM_RECORDED, unpack};
use super::slider::{self, TBM_GETPOS};
use super::view::{draw_item, paint_page};
use super::{Overview, UiEvent, emit, picker};
use crate::keys::key_of;
use crate::ui::{self, BASE_DPI, Scale, Theme, height, rect, width};

pub const WINDOW: WINDOW_STYLE =
    WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0);
const CLASS: PCWSTR = w!("PithHome");
const TITLE: PCWSTR = w!("Pith");
const WHEEL_STEP: i32 = 120;
const PAGE_STEP: i32 = 96;
const REVEAL_MARGIN: i32 = 16;
const SCREEN_SHARE_PERCENT: i32 = 90;
const WM_RELAYOUT: u32 = WM_APP + 3;
const NO_OTHER_APPS: &str = "No other apps are open";
const UNUSABLE_KEY: &str = "That key can not be used for a shortcut.";

fn instance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None) }
        .map(Into::into)
        .unwrap_or_default()
}

fn client(window: HWND) -> RECT {
    let mut area = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut area);
    }
    area
}

fn work_area(window: HWND) -> Option<RECT> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    unsafe { GetMonitorInfoW(monitor, &mut info) }
        .as_bool()
        .then_some(info.rcWork)
}

fn frame_for(scale: Scale, logical: (i32, i32)) -> (i32, i32) {
    let content = rect(0, 0, scale.px(logical.0), scale.px(logical.1));
    ui::frame_size(content, WINDOW, scale.0)
}

fn redraw(window: HWND) {
    unsafe {
        let _ = RedrawWindow(Some(window), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

pub fn relayout(window: HWND) {
    let Some(mut built) = read(layout::build) else {
        return;
    };
    if update(|model| model.fit(built.content_height)) == Some(true) {
        built = read(layout::build).unwrap_or(built);
    }
    if update(|model| model.layout = built).is_none() {
        return;
    }
    reconcile(window, instance(), &plan());
    redraw(window);
}

fn retheme(window: HWND) {
    let theme = Theme::detect();
    let mica = ui::apply_chrome(window, &theme);
    update(|model| {
        model.theme = theme;
        model.mica = mica;
    });
    relayout(window);
}

fn paint_client(window: HWND, hdc: HDC) {
    read(|model| {
        ui::paint(hdc, client(window), model.scale, |canvas| {
            paint_page(canvas, model);
        });
    });
}

fn dispatch(window: HWND, event: UiEvent) {
    update(|model| model.absorb(&event));
    emit(event);
    relayout(window);
}

fn go(window: HWND, page: Page) {
    update(|model| model.go(page));
    relayout(window);
}

fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    let first: String = chars
        .next()
        .into_iter()
        .flat_map(char::to_uppercase)
        .collect();
    format!("{first}{}.", chars.as_str().trim_end_matches('.'))
}

fn choose_app(window: HWND, button: i32) {
    let Some(config) = read(|model| model.config.clone()) else {
        return;
    };
    let choices: Vec<(String, Setting)> = picker::running_apps()
        .into_iter()
        .filter_map(|app| {
            let setting = config.switch_app(&app)?;
            matches!(setting, Setting::Block(_) | Setting::Allow(_)).then_some((app, setting))
        })
        .collect();
    let labels: Vec<String> = choices
        .iter()
        .map(|(app, setting)| match setting {
            Setting::Allow(_) => format!("{app} (off by default)"),
            _ => app.clone(),
        })
        .collect();
    let mut bounds = RECT::default();
    unsafe {
        let _ = GetWindowRect(
            GetDlgItem(Some(window), button).unwrap_or_default(),
            &mut bounds,
        );
    }
    let picked = picker::choose(window, (bounds.left, bounds.bottom), &labels, NO_OTHER_APPS);
    if let Some((_, setting)) = picked.and_then(|index| choices.into_iter().nth(index)) {
        dispatch(window, UiEvent::Apply(setting));
    }
}

fn begin_recording(window: HWND, binding: KeyAction, slot: usize) {
    update(|model| {
        model.recording = Some((binding, slot));
        model.key_notice = None;
    });
    relayout(window);
}

fn click(window: HWND, id: i32) {
    let Some(action) = read(|model| {
        model
            .layout
            .control(id)
            .map(|placed| placed.item.action.clone())
    })
    .flatten() else {
        return;
    };
    match action {
        Action::None | Action::Opacity => {}
        Action::Go(page) => go(window, page),
        Action::Apply(setting) => dispatch(window, UiEvent::Apply(setting)),
        Action::Emit(event) => dispatch(window, event),
        Action::Record(binding, slot) => begin_recording(window, binding, slot),
        Action::ChooseApp => choose_app(window, id),
    }
}

fn slide(window: HWND, slider: HWND, released: bool) {
    let value = unsafe { SendMessageW(slider, TBM_GETPOS, None, None) }.0 as u8;
    if released {
        update(|model| model.preview = None);
        dispatch(window, UiEvent::Apply(Setting::GhostOpacity(value)));
        return;
    }
    update(|model| model.preview = Some(value));
    relayout(window);
}

fn rebind(window: HWND, (binding, slot): (KeyAction, usize), chord: Option<Chord>) {
    let outcome = read(|model| model.config.rebind(binding, slot, chord));
    match outcome {
        Some(Ok(_)) => {
            update(|model| {
                model.recording = None;
                model.key_notice = None;
            });
            dispatch(window, UiEvent::Apply(Setting::Bind(binding, slot, chord)));
        }
        Some(Err(message)) => {
            update(|model| {
                model.recording = None;
                model.key_notice = Some(sentence(&message));
            });
            relayout(window);
        }
        None => {}
    }
}

fn recorded(window: HWND, packed: LPARAM) {
    let Some(target) = read(|model| model.recording).flatten() else {
        return;
    };
    let pressed = unpack(packed);
    let plain = !(pressed.ctrl || pressed.shift || pressed.alt);
    match (pressed.key, plain) {
        (key, true) if key == VK_ESCAPE.0 => {
            update(|model| model.recording = None);
            relayout(window);
        }
        (key, true) if key == VK_BACK.0 => rebind(window, target, None),
        _ => match key_of(pressed.key) {
            Some(key) => {
                let chord = Chord {
                    ctrl: pressed.ctrl,
                    shift: pressed.shift,
                    alt: pressed.alt,
                    win: false,
                    key,
                };
                rebind(window, target, Some(chord));
            }
            None => {
                update(|model| model.key_notice = Some(UNUSABLE_KEY.to_string()));
                relayout(window);
            }
        },
    }
}

fn recorder_lost_focus(window: HWND) {
    if update(|model| model.recording.take()).flatten().is_some() {
        unsafe {
            let _ = PostMessageW(Some(window), WM_RELAYOUT, WPARAM(0), LPARAM(0));
        }
    }
}

fn reveal(window: HWND, control: HWND) {
    let id = unsafe { GetDlgCtrlID(control) };
    if id < PAGE_ID {
        return;
    }
    let mut bounds = RECT::default();
    unsafe {
        let _ = GetWindowRect(control, &mut bounds);
        let corners = std::slice::from_raw_parts_mut(&mut bounds as *mut RECT as *mut POINT, 2);
        MapWindowPoints(None, Some(window), corners);
    }
    let area = client(window);
    let Some(margin) = read(|model| model.scale.px(REVEAL_MARGIN)) else {
        return;
    };
    let by = if bounds.top < margin {
        bounds.top - margin
    } else if bounds.bottom > area.bottom - margin {
        bounds.bottom - area.bottom + margin
    } else {
        return;
    };
    if update(|model| model.scroll_by(by)) == Some(true) {
        relayout(window);
    }
}

fn scroll(window: HWND, wheel: i32) {
    let Some(scale) = read(|model| model.scale) else {
        return;
    };
    let by = -wheel * scale.px(PAGE_STEP) / WHEEL_STEP;
    if update(|model| model.scroll_by(by)) == Some(true) {
        relayout(window);
    }
}

fn command(window: HWND, wparam: WPARAM, lparam: LPARAM) {
    let id = (wparam.0 & 0xFFFF) as i32;
    let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
    let control = HWND(lparam.0 as _);
    match code {
        _ if id == IDCANCEL.0 && lparam.0 == 0 => unsafe {
            let _ = DestroyWindow(window);
        },
        BN_CLICKED => click(window, id),
        BN_SETFOCUS => reveal(window, control),
        BN_KILLFOCUS => recorder_lost_focus(window),
        slider::MOVED => slide(window, control, false),
        slider::RELEASED => slide(window, control, true),
        _ => {}
    }
}

fn dpi_changed(window: HWND, wparam: WPARAM, lparam: LPARAM) {
    let dpi = (wparam.0 & 0xFFFF) as u32;
    update(|model| model.scale = Scale(dpi.max(BASE_DPI)));
    let suggested = unsafe { *(lparam.0 as *const RECT) };
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            suggested.left,
            suggested.top,
            width(suggested),
            height(suggested),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    relayout(window);
}

fn resized(window: HWND, lparam: LPARAM) {
    let size = (
        (lparam.0 & 0xFFFF) as i32,
        ((lparam.0 >> 16) & 0xFFFF) as i32,
    );
    update(|model| model.viewport = size);
    relayout(window);
}

fn limit_size(window: HWND, lparam: LPARAM) {
    let Some(scale) = read(|model| model.scale) else {
        return;
    };
    let (wanted_width, wanted_height) = frame_for(scale, MIN_CLIENT);
    let room = work_area(window).map_or((i32::MAX, i32::MAX), |work| (width(work), height(work)));
    let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
    info.ptMinTrackSize = POINT {
        x: wanted_width.min(room.0),
        y: wanted_height.min(room.1),
    };
}

fn draw_control(lparam: LPARAM) {
    let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
    read(|model| draw_item(item, model));
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
        WM_COMMAND => command(window, wparam, lparam),
        WM_DRAWITEM => {
            draw_control(lparam);
            return LRESULT(1);
        }
        WM_RECORDED => recorded(window, lparam),
        WM_RELAYOUT => relayout(window),
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(window, &mut paint) };
            paint_client(window, hdc);
            unsafe {
                let _ = EndPaint(window, &paint);
            }
        }
        WM_PRINTCLIENT => paint_client(window, HDC(wparam.0 as _)),
        WM_MOUSEWHEEL => scroll(window, ((wparam.0 >> 16) as u16 as i16).into()),
        WM_SIZE if wparam.0 as u32 != SIZE_MINIMIZED => resized(window, lparam),
        WM_GETMINMAXINFO => limit_size(window, lparam),
        WM_DPICHANGED => dpi_changed(window, wparam, lparam),
        WM_CLOSE => unsafe {
            let _ = DestroyWindow(window);
        },
        WM_DESTROY => emit(UiEvent::Closed),
        WM_NCDESTROY => model::clear(),
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    LRESULT(0)
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

fn place(window: HWND, scale: Scale) {
    let (frame_width, frame_height) = frame_for(scale, DEFAULT_CLIENT);
    let (frame_width, frame_height, left, top) = match work_area(window) {
        Some(work) => {
            let shown = (
                frame_width.min(width(work) * SCREEN_SHARE_PERCENT / 100),
                frame_height.min(height(work) * SCREEN_SHARE_PERCENT / 100),
            );
            (
                shown.0,
                shown.1,
                work.left + (width(work) - shown.0) / 2,
                work.top + (height(work) - shown.1) / 2,
            )
        }
        None => (frame_width, frame_height, 0, 0),
    };
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            left,
            top,
            frame_width,
            frame_height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn create(
    config: &pith_core::Config,
    recommended: pith_core::ModelPreset,
    overview: &Overview,
) -> windows::core::Result<HWND> {
    let instance = instance();
    register(instance);
    slider::register(instance);
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
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
    let viewport = (scale.px(DEFAULT_CLIENT.0), scale.px(DEFAULT_CLIENT.1));
    model::install(Model::new(
        (config, recommended),
        overview,
        (theme, mica, scale),
        viewport,
    ));
    place(window, scale);
    relayout(window);
    unsafe {
        SendMessageW(
            window,
            WM_CHANGEUISTATE,
            Some(WPARAM(UIS_SET as usize | (UISF_HIDEFOCUS as usize) << 16)),
            None,
        );
        let _ = SetFocus(GetDlgItem(Some(window), NAV_ID).ok());
    }
    Ok(window)
}
