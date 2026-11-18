mod layout;
mod model;
mod view;
mod window;

use std::cell::RefCell;

use pith_core::{Config, Setting, Suggestions};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, IsWindow, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow,
};

use model::update;

const ID_ENABLED: i32 = 101;
const ID_FEWER: i32 = 102;
const ID_NORMAL: i32 = 103;
const ID_MORE: i32 = 104;
const ID_OPEN_DATA: i32 = 112;
const ID_DELETE_DATA: i32 = 113;
const ID_START: i32 = 114;
const ID_REMOVE_FIRST: i32 = 120;
const MAX_APP_ROWS: usize = 3;
const SEGMENTS: [(i32, Suggestions, &str); 3] = [
    (ID_FEWER, Suggestions::Fewer, "Fewer"),
    (ID_NORMAL, Suggestions::Normal, "Normal"),
    (ID_MORE, Suggestions::More, "More"),
];

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
}

pub fn take_events() -> Vec<UiEvent> {
    EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

fn emit(event: UiEvent) {
    EVENTS.with(|events| events.borrow_mut().push(event));
}

pub struct SettingsWindow {
    hwnd: HWND,
}

impl SettingsWindow {
    pub fn open(
        config: &Config,
        start_with_windows: bool,
        status: &str,
    ) -> windows::core::Result<Self> {
        let hwnd = window::create(config, start_with_windows, status)?;
        Ok(Self { hwnd })
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

    pub fn refresh(&self, config: &Config, start_with_windows: bool, status: &str) {
        let changed = update(|model| {
            let before = model.clone();
            model.refresh(config, start_with_windows, status);
            *model != before
        });
        if changed == Some(true) {
            window::relayout(self.hwnd);
        }
    }
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        if self.is_open() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::layout::build;
    use super::model::current;
    use super::*;
    use crate::ui::{self, Scale, Theme, force_dark_theme};
    use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::w;

    fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
        unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))).0 }
    }

    fn item(window: &SettingsWindow, id: i32) -> HWND {
        unsafe { GetDlgItem(Some(window.hwnd), id) }.unwrap_or_default()
    }

    fn click(window: &SettingsWindow, id: i32) {
        send(item(window, id), BM_CLICK, 0, 0);
    }

    fn name(window: &SettingsWindow, id: i32) -> String {
        let mut buffer = [0u16; 128];
        let length = unsafe { GetWindowTextW(item(window, id), &mut buffer) };
        String::from_utf16_lossy(&buffer[..length as usize])
    }

    fn shown(window: &SettingsWindow, id: i32) -> bool {
        let style = unsafe { GetWindowLongPtrW(item(window, id), GWL_STYLE) } as u32;
        style & WS_VISIBLE.0 != 0
    }

    fn frame_width(window: &SettingsWindow) -> i32 {
        let mut frame = RECT::default();
        unsafe {
            let _ = GetWindowRect(window.hwnd, &mut frame);
        }
        ui::width(frame)
    }

    fn with_apps(apps: &[&str]) -> Config {
        Config {
            blocked_apps: apps.iter().map(|app| app.to_string()).collect(),
            ..Config::default()
        }
    }

    #[test]
    fn controls_reflect_the_config_and_emit_matching_settings() {
        let config = with_apps(&["Slack", "\u{540d}\u{524d}"]);
        let window = SettingsWindow::open(&config, true, "Ready").unwrap();
        assert_eq!(name(&window, ID_ENABLED), "Suggest as I type, on");
        assert_eq!(name(&window, ID_START), "Start with Windows, on");
        assert_eq!(name(&window, ID_NORMAL), "Normal, selected");
        assert_eq!(name(&window, ID_FEWER), "Fewer");
        assert_eq!(
            name(&window, ID_REMOVE_FIRST + 1),
            "Remove \u{540d}\u{524d}"
        );
        assert!(shown(&window, ID_REMOVE_FIRST + 1));
        assert!(!shown(&window, ID_REMOVE_FIRST + 2));
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
        assert_eq!(name(&window, ID_ENABLED), "Suggest as I type, off");
        assert_eq!(name(&window, ID_MORE), "More, selected");
        assert_eq!(name(&window, ID_NORMAL), "Normal");
    }

    #[test]
    fn remove_unblocks_that_exact_name_and_unused_slots_do_nothing() {
        let config = with_apps(&["Slack", "\u{540d}\u{524d}"]);
        let window = SettingsWindow::open(&config, false, "Ready").unwrap();
        take_events();
        click(&window, ID_REMOVE_FIRST + 2);
        assert_eq!(take_events(), []);
        click(&window, ID_REMOVE_FIRST + 1);
        assert_eq!(
            take_events(),
            [UiEvent::Apply(Setting::Unblock("\u{540d}\u{524d}".into()))]
        );
    }

    #[test]
    fn refresh_follows_the_config() {
        let mut config = with_apps(&["Slack"]);
        let window = SettingsWindow::open(&config, false, "Ready").unwrap();
        assert!(shown(&window, ID_REMOVE_FIRST));
        config.blocked_apps.clear();
        config.enabled = false;
        config.min_mean_logprob = -1.234;
        window.refresh(&config, true, "Turned off");
        assert!(!shown(&window, ID_REMOVE_FIRST));
        assert_eq!(name(&window, ID_ENABLED), "Suggest as I type, off");
        assert_eq!(name(&window, ID_START), "Start with Windows, on");
        for (id, level, label) in SEGMENTS {
            assert_eq!(name(&window, id), label, "{level:?}");
        }
    }

    #[test]
    fn a_long_list_scrolls_in_whole_rows() {
        let config = with_apps(&["a", "b", "c", "d", "e"]);
        let window = SettingsWindow::open(&config, false, "Ready").unwrap();
        let wheel = |delta: i16| (delta as u16 as usize) << 16;
        send(window.hwnd, WM_MOUSEWHEEL, wheel(-120), 0);
        assert_eq!(name(&window, ID_REMOVE_FIRST), "Remove b");
        send(window.hwnd, WM_MOUSEWHEEL, wheel(-360), 0);
        assert_eq!(name(&window, ID_REMOVE_FIRST), "Remove c");
        send(window.hwnd, WM_MOUSEWHEEL, wheel(360), 0);
        assert_eq!(name(&window, ID_REMOVE_FIRST), "Remove a");
    }

    #[test]
    fn closing_reports_once_and_drop_is_safe_after_close() {
        let window = SettingsWindow::open(&Config::default(), false, "Ready").unwrap();
        take_events();
        send(window.hwnd, WM_CLOSE, 0, 0);
        assert!(!window.is_open());
        assert_eq!(take_events(), [UiEvent::Closed]);
        drop(window);
        assert_eq!(take_events(), []);
    }

    #[test]
    fn escape_closes_through_the_dialog_cancel_command() {
        let window = SettingsWindow::open(&Config::default(), false, "Ready").unwrap();
        take_events();
        send(window.hwnd, WM_COMMAND, IDCANCEL.0 as usize, 0);
        assert!(!window.is_open());
        assert_eq!(take_events(), [UiEvent::Closed]);
    }

    #[test]
    fn a_dpi_change_rescales_the_window() {
        let window = SettingsWindow::open(&with_apps(&["Slack"]), false, "Ready").unwrap();
        let before = frame_width(&window);
        let suggested = RECT::default();
        send(
            window.hwnd,
            WM_DPICHANGED,
            192 | (192 << 16),
            &suggested as *const RECT as isize,
        );
        assert!(frame_width(&window) > before * 3 / 2);
    }

    #[test]
    fn an_immersive_color_change_retheme_the_window() {
        let window = SettingsWindow::open(&Config::default(), false, "Ready").unwrap();
        for dark in [true, false] {
            force_dark_theme(Some(dark));
            send(
                window.hwnd,
                WM_SETTINGCHANGE,
                0,
                w!("ImmersiveColorSet").as_ptr() as isize,
            );
            let theme = current().map(|model| model.theme).unwrap();
            assert_eq!(theme, Theme::detect());
        }
        force_dark_theme(None);
    }

    #[test]
    fn every_control_fits_inside_the_window_without_overlapping() {
        let config = with_apps(&["a", "b", "c", "d"]);
        for dpi in [96, 120, 144, 192] {
            let model = model::Model::new(
                &config,
                false,
                "Ready",
                (Theme::detect(), false, Scale(dpi)),
            );
            let layout = build(&model);
            let visible: Vec<_> = layout
                .controls
                .iter()
                .filter(|placed| placed.visible)
                .collect();
            assert_eq!(visible.len(), 2 + SEGMENTS.len() + MAX_APP_ROWS + 2);
            for (index, placed) in visible.iter().enumerate() {
                let bounds = placed.bounds;
                assert!(
                    bounds.left >= 0 && bounds.right <= layout.client.right,
                    "{dpi}"
                );
                assert!(
                    bounds.top >= 0 && bounds.bottom <= layout.client.bottom,
                    "{dpi}"
                );
                for other in &visible[index + 1..] {
                    let apart = bounds.right <= other.bounds.left
                        || other.bounds.right <= bounds.left
                        || bounds.bottom <= other.bounds.top
                        || other.bounds.bottom <= bounds.top;
                    assert!(apart, "{} and {} overlap at {dpi}", placed.id, other.id);
                }
            }
        }
    }
}
