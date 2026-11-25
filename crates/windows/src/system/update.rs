use pith_infer::{UpdateCheck, UpdateFailure};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Update {
    #[default]
    Idle,
    Checking,
    Done(UpdateCheck),
}

fn retry(outcome: &str) -> String {
    format!("{outcome} \u{2014} check again")
}

impl Update {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "Check for updates".to_string(),
            Self::Checking => "Checking for updates\u{2026}".to_string(),
            Self::Done(UpdateCheck::UpToDate) => {
                retry(&format!("Up to date (Pith {})", env!("CARGO_PKG_VERSION")))
            }
            Self::Done(UpdateCheck::Available(release)) => format!(
                "Pith {} is available \u{2014} open the release page",
                release.version
            ),
            Self::Done(UpdateCheck::Failed(UpdateFailure::Offline)) => {
                retry("Could not reach GitHub")
            }
            Self::Done(UpdateCheck::Failed(UpdateFailure::RateLimited)) => {
                retry("GitHub is limiting requests, try later")
            }
            Self::Done(UpdateCheck::Failed(UpdateFailure::BadResponse)) => {
                retry("GitHub sent an unexpected answer")
            }
        }
    }

    pub fn can_act(&self) -> bool {
        *self != Self::Checking
    }
}
