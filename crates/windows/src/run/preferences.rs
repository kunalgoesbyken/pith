use std::path::PathBuf;
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
}

impl Preferences {
    pub fn new(saved: Config, config_path: Option<PathBuf>, recommended: ModelPreset) -> Self {
        let mut preferences = Self {
            saved,
            pauses: Pauses::default(),
            config_path,
            recommended,
        };
        preferences.read_power();
        preferences
    }

    pub fn saved(&self) -> &Config {
        &self.saved
    }

    pub fn recommended(&self) -> ModelPreset {
        self.recommended
    }

    pub fn preset(&self) -> ModelPreset {
        self.saved.preset(self.recommended)
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
        let preset = self.prefs.preset();
        self.prefs.apply(&setting);
        if let Setting::GhostOpacity(_) = setting {
            self.overlay.set_opacity(self.prefs.saved().ghost_opacity);
            self.presenter.refresh_settings(&self.prefs);
            return;
        }
        self.sync();
        if self.prefs.preset() != preset {
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
