#![cfg(target_os = "linux")]

mod accessibility;
mod bounded;
mod caret;
mod error;
mod field;
mod focus;
mod hardware;
mod held;
mod identity;
mod insert;
mod overlay;
mod process;
mod reader;
mod reading;
mod render;
mod run;
mod session;
mod settle;
mod single;
mod style;
mod text;
mod typeset;
mod window;
mod x11;

pub use accessibility::Consent;
pub use hardware::hardware;
pub use run::{Inbound, Inbox, InsertPolicy, Options, RunError, run};
pub use session::on_wayland;
pub use single::single_instance;

#[cfg(feature = "e2e")]
pub use held::Held;
#[cfg(feature = "e2e")]
pub use overlay::Overlay;
#[cfg(feature = "e2e")]
pub use reader::{Reader, Report};
#[cfg(feature = "e2e")]
pub use reading::Reading;
#[cfg(feature = "e2e")]
pub use x11::{Desktop, Display};
