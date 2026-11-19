use std::fmt;

use atspi::AtspiError;

#[derive(Debug)]
pub enum ReaderError {
    AccessibilityOff,
    Bus(AtspiError),
    BusClosed,
    Spawn(std::io::Error),
    Stopped,
}

impl fmt::Display for ReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessibilityOff => f.write_str("desktop accessibility is switched off"),
            Self::Bus(error) => write!(f, "accessibility bus: {error}"),
            Self::BusClosed => f.write_str("the accessibility bus closed"),
            Self::Spawn(error) => write!(f, "reader thread: {error}"),
            Self::Stopped => f.write_str("reader stopped before it started"),
        }
    }
}

impl std::error::Error for ReaderError {}

impl From<AtspiError> for ReaderError {
    fn from(error: AtspiError) -> Self {
        Self::Bus(error)
    }
}
