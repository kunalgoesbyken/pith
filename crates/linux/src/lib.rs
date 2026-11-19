#![cfg(target_os = "linux")]

mod accessibility;
mod bounded;
mod caret;
mod error;
mod field;
mod focus;
mod identity;
mod process;
mod reader;
mod reading;
mod style;
mod text;
mod window;

pub use accessibility::Consent;
pub use error::ReaderError;
pub use reader::{Reader, ReadingCallback};
pub use reading::Reading;
