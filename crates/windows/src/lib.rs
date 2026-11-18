#![cfg(windows)]

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
mod pill;
mod reader;
mod registry;
mod run;
mod settings;
mod tray;
mod ui;
mod util;

pub use clipboard::{Trigger, paste};
pub use control::Waker;
pub use keys::KeyAction;
pub use onboarding::{
    Controls, Event as OnboardingEvent, Onboarding, Outcome, Step as OnboardingStep,
    View as OnboardingView,
};
pub use reader::{Reader, SnapshotCallback, Source};
pub use run::{Inbound, InsertMode, Options, run};
pub use settings::{SettingsWindow, StatsView};
pub use ui::force_dark_theme;
pub use util::{open_path, single_instance};
