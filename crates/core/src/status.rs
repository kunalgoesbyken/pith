use crate::{Pause, SkipReason};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Model {
    Loading(String),
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
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
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
            (None, Model::Loading(text)) if text.starts_with("downloading") => capitalised(text),
            (None, Model::Loading(_)) => "Loading model".to_string(),
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
    fn every_skip_reason_has_a_distinct_nonempty_label() {
        let all = [
            SkipReason::Disabled,
            SkipReason::Password,
            SkipReason::Composing,
            SkipReason::BlockedApp,
            SkipReason::Selection,
            SkipReason::TextAfterCaret,
            SkipReason::TooShort,
            SkipReason::NotProse,
        ];
        let mut labels: Vec<_> = all.into_iter().map(skip_label).collect();
        assert!(labels.iter().all(|label| !label.is_empty()));
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), all.len());
    }

    #[test]
    fn line_follows_the_documented_priority() {
        let skip = Some(SkipReason::Password);
        let loading = Model::Loading("loading model.gguf".into());
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
            (status(loading.clone(), None, true, skip), "Loading model"),
            (
                status(
                    Model::Loading("downloading model 40%".into()),
                    None,
                    true,
                    None,
                ),
                "Downloading model 40%",
            ),
            (
                status(Model::Failed("no CPU".into()), None, true, skip),
                "Error: no CPU",
            ),
            (status(Model::Ready, None, false, skip), "Turned off"),
            (
                status(loading, Some(Pause::Until("14:35".into())), true, skip),
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

    #[test]
    fn capitalising_keeps_the_rest_and_handles_non_ascii() {
        assert_eq!(capitalised("ünïcode"), "Ünïcode");
        assert_eq!(capitalised(""), "");
        assert_eq!(capitalised("ǆx"), "ǅx".replace('ǅ', "Ǆ"));
    }
}
