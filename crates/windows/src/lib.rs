#![cfg(windows)]

pub mod brand;
mod caret;
mod clipboard;
mod control;
mod erase;
mod events;
mod foreground;
mod format;
mod keys;
mod onboarding;
mod overlay;
mod power;
mod reach;
mod reader;
mod registry;
mod rival;
mod run;
mod settings;
mod tray;
mod ui;
mod update;
mod util;

pub use clipboard::{Trigger, paste};
pub use keys::KeyAction;
pub use onboarding::{
    Controls, Event as OnboardingEvent, Onboarding, Outcome, Step as OnboardingStep,
};
pub use reader::{Reader, SnapshotCallback, Source};
pub use rival::Rival;
pub use run::{Inbound, Inbox, InsertMode, Options, run};
pub use settings::{SettingsWindow, StatsView};
pub use tray::TrayCommand;
pub use ui::force_dark_theme;
pub use util::{hardware, open_path, single_instance};
