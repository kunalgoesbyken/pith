#![cfg(windows)]

mod appearance;
pub mod brand;
mod caret;
mod clipboard;
mod control;
mod erase;
mod events;
mod foreground;
mod format;
mod keys;
mod language;
mod messages;
mod nearby;
mod onboarding;
mod overlay;
mod power;
mod reach;
mod reader;
mod registry;
mod rival;
mod run;
mod settings;
mod shortcut;
mod spell;
mod tray;
mod ui;
mod update;
mod util;
mod vault;

pub use clipboard::{Trigger, paste};
pub use control::reveal_running_instance;
pub use keys::KeyAction;
pub use onboarding::{
    Controls, Event as OnboardingEvent, Onboarding, Outcome, Step as OnboardingStep,
};
pub use overlay::OVERLAY_CLASS;
#[doc(hidden)]
pub use overlay::{Picture, Sample, preview};
pub use pith_core::{ForegroundApp, TrayCommand};
pub use reader::{Reader, SnapshotCallback, Source};
pub use registry::{BACKGROUND_FLAG, refresh_start_with_windows};
pub use rival::Rival;
pub use run::{Inbound, Inbox, InsertMode, Options, Start, run};
#[doc(hidden)]
pub use settings::Page;
pub use settings::{Overview, SettingsWindow, StatsView, StyleLine, UiEvent};
pub use shortcut::ensure_start_menu_shortcut;
#[doc(hidden)]
pub use ui::{Taskbar, force_dark_theme};
pub use util::{hardware, open_path, single_instance};
pub use vault::DpapiVault;
