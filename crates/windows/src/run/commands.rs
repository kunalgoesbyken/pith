use pith_core::{Event, log};
use pith_infer::UpdateCheck;

use super::App;
use crate::registry;
use crate::rival;
use crate::settings::{UiEvent, take_events};
use crate::tray::TrayEvent;
use crate::update::Update;
use crate::util::{open_path, open_target};
use pith_core::TrayCommand;

impl App {
    pub(super) fn ui_events(&mut self) -> bool {
        take_events().into_iter().any(|event| self.ui_event(event))
    }

    pub(super) fn ui_event(&mut self, event: UiEvent) -> bool {
        match event {
            UiEvent::Apply(setting) => match setting.checked(self.prefs.saved()) {
                Ok(setting) => self.change(setting),
                Err(error) => log(&format!("setting refused: {error}")),
            },
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
            UiEvent::Command(command) => return self.command(command),
            UiEvent::OpenConfigFile => {
                if let Some(path) = self.prefs.config_path() {
                    open_path(path);
                }
            }
            UiEvent::StartWithWindows(enabled) => {
                if let Err(error) = registry::set_start_with_windows(enabled) {
                    log(&format!("start with Windows not changed: {error}"));
                }
                self.presenter.refresh_settings(&self.prefs);
            }
            UiEvent::ResetStats => self.reset_stats(),
            UiEvent::ForgetSentence { app, text } => self.forget_sentence(&app, &text),
            UiEvent::ForgetStyle => self.forget_all_style(),
            UiEvent::OpenTypingSettings => open_target(rival::TYPING_SETTINGS_PAGE),
            UiEvent::Closed => self.presenter.close_settings(),
        }
        false
    }

    pub(super) fn tray_commands(&mut self) -> bool {
        while let Some(event) = self.presenter.next_tray_event() {
            match event {
                TrayEvent::Command(command) => {
                    if self.command(command) {
                        return true;
                    }
                }
                TrayEvent::Update => self.update_clicked(),
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
            TrayCommand::Open => self.presenter.open_settings(&self.prefs),
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
        if let Some(setting) = self.prefs.saved().switch_app(&app) {
            self.change(setting);
        }
    }
}
