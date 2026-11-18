use std::cell::{Cell, RefCell};

use pith_core::{Config, Setting, Suggestions};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, CreateFontIndirectW, CreateSolidBrush, DeleteObject, FillRect, GetSysColorBrush,
    HBRUSH, HDC, HFONT, HGDIOBJ, SetBkColor, SetTextColor,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED, SetWindowTheme};
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForWindow, SystemParametersInfoForDpi,
};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::registry::apps_use_dark_theme;
use crate::util::wide;

const ID_ENABLED: i32 = 101;
const ID_FEWER: i32 = 102;
const ID_NORMAL: i32 = 103;
const ID_MORE: i32 = 104;
const ID_LIST: i32 = 110;
const ID_REMOVE: i32 = 111;
const ID_OPEN_DATA: i32 = 112;
const ID_DELETE_DATA: i32 = 113;
const ID_START: i32 = 114;
const LEVELS: [(i32, Suggestions, &str); 3] = [
    (ID_FEWER, Suggestions::Fewer, "Fewer"),
    (ID_NORMAL, Suggestions::Normal, "Normal"),
    (ID_MORE, Suggestions::More, "More"),
];

const CLIENT_WIDTH: i32 = 360;
const CLIENT_HEIGHT: i32 = 326;
const MARGIN: i32 = 16;
const WINDOW: WINDOW_STYLE =
    WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_CLIPCHILDREN.0);
const DARK_BACKGROUND: u32 = 0x0020_2020;
const DARK_TEXT: u32 = 0x00F0_F0F0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    Apply(Setting),
    OpenDataFolder,
    DeleteData,
    StartWithWindows(bool),
    Closed,
}

thread_local! {
    static EVENTS: RefCell<Vec<UiEvent>> = const { RefCell::new(Vec::new()) };
    static DARK_BRUSH: Cell<Option<HBRUSH>> = const { Cell::new(None) };
}

pub fn take_events() -> Vec<UiEvent> {
    EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

fn emit(event: UiEvent) {
    EVENTS.with(|events| events.borrow_mut().push(event));
}

fn dark_brush() -> Option<HBRUSH> {
    DARK_BRUSH.with(Cell::get)
}

fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))).0 }
}

fn item(window: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(window), id) }.unwrap_or_default()
}

fn is_checked(window: HWND, id: i32) -> bool {
    send(item(window, id), BM_GETCHECK, 0, 0) == BST_CHECKED.0 as isize
}

fn set_checked(window: HWND, id: i32, checked: bool) {
    let state = if checked { BST_CHECKED } else { BST_UNCHECKED };
    send(item(window, id), BM_SETCHECK, state.0 as usize, 0);
}

