mod layout;
mod model;
mod view;
mod window;

use std::cell::RefCell;

use pith_core::{Config, Counts, Length, ModelPreset, Setting, Suggestions};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, IsWindow, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow,
};

use crate::rival::Rival;
use model::update;

const ID_ENABLED: i32 = 101;
const ID_FEWER: i32 = 102;
const ID_NORMAL: i32 = 103;
const ID_MORE: i32 = 104;
const ID_SHORT: i32 = 105;
const ID_NORMAL_LENGTH: i32 = 106;
const ID_LONG: i32 = 107;
const ID_OPACITY_DOWN: i32 = 108;
const ID_OPACITY_UP: i32 = 109;
const ID_OPEN_DATA: i32 = 112;
const ID_DELETE_DATA: i32 = 113;
const ID_START: i32 = 114;
const ID_RESET_STATS: i32 = 115;
const ID_RIVAL_OPEN: i32 = 116;
const ID_REMOVE_FIRST: i32 = 120;
const ID_SMALL: i32 = 130;
const ID_STANDARD: i32 = 131;
const ID_LARGE: i32 = 132;
const MAX_APP_ROWS: usize = 3;
const OPACITY_STEP: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Frequency(Suggestions),
    Length(Length),
    Model(ModelPreset),
}

impl Choice {
    fn setting(self) -> Setting {
        match self {
            Self::Frequency(level) => Setting::Suggestions(level),
            Self::Length(length) => Setting::Length(length),
            Self::Model(preset) => Setting::Model(preset),
        }
    }

