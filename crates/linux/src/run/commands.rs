use std::path::Path;
use std::time::{Duration, SystemTime};

use pith_core::{Event, PauseSpan, TrayCommand, log};

use super::inbox::Flow;
use super::{App, Start};
use crate::clock::local_clock;
use crate::notice::notify;
use crate::open::open_config;

const PAUSE_CHECK: Duration = Duration::from_secs(5);
const WELCOME: &str = "Pith is running. Its icon is in the system tray.";
const WELCOME_WITHOUT_TRAY: &str = "Pith is running without a tray icon because this desktop shows none. Run pith --settings to open its settings file.";

impl App {
    pub(super) fn begin(&mut self, start: Start) {
        match start {
            Start::Tray => {}
            Start::Home => self.open_settings(),
            Start::Welcome(marker) => self.welcome(&marker),
        }
    }

    pub(super) fn command(&mut self, command: TrayCommand) -> Flow {
        match command {
            TrayCommand::Pause(span) => self.pause(span),
            TrayCommand::Resume => self.resume(),
            TrayCommand::ResumeApp(app) => self.dispatch(Event::Resume(app)),
            TrayCommand::ChangeApp => self.change_foreground_app(),
            TrayCommand::Open => self.open_settings(),
            TrayCommand::Quit => return Flow::Stop,
        }
        Flow::Continue
    }

    pub(super) fn expire_pause(&mut self) {
        if self.prefs.pauses.expire(SystemTime::now()) {
            self.sync();
        } else {
            self.watch_pause();
        }
    }

    fn welcome(&mut self, marker: &Path) {
        self.shell.welcome_due = true;
        if self.shell.has_tray() {
            self.tray_settled(true);
        }
        if let Err(error) = std::fs::write(marker, "") {
            log(&format!("welcome marker not written: {error}"));
        }
    }

    pub(super) fn tray_settled(&mut self, present: bool) {
        if std::mem::take(&mut self.shell.welcome_due) {
            let text = if present {
                WELCOME
            } else {
                WELCOME_WITHOUT_TRAY
            };
            notify(text.to_string());
        }
    }

    fn open_settings(&self) {
        log("settings requested");
        if let Some(path) = self.prefs.config_path() {
            open_config(path, self.prefs.saved());
        }
        if !self.shell.has_tray() {
            notify(self.shell.status_line(&self.prefs));
        }
    }

    fn pause(&mut self, span: PauseSpan) {
        let Some(clock) = local_clock() else {
            log("pause not started: the local time is unavailable");
            return;
        };
        self.prefs.pauses.start(span, clock, SystemTime::now());
        self.sync();
    }

    fn resume(&mut self) {
        if self.prefs.pauses.resume() {
            self.sync();
        }
    }

    fn change_foreground_app(&mut self) {
        let Some(setting) = self
            .shell
            .app()
            .and_then(|app| self.prefs.saved().switch_app(app))
        else {
            return;
        };
        self.prefs.apply(&setting);
        self.forget_blocked_style();
        self.sync();
    }

    fn sync(&mut self) {
        let config = self.prefs.effective();
        self.reader.set_config(&config);
        let enabled = config.enabled;
        self.dispatch(Event::SetConfig(config));
        if !enabled {
            self.keyboard.keys.set_trigger(false);
        }
        self.watch_pause();
    }

    fn watch_pause(&mut self) {
        if self.prefs.pauses.runs_out() {
            self.timers.start_pause_check(PAUSE_CHECK);
        }
    }
}
