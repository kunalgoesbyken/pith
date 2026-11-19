mod accessible;
mod action;
mod controls;
mod layout;
mod model;
mod page;
mod pages;
mod picker;
mod recorder;
mod slider;
mod view;
mod window;

use std::cell::RefCell;

use pith_core::{Config, Counts, HISTORY_DAYS, ModelPreset, Pause, Setting};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, IsIconic, IsWindow, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
};

use crate::rival::Rival;
use crate::tray::{ForegroundApp, TrayCommand};

pub use page::Page;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatsView {
    pub today: Counts,
    pub total: Counts,
    pub days: [u64; HISTORY_DAYS],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overview {
    pub status: String,
    pub stats: StatsView,
    pub pause: Option<Pause>,
    pub app: Option<ForegroundApp>,
    pub start_with_windows: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    Apply(Setting),
    Command(TrayCommand),
    OpenDataFolder,
    OpenConfigFile,
    DeleteData,
    StartWithWindows(bool),
    ResetStats,
    OpenTypingSettings,
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
        recommended: ModelPreset,
        overview: &Overview,
    ) -> windows::core::Result<Self> {
        let hwnd = window::create(config, recommended, overview)?;
        Ok(Self { hwnd })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn is_open(&self) -> bool {
        unsafe { IsWindow(Some(self.hwnd)).as_bool() }
    }

    pub fn show(&self) {
        let command = if unsafe { IsIconic(self.hwnd) }.as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        unsafe {
            let _ = ShowWindow(self.hwnd, command);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    pub fn show_page(&self, page: Page) {
        if model::update(|model| model.go(page)).is_some() {
            window::relayout(self.hwnd);
        }
    }

    pub fn set_rival(&self, rival: Option<Rival>) {
        if model::update(|model| model.rival = rival).is_some() {
            window::relayout(self.hwnd);
        }
    }

    pub fn refresh(&self, config: &Config, overview: &Overview) {
        let changed = model::update(|model| model.refresh(config, overview));
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
    use super::model::read;
    use super::recorder::WM_RECORDED;
    use super::*;
    use crate::ui::{self, Theme, force_dark_theme};
    use pith_core::{Chord, KeyAction, Length, PauseSpan, Suggestions};
    use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use windows::Win32::UI::Controls::BST_CHECKED;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_F9;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::w;

    fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
        unsafe { SendMessageW(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))).0 }
    }

    fn open(config: &Config) -> SettingsWindow {
        let overview = Overview {
            status: "Ready".into(),
            ..Overview::default()
        };
        SettingsWindow::open(config, ModelPreset::Standard, &overview).unwrap()
    }

    fn id_of(name: &str) -> i32 {
        read(|model| {
            let found = model
                .layout
                .controls
                .iter()
                .find(|placed| placed.item.name == name);
            found.map(|placed| placed.id)
        })
        .flatten()
        .unwrap_or_else(|| panic!("no control named {name}"))
    }

    fn control(window: &SettingsWindow, name: &str) -> HWND {
        unsafe { GetDlgItem(Some(window.hwnd), id_of(name)) }.unwrap_or_default()
    }

    fn click(window: &SettingsWindow, name: &str) {
        send(control(window, name), BM_CLICK, 0, 0);
    }

    fn checked(window: &SettingsWindow, name: &str) -> bool {
        send(control(window, name), BM_GETCHECK, 0, 0) == BST_CHECKED.0 as isize
    }

    fn with_apps(apps: &[&str]) -> Config {
        Config {
            blocked_apps: apps.iter().map(|app| app.to_string()).collect(),
            ..Config::default()
        }
    }

    #[test]
    fn home_controls_reflect_the_config_and_emit_matching_events() {
        let window = open(&Config::default());
        assert!(checked(&window, "Suggest as I type"));
        take_events();
        click(&window, "Suggest as I type");
        click(&window, "Pause for 15 min");
        assert!(!checked(&window, "Suggest as I type"));
        click(&window, "Change shortcuts");
        assert_eq!(
            take_events(),
            [
                UiEvent::Apply(Setting::Enabled(false)),
                UiEvent::Command(TrayCommand::Pause(PauseSpan::Minutes(15))),
            ]
        );
        assert_eq!(read(|model| model.page), Some(Page::Shortcuts));
    }

    #[test]
    fn choices_select_one_segment_and_apply_their_setting() {
        let window = open(&Config::default());
        window.show_page(Page::General);
        take_events();
        click(&window, "Fewer");
        click(&window, "Short");
        click(&window, "Only when I ask");
        assert_eq!(
            take_events(),
            [
                UiEvent::Apply(Setting::Suggestions(Suggestions::Fewer)),
                UiEvent::Apply(Setting::Length(Length::Short)),
                UiEvent::Apply(Setting::TriggerMode(pith_core::TriggerMode::Manual)),
            ]
        );
        assert!(checked(&window, "Fewer"));
        assert!(checked(&window, "Short"));
        assert!(checked(&window, "Only when I ask"));
        assert!(!checked(&window, "As I type"));
    }

    #[test]
    fn a_recorded_key_rebinds_and_a_taken_key_explains_itself() {
        let window = open(&Config::default());
        window.show_page(Page::Shortcuts);
        take_events();
        click(&window, "Keep the suggestion: Tab");
        assert_eq!(
            read(|model| model.recording),
            Some(Some((KeyAction::AcceptAll, 0)))
        );
        send(window.hwnd, WM_RECORDED, 0, VK_F9.0 as isize);
        assert_eq!(
            take_events(),
            [UiEvent::Apply(Setting::Bind(
                KeyAction::AcceptAll,
                0,
                Some(Chord::parse("F9").unwrap())
            ))]
        );
        click(&window, "Keep one word: Shift+Tab");
        send(window.hwnd, WM_RECORDED, 0, VK_F9.0 as isize);
        assert_eq!(take_events(), []);
        let notice = read(|model| model.key_notice.clone()).flatten();
        assert!(notice.is_some_and(|text| text.ends_with('.')));
    }

    #[test]
    fn an_app_rule_is_removed_by_its_own_button() {
        let window = open(&with_apps(&["Slack", "\u{540d}\u{524d}"]));
        window.show_page(Page::Apps);
        take_events();
        click(&window, "Turn Pith on in \u{540d}\u{524d}");
        assert_eq!(
            take_events(),
            [UiEvent::Apply(Setting::Unblock("\u{540d}\u{524d}".into()))]
        );
        assert!(read(|model| model.config.blocked_apps == ["Slack"]).unwrap());
    }

    #[test]
    fn refresh_follows_the_config_and_the_overview() {
        let window = open(&Config::default());
        take_events();
        let mut overview = Overview::default();
        overview.stats.today.shown = 3;
        let config = Config {
            enabled: false,
            ..Config::default()
        };
        window.refresh(&config, &overview);
        assert!(!checked(&window, "Suggest as I type"));
        assert_eq!(read(|model| model.overview.stats.today.shown), Some(3));
        assert_eq!(take_events(), []);
    }

    #[test]
    fn closing_reports_once_and_drop_is_safe_after_close() {
        for (message, wparam) in [(WM_CLOSE, 0), (WM_COMMAND, IDCANCEL.0 as usize)] {
            let window = open(&Config::default());
            take_events();
            send(window.hwnd, message, wparam, 0);
            assert!(!window.is_open(), "{message}");
            assert_eq!(take_events(), [UiEvent::Closed], "{message}");
            drop(window);
            assert_eq!(take_events(), [], "{message}");
        }
    }

    #[test]
    fn a_dpi_change_rescales_the_window() {
        let window = open(&Config::default());
        let mut before = RECT::default();
        unsafe {
            let _ = GetWindowRect(window.hwnd, &mut before);
        }
        let suggested = RECT {
            right: ui::width(before) * 2,
            bottom: ui::height(before) * 2,
            ..before
        };
        send(
            window.hwnd,
            WM_DPICHANGED,
            192 | (192 << 16),
            &suggested as *const RECT as isize,
        );
        assert_eq!(read(|model| model.scale.0), Some(192));
        let mut after = RECT::default();
        unsafe {
            let _ = GetWindowRect(window.hwnd, &mut after);
        }
        assert!(ui::width(after) > ui::width(before));
    }

    #[test]
    fn an_immersive_color_change_rethemes_the_window() {
        let window = open(&Config::default());
        for dark in [true, false] {
            force_dark_theme(Some(dark));
            send(
                window.hwnd,
                WM_SETTINGCHANGE,
                0,
                w!("ImmersiveColorSet").as_ptr() as isize,
            );
            assert_eq!(read(|model| model.theme), Some(Theme::detect()));
        }
        force_dark_theme(None);
    }

    #[test]
    fn a_rival_row_appears_only_when_set() {
        let window = open(&Config::default());
        let rows = || read(|model| model.layout.controls.len()).unwrap();
        let plain = rows();
        window.set_rival(Some(Rival::TextSuggestions));
        assert_eq!(rows(), plain + 1);
        click(&window, "Review");
        assert_eq!(take_events(), [UiEvent::OpenTypingSettings]);
        window.set_rival(Some(Rival::App("Lightkey")));
        assert_eq!(rows(), plain);
        window.set_rival(None);
        assert_eq!(rows(), plain);
    }

    #[test]
    fn every_control_fits_inside_the_pane_without_overlapping() {
        let config = with_apps(&["a", "b", "c", "d"]);
        for page in Page::ALL {
            let window = open(&config);
            window.show_page(page);
            for dpi in [96, 144, 192] {
                send(
                    window.hwnd,
                    WM_DPICHANGED,
                    dpi | (dpi << 16),
                    &RECT {
                        right: 1600,
                        bottom: 1200,
                        ..Default::default()
                    } as *const RECT as isize,
                );
                let (controls, client) =
                    read(|model| (model.layout.controls.clone(), model.viewport)).unwrap();
                for (index, placed) in controls.iter().enumerate() {
                    let bounds = placed.bounds;
                    assert!(
                        bounds.left >= 0 && bounds.right <= client.0,
                        "{page:?} {dpi}"
                    );
                    for other in &controls[index + 1..] {
                        let apart = bounds.right <= other.bounds.left
                            || other.bounds.right <= bounds.left
                            || bounds.bottom <= other.bounds.top
                            || other.bounds.bottom <= bounds.top;
                        assert!(
                            apart,
                            "{} and {} overlap on {page:?} at {dpi}",
                            placed.id, other.id
                        );
                    }
                }
            }
        }
    }
}
