use std::time::SystemTime;

use pith_core::{Event, Model, PauseSpan, Preferences, Setting};

use super::App;
use crate::power::Power;
use crate::util::local_time;

pub(super) fn read_power(prefs: &mut Preferences) -> bool {
    prefs.set_battery_saver(Power::read() == Power::BatterySaver)
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
        if self.prefs.release_model() {
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
        let now = local_time();
        let clock = (u32::from(now.wHour), u32::from(now.wMinute));
        self.prefs.start_pause(span, clock, SystemTime::now());
        self.sync();
    }

    pub(super) fn resume(&mut self) {
        if self.prefs.resume() {
            self.sync();
        }
    }

    pub(super) fn expire_pause(&mut self) {
        if self.prefs.expire_pause(SystemTime::now()) {
            self.sync();
        }
    }

    pub(super) fn power_changed(&mut self) {
        if read_power(&mut self.prefs) {
            self.sync();
        }
    }
}
