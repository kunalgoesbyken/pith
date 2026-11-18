use std::time::{Duration, SystemTime};

use crate::Config;

const MINUTES_PER_DAY: u32 = 24 * 60;
const SECONDS_PER_MINUTE: u64 = 60;

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
pub enum Pause {
    Until(String),
    UntilResumed,
    BatterySaver,
}

struct UserPause {
    shown: Pause,
    ends: Option<SystemTime>,
}

#[derive(Default)]
pub struct Pauses {
    user: Option<UserPause>,
    battery_saver: bool,
    model_released: bool,
}

impl Pauses {
    pub fn start(&mut self, span: PauseSpan, clock: (u32, u32), now: SystemTime) {
        self.user = Some(match span {
            PauseSpan::UntilResumed => UserPause {
                shown: Pause::UntilResumed,
                ends: None,
            },
            PauseSpan::Minutes(minutes) => UserPause {
                shown: Pause::Until(clock_after(clock.0, clock.1, minutes)),
                ends: now.checked_add(Duration::from_secs(u64::from(minutes) * SECONDS_PER_MINUTE)),
            },
        });
    }

    pub fn resume(&mut self) -> bool {
        self.user.take().is_some()
    }

    pub fn expire(&mut self, now: SystemTime) -> bool {
        let due = self
            .user
            .as_ref()
            .and_then(|pause| pause.ends)
            .is_some_and(|ends| now >= ends);
        due && self.resume()
    }

    pub fn set_battery_saver(&mut self, on: bool) -> bool {
        let changed = self.battery_saver != on;
        self.battery_saver = on;
        changed
    }

    pub fn by_user(&self) -> bool {
        self.user.is_some()
    }

    fn by_power(&self, config: &Config) -> bool {
        config.enabled && config.pause_on_battery_saver && self.battery_saver
    }

    pub fn active(&self, config: &Config) -> bool {
        self.by_user() || self.by_power(config)
    }

    pub fn shown(&self, config: &Config) -> Option<Pause> {
        self.user
            .as_ref()
            .map(|pause| pause.shown.clone())
            .or_else(|| self.by_power(config).then_some(Pause::BatterySaver))
    }

    pub fn release_model(&mut self, config: &Config) -> bool {
        let paused = self.by_power(config);
        let entering = paused && !self.model_released;
        self.model_released = paused;
        entering
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_wraps_past_midnight_and_ignores_whole_days() {
        assert_eq!(clock_after(14, 20, 15), "14:35");
        assert_eq!(clock_after(14, 50, 60), "15:50");
        assert_eq!(clock_after(23, 50, 15), "00:05");
        assert_eq!(clock_after(0, 0, 0), "00:00");
        assert_eq!(clock_after(9, 5, 24 * 60), "09:05");
        assert_eq!(clock_after(99, 99, 7), "03:46");
    }
}
