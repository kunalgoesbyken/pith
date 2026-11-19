#![cfg(target_os = "linux")]

mod accessibility;
mod autostart;
mod bounded;
mod caret;
mod clock;
mod current;
mod error;
mod field;
mod focus;
mod hardware;
mod held;
mod identity;
mod insert;
mod nearby;
mod notice;
mod open;
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
mod tray;
mod typeset;
mod window;
mod x11;

pub use accessibility::Consent;
pub use autostart::Autostart;
pub use hardware::hardware;
pub use run::{Inbound, Inbox, InsertPolicy, Options, RunError, Start, run};
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
pub use session::Quit;
#[cfg(feature = "e2e")]
pub use x11::{Desktop, Display};