fn selected_app(window: HWND) -> Option<String> {
    let list = item(window, ID_LIST);
    let index = usize::try_from(send(list, LB_GETCURSEL, 0, 0)).ok()?;
    let length = usize::try_from(send(list, LB_GETTEXTLEN, index, 0)).ok()?;
    let mut buffer = vec![0u16; length + 1];
    send(list, LB_GETTEXT, index, buffer.as_mut_ptr() as isize);
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn sync_remove_button(window: HWND) {
    let selected = send(item(window, ID_LIST), LB_GETCURSEL, 0, 0) >= 0;
    unsafe {
        let _ = EnableWindow(item(window, ID_REMOVE), selected);
    }
}

fn command(window: HWND, id: i32, code: u32) {
    match (id, code) {
        (ID_ENABLED, BN_CLICKED) => emit(UiEvent::Apply(Setting::Enabled(is_checked(
            window, ID_ENABLED,
        )))),
        (ID_FEWER | ID_NORMAL | ID_MORE, BN_CLICKED) => {
            if let Some((_, level, _)) = LEVELS.iter().find(|(level_id, ..)| *level_id == id) {
                emit(UiEvent::Apply(Setting::Suggestions(*level)));
            }
        }
        (ID_LIST, LBN_SELCHANGE) => sync_remove_button(window),
        (ID_REMOVE, BN_CLICKED) => {
            if let Some(app) = selected_app(window) {
                emit(UiEvent::Apply(Setting::Unblock(app)));
            }
        }
        (ID_OPEN_DATA, BN_CLICKED) => emit(UiEvent::OpenDataFolder),
        (ID_DELETE_DATA, BN_CLICKED) => emit(UiEvent::DeleteData),
        (ID_START, BN_CLICKED) => emit(UiEvent::StartWithWindows(is_checked(window, ID_START))),
        _ => {}
    }
}

unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_COMMAND => {
            command(
                window,
                (wparam.0 & 0xFFFF) as i32,
                ((wparam.0 >> 16) & 0xFFFF) as u32,
            );
            return LRESULT(0);
        }
        WM_ERASEBKGND => {
            let brush = dark_brush().unwrap_or_else(|| unsafe { GetSysColorBrush(COLOR_BTNFACE) });
            let mut area = RECT::default();
            unsafe {
                let _ = GetClientRect(window, &mut area);
                FillRect(HDC(wparam.0 as _), &area, brush);
            }
            return LRESULT(1);
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX => {
            if let Some(brush) = dark_brush() {
                unsafe {
                    let hdc = HDC(wparam.0 as _);
                    SetTextColor(hdc, COLORREF(DARK_TEXT));
                    SetBkColor(hdc, COLORREF(DARK_BACKGROUND));
                }
                return LRESULT(brush.0 as isize);
            }
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(window);
            }
            return LRESULT(0);
        }
        WM_DESTROY => emit(UiEvent::Closed),
        _ => {}
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn high_contrast() -> bool {
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_ok() && contrast.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0
}

fn scaled(logical: i32, dpi: u32) -> i32 {
    (logical * dpi as i32 + 48) / 96
}

fn system_font(dpi: u32) -> Option<HFONT> {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some((&mut metrics as *mut NONCLIENTMETRICSW).cast()),
            0,
            dpi,
        )
        .ok()?;
        let font = CreateFontIndirectW(&metrics.lfMessageFont);
        (!font.is_invalid()).then_some(font)
    }
}

struct Layout {
    parent: HWND,
    instance: HINSTANCE,
    dpi: u32,
    font: Option<HFONT>,
    dark: bool,
}

impl Layout {
    fn control(
        &self,
        class: PCWSTR,
        text: &str,
        style: u32,
        extended: WINDOW_EX_STYLE,
        id: i32,
        (x, y, width, height): (i32, i32, i32, i32),
    ) {
        let text = wide(text);
        let created = unsafe {
            CreateWindowExW(
                extended,
                class,
                PCWSTR(text.as_ptr()),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
                scaled(x, self.dpi),
                scaled(y, self.dpi),
                scaled(width, self.dpi),
                scaled(height, self.dpi),
                Some(self.parent),
                Some(HMENU(id as isize as _)),
                Some(self.instance),
                None,
            )
        };
        let Ok(control) = created else { return };
        if let Some(font) = self.font {
            send(control, WM_SETFONT, font.0 as usize, 1);
        }
        if self.dark && class != w!("STATIC") {
            unsafe {
                let _ = SetWindowTheme(control, w!("DarkMode_Explorer"), PCWSTR::null());
            }
        }
    }

    fn label(&self, text: &str, y: i32) {
        self.control(
            w!("STATIC"),
            text,
            WS_GROUP.0,
            WINDOW_EX_STYLE(0),
            -1,
            (MARGIN, y, CLIENT_WIDTH - 2 * MARGIN, 18),
        );
    }

    fn check(&self, text: &str, id: i32, y: i32) {
        self.control(
            w!("BUTTON"),
            text,
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP.0,
            WINDOW_EX_STYLE(0),
            id,
            (MARGIN, y, CLIENT_WIDTH - 2 * MARGIN, 20),
        );
    }

    fn push(&self, text: &str, id: i32, rect: (i32, i32, i32, i32)) {
        self.control(
            w!("BUTTON"),
            text,
            BS_PUSHBUTTON as u32 | WS_TABSTOP.0,
            WINDOW_EX_STYLE(0),
            id,
            rect,
        );
    }
}

