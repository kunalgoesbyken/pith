#![cfg(windows)]

mod caret;
mod clipboard;
mod events;
mod foreground;
mod keys;
mod onboarding;
mod overlay;
mod reader;
mod registry;
mod run;
mod settings;
mod tray;
mod ui;
mod util;

pub use keys::{ACCEPT_ALL, ACCEPT_WORD, DISMISS, TRIGGER};
pub use onboarding::{
    Controls, Event as OnboardingEvent, Onboarding, Outcome, Step as OnboardingStep,
    View as OnboardingView,
};
pub use reader::{Reader, SnapshotCallback, Source};
pub use run::{Inbound, InsertMode, Options, Waker, run};
pub use settings::SettingsWindow;
pub use ui::force_dark_theme;
pub use util::{open_path, single_instance};
