#![cfg(windows)]

mod control;
mod field;
mod input;
mod onboarding;
mod overlay;
mod run;
mod settings;
mod system;
mod text;
mod tray;
mod ui;

pub use control::reveal_running_instance;
pub use field::reader::{Reader, SnapshotCallback, Source};
pub use input::clipboard::{Trigger, paste};
pub use input::keys::KeyAction;
pub use onboarding::{
    Controls, Event as OnboardingEvent, Onboarding, Outcome, Step as OnboardingStep,
};
pub use overlay::OVERLAY_CLASS;
#[doc(hidden)]
pub use overlay::{Picture, Sample, preview};
pub use run::{Inbound, Inbox, InsertMode, Options, Start, run};
#[doc(hidden)]
pub use settings::Page;
pub use settings::{Overview, SettingsWindow, StatsView, StyleLine, UiEvent};
pub use system::registry::{BACKGROUND_FLAG, refresh_start_with_windows};
pub use system::rival::Rival;
pub use system::shortcut::ensure_start_menu_shortcut;
pub use system::util::{hardware, open_path, single_instance};
pub use system::vault::DpapiVault;
pub use tray::brand;
#[doc(hidden)]
pub use ui::{Taskbar, force_dark_theme};