fn register(instance: HINSTANCE) {
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: w!("PithSettings"),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&class);
    }
}

fn install_dark_brush(dark: bool) {
    DARK_BRUSH.with(|cell| {
        if let Some(old) = cell.take() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(old.0));
            }
        }
        if dark {
            cell.set(Some(unsafe { CreateSolidBrush(COLORREF(DARK_BACKGROUND)) }));
        }
    });
}

pub struct SettingsWindow {
    hwnd: HWND,
    font: Option<HFONT>,
}

impl SettingsWindow {
    pub fn open(config: &Config, start_with_windows: bool) -> windows::core::Result<Self> {
        let dark = apps_use_dark_theme() && !high_contrast();
        install_dark_brush(dark);
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
        register(instance);
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("PithSettings"),
                w!("Pith settings"),
                WINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CLIENT_WIDTH,
                CLIENT_HEIGHT,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: scaled(CLIENT_WIDTH, dpi),
            bottom: scaled(CLIENT_HEIGHT, dpi),
        };
        unsafe {
            let _ = AdjustWindowRectExForDpi(&mut frame, WINDOW, false, WINDOW_EX_STYLE(0), dpi);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                frame.right - frame.left,
                frame.bottom - frame.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let enabled = i32::from(dark);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&enabled as *const i32).cast(),
                size_of::<i32>() as u32,
            );
        }
        let font = system_font(dpi);
        let layout = Layout {
            parent: hwnd,
            instance,
            dpi,
            font,
            dark,
        };
        layout.check("Enabled", ID_ENABLED, 16);
        layout.label("Suggestions", 48);
        for (index, (id, _, name)) in LEVELS.into_iter().enumerate() {
            let first = if index == 0 { WS_GROUP.0 } else { 0 };
            layout.control(
                w!("BUTTON"),
                name,
                BS_AUTORADIOBUTTON as u32 | WS_TABSTOP.0 | first,
                WINDOW_EX_STYLE(0),
                id,
                (MARGIN + index as i32 * 110, 68, 104, 20),
            );
        }
        layout.label("Turned off in these apps", 100);
        layout.control(
            w!("LISTBOX"),
            "",
            LBS_NOTIFY as u32 | WS_VSCROLL.0 | WS_TABSTOP.0,
            WS_EX_CLIENTEDGE,
            ID_LIST,
            (MARGIN, 122, 236, 96),
        );
        layout.push("Remove", ID_REMOVE, (262, 122, 82, 26));
        layout.push("Open data folder", ID_OPEN_DATA, (MARGIN, 232, 160, 28));
        layout.push("Delete model and data", ID_DELETE_DATA, (184, 232, 160, 28));
        layout.check("Start with Windows", ID_START, 272);
        layout.label("Runs on this PC. Nothing leaves it.", 298);
        let window = Self { hwnd, font };
        window.refresh(config, start_with_windows);
        Ok(window)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn is_open(&self) -> bool {
        unsafe { IsWindow(Some(self.hwnd)).as_bool() }
    }

    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    pub fn refresh(&self, config: &Config, start_with_windows: bool) {
        set_checked(self.hwnd, ID_ENABLED, config.enabled);
        set_checked(self.hwnd, ID_START, start_with_windows);
        let current = Suggestions::of(config);
        for (id, level, _) in LEVELS {
            set_checked(self.hwnd, id, current == Some(level));
        }
        let list = item(self.hwnd, ID_LIST);
        send(list, LB_RESETCONTENT, 0, 0);
        for app in &config.blocked_apps {
            let name = wide(app);
            send(list, LB_ADDSTRING, 0, name.as_ptr() as isize);
        }
        sync_remove_button(self.hwnd);
    }
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        unsafe {
            if self.is_open() {
                let _ = DestroyWindow(self.hwnd);
            }
            if let Some(font) = self.font {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;

    fn click(window: &SettingsWindow, id: i32) {
        send(item(window.hwnd, id), BM_CLICK, 0, 0);
    }

    fn select(window: &SettingsWindow, index: usize) {
        send(item(window.hwnd, ID_LIST), LB_SETCURSEL, index, 0);
        let notify = (LBN_SELCHANGE as usize) << 16 | ID_LIST as usize;
        send(window.hwnd, WM_COMMAND, notify, 0);
    }

    fn list_items(window: &SettingsWindow) -> usize {
        send(item(window.hwnd, ID_LIST), LB_GETCOUNT, 0, 0) as usize
    }

    fn remove_enabled(window: &SettingsWindow) -> bool {
        unsafe { IsWindowEnabled(item(window.hwnd, ID_REMOVE)).as_bool() }
    }

    #[test]
    fn scaling_rounds_to_the_nearest_pixel() {
        assert_eq!(scaled(100, 96), 100);
        assert_eq!(scaled(100, 144), 150);
        assert_eq!(scaled(100, 192), 200);
        assert_eq!(scaled(15, 120), 19);
    }

    #[test]
    fn controls_reflect_the_config_and_emit_matching_settings() {
        let config = Config {
            blocked_apps: vec!["Slack".into(), "\u{540d}\u{524d}".into()],
            ..Config::default()
        };
        let window = SettingsWindow::open(&config, true).unwrap();
        assert!(is_checked(window.hwnd, ID_ENABLED));
        assert!(is_checked(window.hwnd, ID_START));
        assert!(is_checked(window.hwnd, ID_NORMAL));
        assert!(!is_checked(window.hwnd, ID_FEWER));
        assert_eq!(list_items(&window), 2);
        assert!(!remove_enabled(&window));
        take_events();

        click(&window, ID_ENABLED);
        click(&window, ID_MORE);
        click(&window, ID_START);
        click(&window, ID_OPEN_DATA);
        click(&window, ID_DELETE_DATA);
        assert_eq!(
            take_events(),
            [
                UiEvent::Apply(Setting::Enabled(false)),
                UiEvent::Apply(Setting::Suggestions(Suggestions::More)),
                UiEvent::StartWithWindows(false),
                UiEvent::OpenDataFolder,
                UiEvent::DeleteData,
            ]
        );
        assert!(is_checked(window.hwnd, ID_MORE));
        assert!(!is_checked(window.hwnd, ID_NORMAL));
    }

    #[test]
    fn remove_needs_a_selection_and_unblocks_that_exact_name() {
        let config = Config {
            blocked_apps: vec!["Slack".into(), "\u{540d}\u{524d}".into()],
            ..Config::default()
        };
        let window = SettingsWindow::open(&config, false).unwrap();
        take_events();
        click(&window, ID_REMOVE);
        assert_eq!(take_events(), []);
        select(&window, 1);
        assert!(remove_enabled(&window));
        click(&window, ID_REMOVE);
        assert_eq!(
            take_events(),
            [UiEvent::Apply(Setting::Unblock("\u{540d}\u{524d}".into()))]
        );
    }

    #[test]
    fn refresh_follows_the_config_and_clears_the_selection() {
        let mut config = Config {
            blocked_apps: vec!["Slack".into()],
            ..Config::default()
        };
        let window = SettingsWindow::open(&config, false).unwrap();
        select(&window, 0);
        assert!(remove_enabled(&window));
        config.blocked_apps.clear();
        config.enabled = false;
        config.min_mean_logprob = -1.234;
        window.refresh(&config, true);
        assert_eq!(list_items(&window), 0);
        assert!(!remove_enabled(&window));
        assert!(!is_checked(window.hwnd, ID_ENABLED));
        assert!(is_checked(window.hwnd, ID_START));
        for (id, ..) in LEVELS {
            assert!(!is_checked(window.hwnd, id));
        }
    }

    #[test]
    fn closing_reports_once_and_drop_is_safe_after_close() {
        let window = SettingsWindow::open(&Config::default(), false).unwrap();
        take_events();
        send(window.hwnd, WM_CLOSE, 0, 0);
        assert!(!window.is_open());
        assert_eq!(take_events(), [UiEvent::Closed]);
        drop(window);
        assert_eq!(take_events(), []);
    }
}
