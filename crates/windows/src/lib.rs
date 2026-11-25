#![cfg(windows)]

pub mod brand;
mod caret;
mod clipboard;
mod control;
mod events;
mod foreground;
mod format;
mod keys;
mod language;
mod messages;
mod nearby;
mod onboarding;
mod overlay;
mod reach;
mod reader;
mod run;
mod settings;
mod spell;
mod system;
mod tray;
mod ui;

pub use clipboard::{Trigger, paste};
pub use control::reveal_running_instance;
pub use keys::KeyAction;
pub use onboarding::{
    Controls, Event as OnboardingEvent, Onboarding, Outcome, Step as OnboardingStep,
};
pub use overlay::OVERLAY_CLASS;
#[doc(hidden)]
pub use overlay::{Picture, Sample, preview};
pub use reader::{Reader, SnapshotCallback, Source};
pub use run::{Inbound, Inbox, InsertMode, Options, Start, run};
#[doc(hidden)]
pub use settings::Page;
pub use settings::{Overview, SettingsWindow, StatsView, StyleLine, UiEvent};
pub use system::registry::{BACKGROUND_FLAG, refresh_start_with_windows};
pub use system::rival::Rival;
pub use system::shortcut::ensure_start_menu_shortcut;
pub use system::util::{hardware, open_path, single_instance};
pub use system::vault::DpapiVault;
#[doc(hidden)]
pub use ui::{Taskbar, force_dark_theme};
