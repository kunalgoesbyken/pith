use pith_core::{Access, Event, Setting, log};
use pith_infer::UpdateCheck;

use super::App;
use crate::registry;
use crate::rival;
use crate::settings::{UiEvent, take_events};
use crate::tray::TrayCommand;
use crate::update::Update;
use crate::util::{open_path, open_target};

impl App {
    pub(super) fn ui_events(&mut self) -> bool {
        for event in take_events() {
            match event {
                UiEvent::Apply(setting) => self.change(setting),
                UiEvent::OpenDataFolder => {
                    if let Some(dir) = &self.data_dir {
                        let _ = std::fs::create_dir_all(dir);
                        open_path(dir);
                    }
                }
                UiEvent::DeleteData => {
                    let confirmed = self
                        .data_dir
                        .clone()
                        .filter(|_| self.presenter.confirm_delete());
                    if confirmed.is_some() {
                        self.erasing = confirmed;
                        return true;
                    }
                }
                UiEvent::StartWithWindows(enabled) => {
                    if let Err(error) = registry::set_start_with_windows(enabled) {
                        log(&format!("start with Windows not changed: {error}"));
                    }
                    self.presenter.refresh_settings(&self.prefs);
                }
                UiEvent::ResetStats => self.reset_stats(),
                UiEvent::OpenTypingSettings => open_target(rival::TYPING_SETTINGS_PAGE),
                UiEvent::Closed => self.presenter.close_settings(),
            }
        }
        false
    }

    pub(super) fn tray_commands(&mut self) -> bool {
        while let Some(command) = self.presenter.next_tray_command() {
            if self.command(command) {
                return true;
            }
        }
        false
    }

    pub(super) fn command(&mut self, command: TrayCommand) -> bool {
        match command {
            TrayCommand::Pause(span) => self.pause(span),
            TrayCommand::Resume => self.resume(),
            TrayCommand::ResumeApp(app) => self.dispatch(Event::Resume(app)),
            TrayCommand::ChangeApp => self.change_foreground_app(),
            TrayCommand::Update => self.update_clicked(),
            TrayCommand::Settings => self.presenter.open_settings(&self.prefs),
            TrayCommand::Quit => return true,
        }
        false
    }
}

impl App {
    fn update_clicked(&mut self) {
        match self.presenter.update() {
            Update::Done(UpdateCheck::Available(release)) => open_target(&release.url),
            Update::Checking => {}
            Update::Idle | Update::Done(_) => self.start_update_check(),
        }
    }

    fn change_foreground_app(&mut self) {
        let Some(app) = self.presenter.foreground_app() else {
            return;
        };
        match self.prefs.saved().access(&app) {
            Access::On | Access::OptedIn => self.change(Setting::Block(app)),
            Access::OffByDefault => self.change(Setting::Allow(app)),
            Access::Off => {}
        }
    }
}