    fn caption(self, label: &'static str, selected: bool) -> &'static str {
        match (self, selected) {
            (Self::Model(_), true) => "In use",
            (Self::Model(_), false) => "Use",
            _ => label,
        }
    }
}

type Segment = (i32, Choice, &'static str);

const FREQUENCY: [Segment; 3] = [
    (ID_FEWER, Choice::Frequency(Suggestions::Fewer), "Fewer"),
    (ID_NORMAL, Choice::Frequency(Suggestions::Normal), "Normal"),
    (ID_MORE, Choice::Frequency(Suggestions::More), "More"),
];
const LENGTH: [Segment; 3] = [
    (ID_SHORT, Choice::Length(Length::Short), "Short"),
    (ID_NORMAL_LENGTH, Choice::Length(Length::Normal), "Normal"),
    (ID_LONG, Choice::Length(Length::Long), "Long"),
];

const MODELS: [Segment; 3] = [
    (ID_SMALL, Choice::Model(ModelPreset::Small), "Small model"),
    (
        ID_STANDARD,
        Choice::Model(ModelPreset::Standard),
        "Standard model",
    ),
    (ID_LARGE, Choice::Model(ModelPreset::Large), "Large model"),
];

fn segments() -> impl Iterator<Item = &'static Segment> {
    FREQUENCY.iter().chain(&LENGTH).chain(&MODELS)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatsView {
    pub today: Counts,
    pub total: Counts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    Apply(Setting),
    OpenDataFolder,
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
        start_with_windows: bool,
        status: &str,
        stats: &StatsView,
    ) -> windows::core::Result<Self> {
        let hwnd = window::create(config, recommended, start_with_windows, status, stats)?;
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

    pub fn set_rival(&self, rival: Option<Rival>) {
        if update(|model| model.rival = rival).is_some() {
            window::relayout(self.hwnd);
        }
    }

    pub fn refresh(
        &self,
        config: &Config,
        start_with_windows: bool,
        status: &str,
        stats: &StatsView,
    ) {
        let changed = update(|model| {
            let before = model.clone();
            model.refresh(config, start_with_windows, status, stats);
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
        {
            let config = with_apps(&["Slack", "\u{540d}\u{524d}"]);
            let window = SettingsWindow::open(
                &config,
                ModelPreset::Standard,
                true,
                "Ready",
                &StatsView::default(),
            )
            .unwrap();
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
        {
            let config = Config {
                ghost_opacity: 90,
                ..Config::default()
            };
            let window = SettingsWindow::open(
                &config,
                ModelPreset::Standard,
                false,
                "Ready",
                &StatsView::default(),
            )
            .unwrap();
            assert_eq!(name(&window, ID_NORMAL_LENGTH), "Normal, selected");
            assert_eq!(name(&window, ID_NORMAL), "Normal, selected");
            take_events();

            click(&window, ID_LONG);
            click(&window, ID_SHORT);
            click(&window, ID_OPACITY_UP);
            click(&window, ID_OPACITY_UP);
            click(&window, ID_OPACITY_DOWN);
            assert_eq!(
                take_events(),
                [
                    UiEvent::Apply(Setting::Length(Length::Long)),
                    UiEvent::Apply(Setting::Length(Length::Short)),
                    UiEvent::Apply(Setting::GhostOpacity(100)),
                    UiEvent::Apply(Setting::GhostOpacity(90)),
                ]
            );
            assert_eq!(name(&window, ID_SHORT), "Short, selected");
            assert_eq!(name(&window, ID_NORMAL), "Normal, selected");
            assert_eq!(current().map(|model| model.opacity), Some(90));
        }
    }

    #[test]
    fn reset_stats_asks_the_host_and_refresh_follows_the_config_and_stats() {
        {
            let window = SettingsWindow::open(
                &Config::default(),
                ModelPreset::Standard,
                false,
                "Ready",
                &StatsView::default(),
            )
            .unwrap();
            assert_eq!(name(&window, ID_RESET_STATS), "Reset stats");
            take_events();
            click(&window, ID_RESET_STATS);
            assert_eq!(take_events(), [UiEvent::ResetStats]);
            let stats = StatsView {
                today: Counts {
                    shown: 3,
                    ..Counts::default()
                },
                total: Counts {
                    shown: 12,
                    ..Counts::default()
                },
            };
            window.refresh(&Config::default(), false, "Ready", &stats);
            let model = current().unwrap();
            assert_eq!((model.today.shown, model.total.shown), (3, 12));
        }
        {
            let mut config = with_apps(&["Slack"]);
            let window = SettingsWindow::open(
                &config,
                ModelPreset::Standard,
                false,
                "Ready",
                &StatsView::default(),
            )
            .unwrap();
            assert!(shown(&window, ID_REMOVE_FIRST));
            config.blocked_apps.clear();
            config.enabled = false;
            config.min_mean_logprob = -1.234;
            window.refresh(&config, true, "Turned off", &StatsView::default());
            assert!(!shown(&window, ID_REMOVE_FIRST));
            assert_eq!(name(&window, ID_ENABLED), "Suggest as I type, off");
            assert_eq!(name(&window, ID_START), "Start with Windows, on");
            for (id, choice, label) in segments() {
                let selected = matches!(
                    choice,
                    Choice::Length(Length::Normal) | Choice::Model(ModelPreset::Standard)
                );
                let expected = if selected {
                    format!("{label}, selected")
                } else {
                    label.to_string()
                };
                assert_eq!(name(&window, *id), expected, "{choice:?}");
            }
        }
    }

    #[test]
    fn remove_unblocks_that_exact_name_and_unused_slots_do_nothing() {
        let config = with_apps(&["Slack", "\u{540d}\u{524d}"]);
        let window = SettingsWindow::open(
            &config,
            ModelPreset::Standard,
            false,
            "Ready",
            &StatsView::default(),
        )
        .unwrap();
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
    fn a_long_list_scrolls_in_whole_rows() {
        let config = with_apps(&["a", "b", "c", "d", "e"]);
        let window = SettingsWindow::open(
            &config,
            ModelPreset::Standard,
            false,
            "Ready",
            &StatsView::default(),
        )
        .unwrap();
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
        for (message, wparam) in [(WM_CLOSE, 0), (WM_COMMAND, IDCANCEL.0 as usize)] {
            let window = SettingsWindow::open(
                &Config::default(),
                ModelPreset::Standard,
                false,
                "Ready",
                &StatsView::default(),
            )
            .unwrap();
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
        let window = SettingsWindow::open(
            &with_apps(&["Slack"]),
            ModelPreset::Standard,
            false,
            "Ready",
            &StatsView::default(),
        )
        .unwrap();
        let before = frame_width(&window);
        let suggested = RECT::default();
        send(
            window.hwnd,
            WM_DPICHANGED,
            192 | (192 << 16),
            &suggested as *const RECT as isize,
        );
        let largest_allowed = unsafe { GetSystemMetrics(SM_CXMAXTRACK) };
        assert!(frame_width(&window) > (before * 3 / 2).min(largest_allowed - 1));
    }

    #[test]
    fn an_immersive_color_change_retheme_the_window() {
        let window = SettingsWindow::open(
            &Config::default(),
            ModelPreset::Standard,
            false,
            "Ready",
            &StatsView::default(),
        )
        .unwrap();
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
    fn a_rival_line_appears_only_when_set_and_only_the_system_feature_has_an_action() {
        let window = SettingsWindow::open(
            &Config::default(),
            ModelPreset::Standard,
            false,
            "Ready",
            &StatsView::default(),
        )
        .unwrap();
        assert!(!shown(&window, ID_RIVAL_OPEN));
        let plain_height = current().map(|model| build(&model).client.bottom).unwrap();
        take_events();

        window.set_rival(Some(Rival::TextSuggestions));
        assert!(shown(&window, ID_RIVAL_OPEN));
        assert_eq!(name(&window, ID_RIVAL_OPEN), "Open Windows typing settings");
        click(&window, ID_RIVAL_OPEN);
        assert_eq!(take_events(), [UiEvent::OpenTypingSettings]);

        window.set_rival(Some(Rival::App("Lightkey")));
        assert!(!shown(&window, ID_RIVAL_OPEN));

        window.refresh(&Config::default(), false, "Ready", &StatsView::default());
        assert_eq!(
            current().and_then(|model| model.rival),
            Some(Rival::App("Lightkey"))
        );

        window.set_rival(None);
        assert!(!shown(&window, ID_RIVAL_OPEN));
        let after = current().map(|model| build(&model).client.bottom).unwrap();
        assert_eq!(after, plain_height);
    }

    #[test]
    fn every_control_fits_inside_the_window_without_overlapping() {
        let config = with_apps(&["a", "b", "c", "d"]);
        for dpi in [96, 120, 144, 192] {
            let model = model::Model::new(
                &config,
                ModelPreset::Standard,
                false,
                "Ready",
                &StatsView::default(),
                (Theme::detect(), false, Scale(dpi)),
            );
            let layout = build(&model);
            let visible: Vec<_> = layout
                .controls
                .iter()
                .filter(|placed| placed.visible)
                .collect();
            assert_eq!(
                visible.len(),
                2 + FREQUENCY.len() + LENGTH.len() + 2 + MODELS.len() + MAX_APP_ROWS + 2 + 1
            );
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
