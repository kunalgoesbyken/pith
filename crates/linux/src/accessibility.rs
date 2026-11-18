use atspi::connection::{read_session_accessibility, set_session_accessibility};
use futures_lite::future::block_on;

use crate::error::ReaderError;

pub struct Consent(());

impl Consent {
    pub fn granted() -> Self {
        Self(())
    }
}

pub enum Accessibility {
    RequireEnabled,
    EnableWithConsent(Consent),
}

pub(crate) enum Lease {
    AlreadyEnabled,
    EnabledByPith,
}

impl Accessibility {
    pub(crate) async fn acquire(self) -> Result<Lease, ReaderError> {
        if read_session_accessibility().await? {
            return Ok(Lease::AlreadyEnabled);
        }
        match self {
            Self::RequireEnabled => Err(ReaderError::AccessibilityOff),
            Self::EnableWithConsent(_) => {
                set_session_accessibility(true).await?;
                Ok(Lease::EnabledByPith)
            }
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if matches!(self, Self::EnabledByPith) {
            let _ = block_on(set_session_accessibility(false));
        }
    }
}
