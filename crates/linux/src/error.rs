use std::fmt;

use atspi::AtspiError;

#[derive(Debug)]
pub enum ReaderError {
    AccessibilityOff,
    Bus(AtspiError),
    BusClosed,
    Spawn(std::io::Error),
    Stopped,
    StartupTimedOut,
}

impl fmt::Display for ReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessibilityOff => f.write_str("desktop accessibility is switched off"),
            Self::Bus(error) => write!(f, "accessibility bus: {error}"),
            Self::BusClosed => f.write_str("the accessibility bus closed"),
            Self::Spawn(error) => write!(f, "reader thread: {error}"),
            Self::Stopped => f.write_str("reader stopped before it started"),
            Self::StartupTimedOut => f.write_str("the accessibility bus did not answer in time"),
        }
    }
}

impl std::error::Error for ReaderError {}

impl From<AtspiError> for ReaderError {
    fn from(error: AtspiError) -> Self {
        Self::Bus(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inserted {
    Confirmed,
    Unconfirmed,
}

#[derive(Debug)]
pub enum InsertError {
    NoField,
    Unsupported,
    Rejected,
    TimedOut,
    Bus(AtspiError),
}

impl fmt::Display for InsertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoField => f.write_str("no field has the focus"),
            Self::Unsupported => f.write_str("the field does not accept inserted text"),
            Self::Rejected => f.write_str("the field refused the inserted text"),
            Self::TimedOut => f.write_str("the field did not answer in time"),
            Self::Bus(error) => write!(f, "accessibility bus: {error}"),
        }
    }
}

impl std::error::Error for InsertError {}

impl From<AtspiError> for InsertError {
    fn from(error: AtspiError) -> Self {
        Self::Bus(error)
    }
}

impl From<zbus::Error> for InsertError {
    fn from(error: zbus::Error) -> Self {
        Self::Bus(error.into())
    }
}
