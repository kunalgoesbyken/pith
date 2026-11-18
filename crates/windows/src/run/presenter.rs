use std::time::Duration;

use pith_core::{Config, Counts, Day, Ledger, Model, SkipReason, Status, log};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use super::preferences::Preferences;
use super::{App, Options};
use crate::foreground::Foreground;
use crate::onboarding::{Event as OnboardingEvent, Onboarding, Outcome};
use crate::registry;
use crate::rival;
use crate::settings::{SettingsWindow, StatsView};
use crate::tray::{ForegroundApp, Tray, TrayCommand, TrayView};
use crate::update::Update;
use crate::util::local_time;

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
}

impl Presenter {
    pub fn new(config: &Config, options: &Options) -> Self {
        let tray = if options.tray {
            Tray::create(config.enabled, &config.reach())
        } else {
            None
        };
        let onboarding = options.onboarding.clone().and_then(|marker| {
            Onboarding::create(marker, true, &config.reach())
                .map_err(|error| log(&format!("welcome window failed: {error}")))
                .ok()
        });
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
        }
    }

    pub fn set_model(&mut self, model: Model) {
        self.model = model;
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

    pub fn finish_onboarding(&mut self) {
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
    }

    pub fn handles_dialog(&self, message: &MSG) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|window| unsafe { IsDialogMessageW(window.hwnd(), message) }.as_bool())
    }

    pub fn next_tray_command(&self) -> Option<TrayCommand> {
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
        self.refresh_settings(prefs);
        self.refresh_tray(prefs);
    }

    pub fn refresh_settings(&self, prefs: &Preferences) {
        if let Some(window) = self.settings.as_ref().filter(|window| window.is_open()) {
            window.refresh(
                prefs.saved(),
                registry::start_with_windows(),
                &self.status_line(prefs),
                &self.stats_view(),
            );
        }
    }

    fn refresh_tray(&self, prefs: &Preferences) {
        let Some(tray) = &self.tray else { return };
        tray.show(TrayView {
            status: self.status_line(prefs),
            active: prefs.enabled(),
            pause: prefs.shown_pause(),
            foreground: self.foreground.app().map(|name| ForegroundApp {
                name: name.to_string(),
                access: prefs.saved().access(name),
            }),
            update: self.update.clone(),
        });
    }

    fn status_line(&self, prefs: &Preferences) -> String {
        Status {
            model: self.model.clone(),
            pause: prefs.shown_pause(),
            enabled: prefs.saved().enabled,
            skip: self.skip,
            notice: self.notice.clone(),
        }
        .line()
    }

    fn stats_view(&self) -> StatsView {
        let stats = self.ledger.stats();
        StatsView {
            today: stats.today(today()),
            total: stats.total(),
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
            registry::start_with_windows(),
            &self.status_line(prefs),
            &self.stats_view(),
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
