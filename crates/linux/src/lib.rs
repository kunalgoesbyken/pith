#![cfg(target_os = "linux")]

mod accessibility;
mod bounded;
mod error;
mod host;
mod overlay;
mod run;
mod shared;
mod x11;

pub use accessibility::Consent;
pub use host::{Autostart, hardware, on_wayland, single_instance};
pub use run::{InsertPolicy, Options, RunError, Start, run};
pub use shared::{Inbound, Inbox};

#[cfg(feature = "e2e")]
pub use accessibility::{Reader, Reading, Report};
#[cfg(feature = "e2e")]
pub use overlay::Overlay;
#[cfg(feature = "e2e")]
pub use shared::{Held, Quit};
#[cfg(feature = "e2e")]
pub use x11::{Desktop, Display};
