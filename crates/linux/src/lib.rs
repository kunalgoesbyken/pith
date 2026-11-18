#![cfg(target_os = "linux")]

mod accessibility;
mod caret;
mod error;
mod field;
mod focus;
mod process;
mod reader;
mod style;
mod window;

pub use accessibility::{Accessibility, Consent};
pub use error::ReaderError;
pub use reader::{Reader, SnapshotCallback};
