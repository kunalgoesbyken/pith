use std::cell::Cell;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, HDC, HFONT, PAINTSTRUCT, RDW_ALLCHILDREN, RDW_INVALIDATE,
    RedrawWindow,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::Controls::RichEdit::{
    CFM_COLOR, CHARFORMAT2W, CHARFORMATW, EM_SETBKGNDCOLOR, EM_SETCHARFORMAT, SCF_ALL, SCF_DEFAULT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::layout::{DONE_LABEL, HINT, KEYS_NAME, Layout, build};
use super::model::{self, Model, current, update};
use super::step::{Event, View};
use super::view::{draw_item, paint_page};
use super::{Controls, DONE_ID, HINT_ID, KEYS_ID, PRACTICE_ID, PROGRESS_ID, STATUS_ID};
use crate::ui::{self, BASE_DPI, Parent, Role, Scale, Theme, height, width};

const WINDOW: WINDOW_STYLE =
    WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0);
const CLASS: PCWSTR = w!("PithOnboarding");
const EDIT_STYLE: u32 = (ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32 | WS_TABSTOP.0;
const SCREEN_TOP_DIVISOR: i32 = 3;

thread_local! {
    static COMMAND: Cell<Option<Event>> = const { Cell::new(None) };
    static FONT: Cell<Option<HFONT>> = const { Cell::new(None) };
}

pub fn take_command() -> Option<Event> {
    COMMAND.with(Cell::take)
}

pub fn release_font() {
    if let Some(font) = FONT.with(Cell::take) {
        unsafe {
            let _ = DeleteObject(font.into());
        }
    }
}

fn client(window: HWND) -> RECT {
    let mut area = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut area);
    }
    area
}

fn place(window: HWND, id: usize, bounds: RECT, visible: bool) {
    let control = unsafe { GetDlgItem(Some(window), id as i32) }.unwrap_or_default();
    let flags = if visible {
        SWP_SHOWWINDOW
    } else {
        SWP_HIDEWINDOW
    };
    unsafe {
        let _ = SetWindowPos(
            control,
            None,
            bounds.left,
            bounds.top,
            width(bounds),
            height(bounds),
            flags | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn arrange(window: HWND, model: &Model, layout: &Layout) {
    let view: &View = &model.view;
    place(window, STATUS_ID, layout.status, true);
    place(
        window,
        PROGRESS_ID,
        layout.progress,
        view.permille.is_some(),
    );
    place(window, HINT_ID, layout.hint, view.practice_enabled);
    place(window, PRACTICE_ID, layout.practice, view.practice_enabled);
    place(window, KEYS_ID, layout.keys, true);
    place(window, DONE_ID, layout.done, true);
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
    arrange(window, &model, &build(&model));
}

fn style_practice(window: HWND, model: &Model) {
    let edit = unsafe { GetDlgItem(Some(window), PRACTICE_ID as i32) }.unwrap_or_default();
    let palette = &model.theme.palette;
    let format = CHARFORMAT2W {
        Base: CHARFORMATW {
            cbSize: size_of::<CHARFORMAT2W>() as u32,
            dwMask: CFM_COLOR,
            crTextColor: ui::colorref(palette.text),
            ..Default::default()
        },
        ..Default::default()
    };
    let font = ui::gdi_font(Role::Body, model.scale);
    unsafe {
        if let Some(font) = font {
            SendMessageW(
                edit,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
        SendMessageW(
            edit,
            EM_SETBKGNDCOLOR,
            Some(WPARAM(0)),
            Some(LPARAM(ui::colorref(palette.card).0 as isize)),
        );
        for scope in [SCF_DEFAULT, SCF_ALL] {
            SendMessageW(
                edit,
                EM_SETCHARFORMAT,
                Some(WPARAM(scope as usize)),
                Some(LPARAM(&format as *const CHARFORMAT2W as isize)),
            );
        }
    }
    if let Some(old) = FONT.with(|cell| cell.replace(font)) {
        unsafe {
            let _ = DeleteObject(old.into());
        }
    }
}

fn retheme(window: HWND) {
    let theme = Theme::detect();
    let mica = ui::apply_chrome(window, &theme);
    update(|model| {
        model.theme = theme;
        model.mica = mica;
    });
    if let Some(model) = current() {
        style_practice(window, &model);
    }
    unsafe {
        let _ = RedrawWindow(Some(window), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

fn dpi_changed(window: HWND, wparam: WPARAM, lparam: LPARAM) {
    let dpi = (wparam.0 & 0xFFFF) as u32;
    update(|model| model.scale = Scale(dpi.max(BASE_DPI)));
    if let Some(model) = current() {
        style_practice(window, &model);
    }
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

fn paint_client(window: HWND, hdc: HDC) {
    let Some(model) = current() else { return };
    let layout = build(&model);
    ui::paint(hdc, client(window), model.scale, |canvas| {
        paint_page(canvas, &model, &layout);
    });
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
    let command = match message {
        WM_COMMAND if wparam.0 & 0xFFFF == DONE_ID => Some(Event::Done),
        WM_CLOSE => Some(Event::Close),
        _ => None,
    };
    if let Some(command) = command {
        COMMAND.with(|cell| cell.set(Some(command)));
        return LRESULT(0);
    }
    match message {
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
        WM_DPICHANGED => {
            dpi_changed(window, wparam, lparam);
            return LRESULT(0);
        }
        WM_NCDESTROY => model::clear(),
        _ => {}
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn register(instance: HINSTANCE) -> windows::core::Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        hIcon: ui::app_icon(instance),
        lpszClassName: CLASS,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&class);
    }
    Ok(())
}

fn center(window: HWND, layout: &Layout, scale: Scale) {
    let (frame_width, frame_height) = ui::frame_size(layout.client, WINDOW, scale.0);
    unsafe {
        let _ = SetWindowPos(
            window,
            None,
            (GetSystemMetrics(SM_CXSCREEN) - frame_width) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - frame_height) / SCREEN_TOP_DIVISOR,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn create(initial: &View, promise: String) -> windows::core::Result<Controls> {
    unsafe {
        let _ = LoadLibraryW(w!("Msftedit.dll"));
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        register(instance)?;
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("Welcome to Pith"),
            WINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(instance),
            None,
        )?;
        let theme = Theme::detect();
        let mica = ui::apply_chrome(window, &theme);
        let scale = Scale(GetDpiForWindow(window).max(BASE_DPI));
        let model = Model {
            theme,
            mica,
            scale,
            view: initial.clone(),
            promise,
        };
        model::install(model.clone());
        let layout = build(&model);
        let parent = Parent {
            hwnd: window,
            instance,
        };
        let status = parent.label(STATUS_ID as i32, "", layout.status)?;
        let progress = parent.label(PROGRESS_ID as i32, "", layout.progress)?;
        let hint = parent.label(HINT_ID as i32, HINT, layout.hint)?;
        let practice = parent.child(
            w!("RICHEDIT50W"),
            "",
            EDIT_STYLE,
            PRACTICE_ID as i32,
            layout.practice,
        )?;
        ui::make_opaque(practice);
        let keys = parent.label(KEYS_ID as i32, KEYS_NAME, layout.keys)?;
        let done = parent.button(DONE_ID as i32, DONE_LABEL, layout.done)?;
        style_practice(window, &model);
        arrange(window, &model, &layout);
        center(window, &layout, scale);
        Ok(Controls {
            window,
            status,
            progress,
            hint,
            practice,
            keys,
            done,
        })
    }
}
