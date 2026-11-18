use std::io;
use std::path::{Path, PathBuf};

use crate::{Config, app_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suggestions {
    Fewer,
    Normal,
    More,
}

impl Suggestions {
    pub const ALL: [Self; 3] = [Self::Fewer, Self::Normal, Self::More];

    fn thresholds(self) -> (f32, usize) {
        match self {
            Self::Fewer => (-1.1, 5),
            Self::Normal => {
                let default = Config::default();
                (default.min_mean_logprob, default.min_chars_before)
            }
            Self::More => (-2.2, 2),
        }
    }

    pub fn apply(self, config: &mut Config) {
        (config.min_mean_logprob, config.min_chars_before) = self.thresholds();
    }

    pub fn of(config: &Config) -> Option<Self> {
        let current = (config.min_mean_logprob, config.min_chars_before);
        Self::ALL
            .into_iter()
            .find(|level| level.thresholds() == current)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Enabled(bool),
    Suggestions(Suggestions),
    Block(String),
    Unblock(String),
}

impl Setting {
    pub fn apply(&self, config: &mut Config) {
        match self {
            Self::Enabled(enabled) => config.enabled = *enabled,
            Self::Suggestions(level) => level.apply(config),
            Self::Block(app) => {
                config.block_app(app);
            }
            Self::Unblock(app) => {
                config.unblock_app(app);
            }
        }
    }
}

impl Config {
    pub fn block_app(&mut self, app: &str) -> bool {
        if self.is_blocked(app) {
            return false;
        }
        self.blocked_apps.push(app_name(app));
        true
    }

    pub fn unblock_app(&mut self, app: &str) -> bool {
        let name = app_name(app);
        let before = self.blocked_apps.len();
        self.blocked_apps
            .retain(|blocked| !app_name(blocked).eq_ignore_ascii_case(&name));
        self.blocked_apps.len() != before
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pith-core-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn levels_are_distinct_and_recognised_after_a_toml_round_trip() {
        for level in Suggestions::ALL {
            let mut config = Config::default();
            level.apply(&mut config);
            let reloaded = Config::from_toml(&config.to_toml()).unwrap();
            assert_eq!(Suggestions::of(&reloaded), Some(level), "{level:?}");
        }
        assert_eq!(Suggestions::of(&Config::default()), Some(Suggestions::Normal));
    }

    #[test]
    fn levels_order_confidence_and_length() {
        let thresholds = Suggestions::ALL.map(Suggestions::thresholds);
        assert!(thresholds[0].0 > thresholds[1].0 && thresholds[1].0 > thresholds[2].0);
        assert!(thresholds[0].1 > thresholds[1].1 && thresholds[1].1 > thresholds[2].1);
    }

    #[test]
    fn hand_tuned_values_match_no_level_and_other_fields_are_kept() {
        let mut config = Config {
            min_mean_logprob: -1.23,
            max_words: 9,
            ..Config::default()
        };
        assert_eq!(Suggestions::of(&config), None);
        Suggestions::More.apply(&mut config);
        assert_eq!(config.max_words, 9);
        assert_eq!(Suggestions::of(&config), Some(Suggestions::More));
    }

    #[test]
    fn blocking_normalises_names_and_never_duplicates() {
        let mut config = Config::default();
        assert!(config.block_app(r"C:\Apps\Slack.EXE"));
        assert_eq!(config.blocked_apps, ["Slack"]);
        for again in ["slack", "Slack.exe", r"D:\x\SLACK.exe"] {
            assert!(!config.block_app(again), "{again}");
        }
        for always in ["cmd", "KeePass.exe", "Code"] {
            assert!(!config.block_app(always), "{always}");
        }
        for empty in ["", "  ", ".exe", r"C:\dir\"] {
            assert!(!config.block_app(empty), "{empty:?}");
        }
        assert_eq!(config.blocked_apps, ["Slack"]);
    }

    #[test]
    fn unblocking_removes_every_spelling_and_leaves_the_rest() {
        let mut config = Config {
            blocked_apps: vec![
                "Foo.Bar.exe".into(),
                "slack".into(),
                "SLACK.exe".into(),
                "zoom".into(),
            ],
            ..Config::default()
        };
        assert!(config.unblock_app("Slack"));
        assert_eq!(config.blocked_apps, ["Foo.Bar.exe", "zoom"]);
        assert!(!config.unblock_app("slack"));
        assert!(config.unblock_app("foo.bar"));
        assert!(!config.unblock_app("Foo"));
        assert_eq!(config.blocked_apps, ["zoom"]);
        assert!(!config.unblock_app("cmd"));
        assert!(config.is_blocked("cmd"));
    }

    #[test]
    fn settings_apply_only_their_own_field() {
        let base = Config::default();
        let mut config = base.clone();
        Setting::Enabled(false).apply(&mut config);
        assert_eq!(
            config,
            Config {
                enabled: false,
                ..base.clone()
            }
        );
        let mut config = base.clone();
        Setting::Block("zoom".into()).apply(&mut config);
        Setting::Block("zoom".into()).apply(&mut config);
        assert_eq!(config.blocked_apps, ["zoom"]);
        Setting::Unblock("ZOOM.exe".into()).apply(&mut config);
        assert_eq!(config, base);
        Setting::Suggestions(Suggestions::Fewer).apply(&mut config);
        assert_eq!(Suggestions::of(&config), Some(Suggestions::Fewer));
    }

    #[test]
    fn save_round_trips_creates_parents_and_leaves_no_temporary_file() {
        let dir = scratch("save");
        let path = dir.join("nested").join("config.toml");
        let config = Config {
            enabled: false,
            blocked_apps: vec!["Zoom".into(), "名前".into()],
            ..Config::default()
        };
        config.save(&path).unwrap();
        config.save(&path).unwrap();
        let loaded = Config::from_toml(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, config);
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["config.toml"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_failure_keeps_the_previous_file() {
        let dir = scratch("blocked");
        std::fs::create_dir_all(dir.join("config.toml.tmp")).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "enabled = false").unwrap();
        assert!(Config::default().save(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "enabled = false");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
