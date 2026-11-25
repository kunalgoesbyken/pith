use atspi::connection::{read_session_accessibility, set_session_accessibility};

use crate::bounded::{DEADLINE, STARTUP_DEADLINE, within};
use crate::error::ReaderError;

pub struct Consent(());

impl Consent {
    pub fn granted() -> Self {
        Self(())
    }
}

pub(crate) enum Lease {
    AlreadyEnabled,
    EnabledByPith,
}

impl Lease {
    pub(crate) async fn acquire(consent: Option<Consent>) -> Result<Self, ReaderError> {
        let enabled = within(STARTUP_DEADLINE, read_session_accessibility())
            .await
            .ok_or(ReaderError::StartupTimedOut)??;
        if enabled {
            return Ok(Self::AlreadyEnabled);
        }
        match consent {
            None => Err(ReaderError::AccessibilityOff),
            Some(Consent(())) => {
                match within(STARTUP_DEADLINE, set_session_accessibility(true)).await {
                    Some(Ok(())) => Ok(Self::EnabledByPith),
                    Some(Err(error)) => Err(error.into()),
                    None => {
                        Self::EnabledByPith.release().await;
                        Err(ReaderError::StartupTimedOut)
                    }
                }
            }
        }
    }

    pub(crate) async fn release(self) {
        if matches!(self, Self::EnabledByPith) {
            let _ = within(DEADLINE, set_session_accessibility(false)).await;
        }
    }
}
