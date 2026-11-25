#![cfg(target_os = "linux")]

mod accessibility;
mod autostart;
mod bounded;
mod clock;
mod error;
mod hardware;
mod notice;
mod open;
mod overlay;
mod run;
mod session;
mod shared;
mod single;
mod tray;
mod x11;

pub use accessibility::Consent;
pub use autostart::Autostart;
pub use hardware::hardware;
pub use run::{InsertPolicy, Options, RunError, Start, run};
pub use session::on_wayland;
pub use shared::{Inbound, Inbox};
pub use single::single_instance;

#[cfg(feature = "e2e")]
pub use accessibility::{Reader, Reading, Report};
#[cfg(feature = "e2e")]
pub use overlay::Overlay;
#[cfg(feature = "e2e")]
pub use shared::{Held, Quit};
#[cfg(feature = "e2e")]
pub use x11::{Desktop, Display};
