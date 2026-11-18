use pith_infer::{UpdateCheck, UpdateFailure};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Update {
    #[default]
    Idle,
    Checking,
    Done(UpdateCheck),
}

impl Update {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "Check for updates".to_string(),
            Self::Checking => "Checking for updates\u{2026}".to_string(),
            Self::Done(UpdateCheck::UpToDate) => format!(
                "Up to date (Pith {}) \u{2014} check again",
                env!("CARGO_PKG_VERSION")
            ),
            Self::Done(UpdateCheck::Available(release)) => format!(
                "Pith {} is available \u{2014} open the release page",
                release.version
            ),
            Self::Done(UpdateCheck::Failed(UpdateFailure::Offline)) => {
                "Could not reach GitHub \u{2014} check again".to_string()
            }
            Self::Done(UpdateCheck::Failed(UpdateFailure::RateLimited)) => {
                "GitHub is limiting requests, try later \u{2014} check again".to_string()
            }
            Self::Done(UpdateCheck::Failed(UpdateFailure::BadResponse)) => {
                "GitHub sent an unexpected answer \u{2014} check again".to_string()
            }
        }
    }

    pub fn can_act(&self) -> bool {
        *self != Self::Checking
    }
}
