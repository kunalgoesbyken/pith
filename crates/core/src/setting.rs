use std::io;
use std::path::Path;

use crate::stored::{patch, write_atomically};
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

    pub fn persist(&self, path: &Path) -> Result<(), String> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.to_string()),
        };
        let before = Config::from_toml(&source)?;
        let mut after = before.clone();
        self.apply(&mut after);
        let text = patch(&source, &before, &after)?;
        if text == source {
            return Ok(());
        }
        write_atomically(path, &text).map_err(|error| error.to_string())
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
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
        assert_eq!(
            Suggestions::of(&Config::default()),
            Some(Suggestions::Normal)
        );
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
        write_atomically(&path, &config.to_toml()).unwrap();
        write_atomically(&path, &config.to_toml()).unwrap();
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
    fn persisting_changes_only_its_own_field_in_the_stored_file() {
        let dir = scratch("persist");
        let path = dir.join("config.toml");
        Setting::Block("Zoom".into()).persist(&path).unwrap();
        assert_eq!(
            Config::from_toml(&std::fs::read_to_string(&path).unwrap())
                .unwrap()
                .blocked_apps,
            ["Zoom"]
        );
        std::fs::write(
            &path,
            "max_words = 9
blocked_apps = [\"Zoom\"]
",
        )
        .unwrap();
        Setting::Suggestions(Suggestions::More)
            .persist(&path)
            .unwrap();
        Setting::Enabled(false).persist(&path).unwrap();
        let stored = Config::from_toml(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored.max_words, 9);
        assert_eq!(stored.blocked_apps, ["Zoom"]);
        assert!(!stored.enabled);
        assert_eq!(Suggestions::of(&stored), Some(Suggestions::More));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persisting_never_overwrites_a_file_it_cannot_read() {
        let dir = scratch("persist-invalid");
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        for broken in ["max_words = 0", "this is = not toml ["] {
            std::fs::write(&path, broken).unwrap();
            assert!(Setting::Enabled(false).persist(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_failure_keeps_the_previous_file() {
        let dir = scratch("blocked");
        std::fs::create_dir_all(dir.join("config.toml.tmp")).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "enabled = false").unwrap();
        assert!(write_atomically(&path, &Config::default().to_toml()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "enabled = false");
        let _ = std::fs::remove_dir_all(&dir);
    }

    const COMMENTED: &str = r#"# Pith settings, edited by hand
max_words = 9 # keep suggestions short

# apps where Pith stays quiet
blocked_apps = [
  "Zoom", # video calls
  'Teams',
]

[unknown_table]
note = "kept"
"#;

    fn stored(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn persisting_keeps_comments_layout_and_unknown_keys() {
        let dir = scratch("comments");
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, COMMENTED).unwrap();
        Setting::Enabled(false).persist(&path).unwrap();
        Setting::Suggestions(Suggestions::More)
            .persist(&path)
            .unwrap();
        Setting::Block("Slack".into()).persist(&path).unwrap();
        Setting::Unblock("zoom".into()).persist(&path).unwrap();
        let text = stored(&path);
        for kept in [
            "# Pith settings, edited by hand",
            "max_words = 9 # keep suggestions short",
            "# apps where Pith stays quiet",
            "'Teams',",
            "[unknown_table]",
            r#"note = "kept""#,
        ] {
            assert!(
                text.contains(kept),
                "{kept}
{text}"
            );
        }
        assert!(!text.contains("Zoom"), "{text}");
        assert!(
            text.contains(
                "min_mean_logprob = -2.2
"
            ),
            "{text}"
        );
        assert!(text.contains("# video calls"), "{text}");
        let config = Config::from_toml(&text).unwrap();
        assert_eq!(config.blocked_apps, ["Teams", "Slack"]);
        assert_eq!(config.max_words, 9);
        assert!(!config.enabled);
        assert_eq!(Suggestions::of(&config), Some(Suggestions::More));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persisting_a_no_op_leaves_the_file_byte_identical() {
        let dir = scratch("noop");
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        let source = COMMENTED;
        std::fs::write(&path, &source).unwrap();
        Setting::Block("teams.exe".into()).persist(&path).unwrap();
        Setting::Unblock("nothing".into()).persist(&path).unwrap();
        Setting::Enabled(true).persist(&path).unwrap();
        assert_eq!(stored(&path), source);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persisting_only_writes_the_keys_it_changes() {
        let dir = scratch("sparse");
        let path = dir.join("config.toml");
        Setting::Enabled(false).persist(&path).unwrap();
        assert_eq!(stored(&path).trim(), "enabled = false");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
