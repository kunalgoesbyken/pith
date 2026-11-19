use pith_core::{Model, Preferences, SkipReason, TrayView};

use super::App;
use crate::tray::{Shown, Tray};

pub(super) struct Shell {
    tray: Option<Tray>,
    shown: Option<Shown>,
    model: Model,
    reported: Option<String>,
    skip: Option<SkipReason>,
    notice: Option<String>,
    app: Option<String>,
    auto_paused: Vec<String>,
}

impl Shell {
    pub(super) fn new(tray: Option<Tray>, notice: Option<String>) -> Self {
        Self {
            tray,
            shown: None,
            model: Model::Loading,
            reported: None,
            skip: None,
            notice,
            app: None,
            auto_paused: Vec::new(),
        }
    }

    pub(super) fn has_tray(&self) -> bool {
        self.tray.is_some()
    }

    pub(super) fn set_tray(&mut self, tray: Tray) {
        self.tray = Some(tray);
    }

    pub(super) fn app(&self) -> Option<&str> {
        self.app.as_deref()
    }

    pub(super) fn set_model(&mut self, model: Model) {
        if model == Model::Ready {
            self.reported = None;
        }
        self.model = model;
    }

    pub(super) fn is_new_failure(&mut self, error: &str) -> bool {
        let fresh = self.reported.as_deref() != Some(error);
        if fresh {
            self.reported = Some(error.to_string());
        }
        fresh
    }

    pub(super) fn set_skip(&mut self, skip: Option<SkipReason>) {
        self.skip = skip;
    }

    pub(super) fn set_app(&mut self, app: &str) {
        if self.app.as_deref() != Some(app) {
            self.app = Some(app.to_string());
        }
    }

    pub(super) fn set_auto_paused(&mut self, apps: Vec<String>) {
        self.auto_paused = apps;
    }

    pub(super) fn status_line(&self, prefs: &Preferences) -> String {
        prefs.status_line(&self.model, self.skip, self.notice.as_deref())
    }

    pub(super) fn show(&mut self, prefs: &Preferences) {
        let Some(tray) = &self.tray else { return };
        let shown = Shown {
            view: TrayView {
                status: self.status_line(prefs),
                active: prefs.enabled(),
                pause: prefs.shown_pause(),
                foreground: self.app.as_deref().map(|name| prefs.foreground_app(name)),
            },
            auto_paused: self.auto_paused.clone(),
        };
        if self.shown.as_ref() == Some(&shown) {
            return;
        }
        tray.show(shown.clone());
        self.shown = Some(shown);
    }
}

impl App {
    pub(super) fn refresh(&mut self) {
        self.shell.show(&self.prefs);
    }
}
