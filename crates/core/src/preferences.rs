use std::path::{Path, PathBuf};

use crate::{
    Config, ForegroundApp, Model, ModelPreset, Pause, Pauses, Setting, SkipReason, Status, log,
};

pub struct Preferences {
    saved: Config,
    pub pauses: Pauses,
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
        Self {
            saved,
            pauses: Pauses::default(),
            config_path,
            recommended,
            spelling,
        }
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

    pub fn status_line(
        &self,
        model: &Model,
        skip: Option<SkipReason>,
        notice: Option<&str>,
    ) -> String {
        Status {
            model: model.clone(),
            pause: self.shown_pause(),
            enabled: self.saved.enabled,
            skip,
            notice: notice.map(str::to_string),
        }
        .line()
    }

    pub fn foreground_app(&self, name: &str) -> ForegroundApp {
        ForegroundApp {
            name: name.to_string(),
            access: self.saved.access(name),
            switch: self.saved.switch_app(name),
        }
    }

    pub fn apply(&mut self, setting: &Setting) {
        setting.apply(&mut self.saved);
        let Some(path) = &self.config_path else {
            return;
        };
        if let Err(error) = setting.persist(path) {
            log(&format!("settings not saved: {error}"));
        }
    }

    pub fn release_model(&mut self) -> bool {
        self.pauses.release_model(&self.saved)
    }
}
