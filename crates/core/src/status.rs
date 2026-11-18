use crate::SkipReason;

const MINUTES_PER_DAY: u32 = 24 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseSpan {
    Minutes(u32),
    UntilResumed,
}

impl PauseSpan {
    pub const CHOICES: [Self; 3] = [Self::Minutes(15), Self::Minutes(60), Self::UntilResumed];

    pub fn label(self) -> String {
        match self {
            Self::Minutes(minutes) if minutes % 60 == 0 => match minutes / 60 {
                1 => "Pause for 1 hour".to_string(),
                hours => format!("Pause for {hours} hours"),
            },
            Self::Minutes(minutes) => format!("Pause for {minutes} min"),
            Self::UntilResumed => "Pause until I resume".to_string(),
        }
    }
}

pub fn clock_after(hour: u32, minute: u32, minutes: u32) -> String {
    let start = (hour % 24) * 60 + minute % 60;
    let end = (start + minutes % MINUTES_PER_DAY) % MINUTES_PER_DAY;
    format!("{:02}:{:02}", end / 60, end % 60)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Model {
    Loading(String),
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pause {
    Until(String),
    UntilResumed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub model: Model,
    pub pause: Option<Pause>,
    pub enabled: bool,
    pub skip: Option<SkipReason>,
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
            (None, _) if !self.enabled => "Turned off".to_string(),
            (None, Model::Failed(error)) => format!("Error: {error}"),
            (None, Model::Loading(text)) if text.starts_with("downloading") => capitalised(text),
            (None, Model::Loading(_)) => "Loading model".to_string(),
            (None, Model::Ready) => match self.skip {
                Some(reason) => format!("Skipped: {}", skip_label(reason)),
                None => "Ready".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(model: Model, pause: Option<Pause>, enabled: bool, skip: Option<SkipReason>) -> Status {
        Status {
            model,
            pause,
            enabled,
            skip,
        }
    }

    #[test]
    fn clock_wraps_past_midnight_and_ignores_whole_days() {
        assert_eq!(clock_after(14, 20, 15), "14:35");
        assert_eq!(clock_after(14, 50, 60), "15:50");
        assert_eq!(clock_after(23, 50, 15), "00:05");
        assert_eq!(clock_after(0, 0, 0), "00:00");
        assert_eq!(clock_after(9, 5, 24 * 60), "09:05");
        assert_eq!(clock_after(99, 99, 7), "03:46");
    }

    #[test]
    fn pause_choices_have_distinct_labels() {
        let labels: Vec<String> = PauseSpan::CHOICES.into_iter().map(PauseSpan::label).collect();
        assert_eq!(
            labels,
            ["Pause for 15 min", "Pause for 1 hour", "Pause until I resume"]
        );
        assert_eq!(PauseSpan::Minutes(120).label(), "Pause for 2 hours");
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
            (status(Model::Ready, None, true, skip), "Skipped: password field"),
            (status(loading.clone(), None, true, skip), "Loading model"),
            (
                status(Model::Loading("downloading model 40%".into()), None, true, None),
                "Downloading model 40%",
            ),
            (status(Model::Failed("no CPU".into()), None, true, skip), "Error: no CPU"),
            (status(Model::Ready, None, false, skip), "Turned off"),
            (
                status(loading, Some(Pause::Until("14:35".into())), true, skip),
                "Paused until 14:35",
            ),
            (
                status(Model::Ready, Some(Pause::UntilResumed), false, None),
                "Paused until you resume",
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
