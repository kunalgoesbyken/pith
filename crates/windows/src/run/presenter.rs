use std::time::Duration;

use pith_core::{
    Config, Counts, Day, ForegroundApp, Ledger, Model, Preferences, SkipReason, TrayView, log,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use super::{App, Options, Start};
use crate::foreground::Foreground;
use crate::onboarding::{Event as OnboardingEvent, Onboarding, Outcome};
use crate::settings::{Overview, SettingsWindow, StatsView, StyleLine};
use crate::system::appearance::Appearance;
use crate::system::registry;
use crate::system::rival;
use crate::system::update::Update;
use crate::system::util::local_time;
use crate::tray::{Tray, TrayEvent, TrayFace};

const STATS_FILE: &str = "stats.toml";
const STATS_FLUSH: Duration = Duration::from_secs(60);

fn today() -> Day {
    let now = local_time();
    Day::from_ymd(now.wYear, now.wMonth as u8, now.wDay as u8)
}

fn log_unsaved_stats(result: Result<(), String>) {
    if let Err(error) = result {
        log(&format!("stats not saved: {error}"));
    }
}

pub struct Presenter {
    tray: Option<Tray>,
    settings: Option<SettingsWindow>,
    onboarding: Option<Onboarding>,
    foreground: Foreground,
    ledger: Ledger,
    model: Model,
    skip: Option<SkipReason>,
    notice: Option<String>,
    update: Update,
    appearance: Appearance,
    style: Vec<StyleLine>,
}

impl Presenter {
    pub fn new(config: &Config, options: &Options, appearance: Appearance) -> Self {
        let tray = if options.tray {
            Tray::create(config.enabled, &config.reach(), &appearance)
        } else {
            None
        };
        let onboarding = match &options.start {
            Start::Onboarding(marker) => Onboarding::create(marker.clone(), true, &config.reach())
                .map_err(|error| log(&format!("welcome window failed: {error}")))
                .ok(),
            Start::Tray | Start::Home => None,
        };
        let mut ledger = Ledger::new(options.data_dir.as_ref().map(|dir| dir.join(STATS_FILE)));
        if let Err(error) = ledger.load() {
            log(&format!(
                "stats not loaded, not saving them this session: {error}"
            ));
        }
        Self {
            tray,
            settings: None,
            onboarding,
            foreground: Foreground::default(),
            ledger,
            model: Model::Loading,
            skip: None,
            notice: options.notice.clone(),
            update: Update::default(),
            appearance,
            style: Vec::new(),
        }
    }

    pub fn set_model(&mut self, model: Model) {
        self.model = model;
    }

    pub fn set_style(&mut self, style: Vec<StyleLine>) {
        self.style = style;
    }

    pub fn recorded(&self) -> Counts {
        self.ledger.stats().total()
    }

    pub fn set_appearance(&mut self, appearance: Appearance, prefs: &Preferences) {
        self.appearance = appearance;
        self.show_tray(prefs, self.foreground_view(prefs));
    }

    pub fn update(&self) -> &Update {
        &self.update
    }

    pub fn set_update(&mut self, update: Update) {
        self.update = update;
    }

    pub fn set_skip(&mut self, skip: Option<SkipReason>) {
        self.skip = skip;
    }

    pub fn set_auto_paused(&self, apps: &[String]) {
        if let Some(tray) = &self.tray {
            tray.set_auto_paused(apps);
        }
    }

    pub fn onboard(&mut self, event: OnboardingEvent) {
        if let Some(onboarding) = &mut self.onboarding {
            onboarding.event(event);
        }
    }

    pub fn finish_onboarding(&mut self, prefs: &Preferences) {
        let Some(onboarding) = &mut self.onboarding else {
            return;
        };
        if onboarding.poll() == Outcome::Pending {
            return;
        }
        self.onboarding = None;
        if let Some(tray) = &self.tray {
            tray.balloon("Pith", "Pith is running in the tray");
        }
        self.open_settings(prefs);
    }

    pub fn handles_dialog(&self, message: &MSG) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|window| unsafe { IsDialogMessageW(window.hwnd(), message) }.as_bool())
    }

    pub fn next_tray_event(&self) -> Option<TrayEvent> {
        self.tray.as_ref().and_then(Tray::poll)
    }

    pub fn foreground_app(&self) -> Option<String> {
        self.foreground.app().map(str::to_string)
    }

    pub fn poll(&mut self, prefs: &Preferences) {
        if self.tray.is_some() {
            self.foreground.poll();
            self.refresh(prefs);
        }
    }

    pub fn refresh(&self, prefs: &Preferences) {
        let app = self.foreground_view(prefs);
        self.show_settings(prefs, app.clone());
        self.show_tray(prefs, app);
    }

    pub fn refresh_settings(&self, prefs: &Preferences) {
        self.show_settings(prefs, self.foreground_view(prefs));
    }

    fn show_settings(&self, prefs: &Preferences, app: Option<ForegroundApp>) {
        if let Some(window) = self.settings.as_ref().filter(|window| window.is_open()) {
            window.refresh(prefs.saved(), &self.overview(prefs, app));
        }
    }

    fn show_tray(&self, prefs: &Preferences, app: Option<ForegroundApp>) {
        let Some(tray) = &self.tray else { return };
        tray.show(TrayFace {
            view: TrayView {
                status: self.status_line(prefs),
                active: prefs.enabled(),
                pause: prefs.shown_pause(),
                foreground: app,
            },
            taskbar: self.appearance.taskbar,
            icon_px: self.appearance.icon_px,
            update: self.update.clone(),
        });
    }

    fn status_line(&self, prefs: &Preferences) -> String {
        prefs.status_line(&self.model, self.skip, self.notice.as_deref())
    }

    fn foreground_view(&self, prefs: &Preferences) -> Option<ForegroundApp> {
        self.foreground.app().map(|name| prefs.foreground_app(name))
    }

    fn stats_view(&self) -> StatsView {
        let stats = self.ledger.stats();
        let day = today();
        StatsView {
            today: stats.today(day),
            total: stats.total(),
            days: stats.recent_words(day),
        }
    }

    fn overview(&self, prefs: &Preferences, app: Option<ForegroundApp>) -> Overview {
        Overview {
            status: self.status_line(prefs),
            stats: self.stats_view(),
            pause: prefs.shown_pause(),
            app,
            start_with_windows: registry::start_with_windows(),
            style: self.style.clone(),
        }
    }

    pub fn record_stats(&mut self, counts: Counts, prefs: &Preferences) {
        if self.ledger.record(counts, today()) {
            self.refresh_settings(prefs);
        }
        log_unsaved_stats(self.ledger.flush_every(STATS_FLUSH));
    }

    pub fn reset_stats(&mut self, prefs: &Preferences) {
        log_unsaved_stats(self.ledger.reset());
        self.refresh_settings(prefs);
    }

    pub fn flush_stats(&mut self) {
        log_unsaved_stats(self.ledger.flush());
    }

    pub fn open_settings(&mut self, prefs: &Preferences) {
        if let Some(window) = self.settings.as_ref().filter(|window| window.is_open()) {
            window.show();
            window.set_rival(rival::detect());
            return;
        }
        match SettingsWindow::open(
            prefs.saved(),
            prefs.recommended(),
            &self.overview(prefs, self.foreground_view(prefs)),
        ) {
            Ok(window) => {
                window.show();
                window.set_rival(rival::detect());
                self.settings = Some(window);
            }
            Err(error) => log(&format!("settings window failed: {error}")),
        }
    }

    pub fn close_settings(&mut self) {
        self.settings = None;
    }

    pub fn confirm_delete(&self) -> bool {
        let owner = self.settings.as_ref().map(SettingsWindow::hwnd);
        let answer = unsafe {
            MessageBoxW(
                owner,
                w!(
                    "Delete the downloaded model and the local data of Pith from this PC, then close Pith? Your settings are kept. The model downloads again the next time Pith starts."
                ),
                w!("Delete model and data"),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        answer == IDYES
    }

    pub fn confirm_forget_style(&self) -> bool {
        let owner = self.settings.as_ref().map(SettingsWindow::hwnd);
        let answer = unsafe {
            MessageBoxW(
                owner,
                w!("Forget every sentence Pith learned from you? This cannot be undone."),
                w!("Forget my style"),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        answer == IDYES
    }
}

impl App {
    pub(super) fn refresh(&self) {
        self.presenter.refresh(&self.prefs);
    }

    pub(super) fn collect_stats(&mut self) {
        let counts = self.engine.take_counts();
        self.presenter.record_stats(counts, &self.prefs);
    }

    pub(super) fn reset_stats(&mut self) {
        self.engine.take_counts();
        self.presenter.reset_stats(&self.prefs);
    }
}
