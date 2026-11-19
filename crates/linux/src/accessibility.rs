use atspi::connection::{read_session_accessibility, set_session_accessibility};

use crate::bounded::within;
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
        if read_session_accessibility().await? {
            return Ok(Self::AlreadyEnabled);
        }
        match consent {
            None => Err(ReaderError::AccessibilityOff),
            Some(Consent(())) => {
                set_session_accessibility(true).await?;
                Ok(Self::EnabledByPith)
            }
        }
    }

    pub(crate) async fn release(self) {
        if matches!(self, Self::EnabledByPith) {
            let _ = within(async {
                if read_session_accessibility().await.unwrap_or(false) {
                    let _ = set_session_accessibility(false).await;
                }
            })
            .await;
        }
    }
}
