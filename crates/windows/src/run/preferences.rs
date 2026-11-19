use std::path::{Path, PathBuf};
use std::time::SystemTime;

use pith_core::{Config, Event, Model, ModelPreset, Pause, PauseSpan, Pauses, Setting, log};

use super::App;
use crate::power::Power;
use crate::util::local_time;

pub struct Preferences {
    saved: Config,
    pauses: Pauses,
    config_path: Option<PathBuf>,
    recommended: ModelPreset,
    spelling: bool,
}

impl Preferences {
    pub fn new(
        saved: Config,
        config_path: Option<PathBuf>,
        recommended: ModelPreset,
        spelling: bool,
    ) -> Self {
        let mut preferences = Self {
            saved,
            pauses: Pauses::default(),
            config_path,
            recommended,
            spelling,
        };
        preferences.read_power();
        preferences
    }

    pub fn saved(&self) -> &Config {
        &self.saved
    }

    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    pub fn recommended(&self) -> ModelPreset {
        self.recommended
    }

    pub fn paused(&self) -> bool {
        self.pauses.active(&self.saved)
    }

    pub fn enabled(&self) -> bool {
        self.saved.enabled && !self.paused()
    }

    pub fn shown_pause(&self) -> Option<Pause> {
        self.pauses.shown(&self.saved)
    }

    pub fn effective(&self) -> Config {
        Config {
            enabled: self.enabled(),
            typo_fix: self.saved.typo_fix && self.spelling,
            ..self.saved.clone()
        }
    }

    fn apply(&mut self, setting: &Setting) {
        setting.apply(&mut self.saved);
        let Some(path) = &self.config_path else {
            return;
        };
        if let Err(error) = setting.persist(path) {
            log(&format!("settings not saved: {error}"));
        }
    }

    fn start_pause(&mut self, span: PauseSpan) {
        let now = local_time();
        let clock = (u32::from(now.wHour), u32::from(now.wMinute));
        self.pauses.start(span, clock, SystemTime::now());
    }

    fn read_power(&mut self) -> bool {
        self.pauses
            .set_battery_saver(Power::read() == Power::BatterySaver)
    }
}

impl App {
    pub(super) fn sync(&mut self) {
        let config = self.prefs.effective();
        self.keyboard.rebind(&config.keymap());
        self.reader.set_config(&config);
        self.overlay.set_opacity(config.ghost_opacity);
        let enabled = config.enabled;
        self.dispatch(Event::SetConfig(config));
        self.keyboard.set_trigger(enabled);
        if self.prefs.pauses.release_model(&self.prefs.saved) {
            self.worker.unload();
        }
        self.refresh();
    }

    pub(super) fn change(&mut self, setting: Setting) {
        let before = self.prefs.saved().clone();
        self.prefs.apply(&setting);
        self.forget_blocked_style();
        if let Setting::GhostOpacity(_) = setting {
            self.overlay.set_opacity(self.prefs.saved().ghost_opacity);
            self.presenter.refresh_settings(&self.prefs);
            return;
        }
        self.sync();
        if !before.same_backend(self.prefs.saved(), self.prefs.recommended()) {
            self.reload_model();
        }
    }

    fn reload_model(&mut self) {
        self.presenter.set_model(Model::Loading);
        self.worker.reload(self.prefs.saved().clone());
        self.refresh();
    }

    pub(super) fn pause(&mut self, span: PauseSpan) {
        self.prefs.start_pause(span);
        self.sync();
    }

    pub(super) fn resume(&mut self) {
        if self.prefs.pauses.resume() {
            self.sync();
        }
    }

    pub(super) fn expire_pause(&mut self) {
        if self.prefs.pauses.expire(SystemTime::now()) {
            self.sync();
        }
    }

    pub(super) fn power_changed(&mut self) {
        if self.prefs.read_power() {
            self.sync();
        }
    }
}
