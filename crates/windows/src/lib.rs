#![cfg(windows)]

mod caret;
mod clipboard;
mod events;
mod keys;
mod onboarding;
mod overlay;
mod reader;
mod run;
mod tray;
mod util;

pub use keys::{ACCEPT_ALL, ACCEPT_WORD, DISMISS};
pub use onboarding::{
    Controls, Event as OnboardingEvent, Onboarding, Outcome, Step as OnboardingStep,
    View as OnboardingView,
};
pub use reader::Source;
pub use run::{Inbound, InsertMode, Options, Waker, run};
pub use util::{open_path, single_instance};
