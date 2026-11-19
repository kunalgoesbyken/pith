use crate::{Pause, SkipReason};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Model {
    Loading,
    Downloading { percent: u64 },
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub model: Model,
    pub pause: Option<Pause>,
    pub enabled: bool,
    pub skip: Option<SkipReason>,
    pub notice: Option<String>,
}

pub fn skip_label(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Disabled => "turned off",
        SkipReason::Password => "password field",
        SkipReason::Composing => "composing text",
        SkipReason::BlockedApp => "turned off in this app",
        SkipReason::Selection => "text is selected",
        SkipReason::TextAfterCaret => "text after the cursor",
        SkipReason::TooShort => "not enough text yet",
        SkipReason::NotProse => "not prose",
        SkipReason::NoRoom => "no room for the suggestion",
    }
}

impl Status {
    pub fn line(&self) -> String {
        match (&self.pause, &self.model) {
            (Some(Pause::Until(clock)), _) => format!("Paused until {clock}"),
            (Some(Pause::UntilResumed), _) => "Paused until you resume".to_string(),
            (Some(Pause::BatterySaver), _) => "Paused while battery saver is on".to_string(),
            (None, _) if !self.enabled => "Turned off".to_string(),
            (None, Model::Failed(error)) => format!("Error: {error}"),
            (None, Model::Downloading { percent }) => format!("Downloading model {percent}%"),
            (None, Model::Loading) => "Loading model".to_string(),
            (None, Model::Ready) => match (&self.notice, self.skip) {
                (Some(notice), _) => format!("Settings ignored: {notice}"),
                (None, Some(reason)) => format!("Skipped: {}", skip_label(reason)),
                (None, None) => "Ready".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(
        model: Model,
        pause: Option<Pause>,
        enabled: bool,
        skip: Option<SkipReason>,
    ) -> Status {
        Status {
            model,
            pause,
            enabled,
            skip,
            notice: None,
        }
    }

    #[test]
    fn line_follows_the_documented_priority() {
        let skip = Some(SkipReason::Password);
        let rows = [
            (status(Model::Ready, None, true, None), "Ready"),
            (
                Status {
                    notice: Some("max_words must be at least 1".into()),
                    ..status(Model::Ready, None, true, skip)
                },
                "Settings ignored: max_words must be at least 1",
            ),
            (
                status(Model::Ready, None, true, skip),
                "Skipped: password field",
            ),
            (status(Model::Loading, None, true, skip), "Loading model"),
            (
                status(Model::Downloading { percent: 40 }, None, true, None),
                "Downloading model 40%",
            ),
            (
                status(Model::Failed("no CPU".into()), None, true, skip),
                "Error: no CPU",
            ),
            (status(Model::Ready, None, false, skip), "Turned off"),
            (
                status(
                    Model::Loading,
                    Some(Pause::Until("14:35".into())),
                    true,
                    skip,
                ),
                "Paused until 14:35",
            ),
            (
                status(Model::Ready, Some(Pause::UntilResumed), false, None),
                "Paused until you resume",
            ),
            (
                status(Model::Ready, Some(Pause::BatterySaver), true, skip),
                "Paused while battery saver is on",
            ),
        ];
        for (status, expected) in rows {
            assert_eq!(status.line(), expected, "{status:?}");
        }
    }
}
