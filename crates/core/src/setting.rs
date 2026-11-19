use std::path::Path;

use crate::apps::{Access, editor, same_app};
use crate::config::{MAX_GHOST_OPACITY, MIN_GHOST_OPACITY};
use crate::keybind::ensure_bindable;
use crate::stored::{patch, read, write_atomically};
use crate::{Chord, Config, KeyAction, ModelPreset, TriggerMode, app_name};

pub const IDLE_UNLOAD_CHOICES: [u64; 4] = [60, 300, 900, 0];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Length {
    Short,
    Normal,
    Long,
}

impl Length {
    pub const ALL: [Self; 3] = [Self::Short, Self::Normal, Self::Long];

    fn words(self) -> usize {
        match self {
            Self::Short => 3,
            Self::Normal => Config::default().max_words,
            Self::Long => 10,
        }
    }

    fn tokens(self) -> usize {
        let defaults = Config::default();
        (self.words() * defaults.max_tokens).div_ceil(defaults.max_words)
    }

    pub fn apply(self, config: &mut Config) {
        config.max_words = self.words();
        config.max_tokens = self.tokens();
    }

    pub fn of(config: &Config) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|length| length.words() == config.max_words)
    }
}

pub fn clamp_opacity(percent: u8) -> u8 {
    percent.clamp(MIN_GHOST_OPACITY, MAX_GHOST_OPACITY)
}

pub fn thread_choices(cores: usize) -> Vec<usize> {
    let doubling = std::iter::successors(Some(1usize), |threads| threads.checked_mul(2));
    std::iter::once(0)
        .chain(doubling.take_while(|threads| *threads <= cores))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Enabled(bool),
    TypoFix(bool),
    Suggestions(Suggestions),
    Length(Length),
    GhostOpacity(u8),
    TriggerMode(TriggerMode),
    SmartSpace(bool),
    AutoPause(bool),
    Bind(KeyAction, usize, Option<Chord>),
    DefaultKeys,
    Model(ModelPreset),
    IdleUnload(u64),
    BatterySaver(bool),
    Threads(usize),
    Block(String),
    Unblock(String),
    Allow(String),
    Disallow(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppChoice {
    pub setting: Setting,
    pub off_by_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backend<'a> {
    Remote {
        url: &'a str,
        model: Option<&'a str>,
    },
    File(&'a Path),
    Preset(ModelPreset),
}

impl Setting {
    pub fn apply(&self, config: &mut Config) {
        match self {
            Self::Enabled(enabled) => config.enabled = *enabled,
            Self::TypoFix(on) => config.typo_fix = *on,
            Self::Suggestions(level) => level.apply(config),
            Self::Length(length) => length.apply(config),
            Self::GhostOpacity(percent) => config.ghost_opacity = clamp_opacity(*percent),
            Self::TriggerMode(mode) => {
                config.trigger_mode = *mode;
                config.force_trigger |= *mode == TriggerMode::Manual;
            }
            Self::SmartSpace(on) => config.smart_space = *on,
            Self::AutoPause(on) => match (*on, config.auto_pause_after) {
                (false, _) => config.auto_pause_after = 0,
                (true, 0) => config.auto_pause_after = Config::default().auto_pause_after,
                (true, _) => {}
            },
            Self::Bind(action, slot, chord) => config.set_chord(*action, *slot, *chord),
            Self::DefaultKeys => *config = config.default_keys(),
            Self::Model(preset) => config.model = Some(*preset),
            Self::IdleUnload(seconds) => config.idle_unload_secs = *seconds,
            Self::BatterySaver(on) => config.pause_on_battery_saver = *on,
            Self::Threads(threads) => config.threads = *threads,
            Self::Block(app) => {
                config.block_app(app);
            }
            Self::Unblock(app) => {
                config.unblock_app(app);
            }
            Self::Allow(app) => {
                config.allow_app(app);
            }
            Self::Disallow(app) => {
                config.disallow_app(app);
            }
        }
    }

    pub fn checked(self, config: &Config) -> Result<Self, String> {
        if let Self::Bind(action, _, chord) = &self {
            ensure_bindable(*action, *chord)?;
        }
        let mut next = config.clone();
        self.apply(&mut next);
        next.validate()?;
        Ok(self)
    }

    pub fn persist(&self, path: &Path) -> Result<(), String> {
        let source = read(path)?.unwrap_or_default();
        let before = Config::from_toml(&source)?;
        let mut after = before.clone();
        self.apply(&mut after);
        after.clone().validate()?;
        let text = patch(&source, &before, &after)?;
        if text == source {
            return Ok(());
        }
        write_atomically(path, &text).map_err(|error| error.to_string())
    }
}

impl Config {
    pub fn auto_pause_on(&self) -> bool {
        self.auto_pause_after > 0
    }

    pub fn backend(&self, recommended: ModelPreset) -> Backend<'_> {
        match (&self.remote_url, &self.model_path) {
            (Some(url), _) => Backend::Remote {
                url,
                model: self.remote_model.as_deref(),
            },
            (None, Some(path)) => Backend::File(path),
            (None, None) => Backend::Preset(self.preset(recommended)),
        }
    }

    pub fn same_backend(&self, other: &Self, recommended: ModelPreset) -> bool {
        self.backend(recommended) == other.backend(recommended)
            && self.threads == other.threads
            && self.context_tokens == other.context_tokens
            && self.idle_unload_secs == other.idle_unload_secs
    }

    pub fn app_choice(&self, app: &str) -> Option<AppChoice> {
        match self.access(app) {
            Access::On | Access::OptedIn => Some(AppChoice {
                setting: Setting::Block(app.to_string()),
                off_by_default: false,
            }),
            Access::OffByDefault => Some(AppChoice {
                setting: Setting::Allow(app.to_string()),
                off_by_default: true,
            }),
            Access::Off => None,
        }
    }

    pub fn switch_app(&self, app: &str) -> Option<Setting> {
        self.app_choice(app)
            .map(|choice| choice.setting)
            .or_else(|| {
                self.blocked_apps
                    .iter()
                    .any(|blocked| same_app(blocked, app))
                    .then(|| Setting::Unblock(app.to_string()))
            })
    }

    pub fn block_app(&mut self, app: &str) -> bool {
        match self.access(app) {
            Access::On => {
                self.blocked_apps.push(app_name(app).to_string());
                true
            }
            Access::OptedIn => self.disallow_app(app),
            Access::OffByDefault | Access::Off => false,
        }
    }

    pub fn unblock_app(&mut self, app: &str) -> bool {
        let before = self.blocked_apps.len();
        self.blocked_apps.retain(|blocked| !same_app(blocked, app));
        self.blocked_apps.len() != before
    }

    pub fn allow_app(&mut self, app: &str) -> bool {
        let Some(editor) = editor(app).filter(|_| self.access(app) == Access::OffByDefault) else {
            return false;
        };
        self.allowed_apps.push(editor.to_string());
        true
    }

    pub fn disallow_app(&mut self, app: &str) -> bool {
        let before = self.allowed_apps.len();
        self.allowed_apps.retain(|allowed| !same_app(allowed, app));
        self.allowed_apps.len() != before
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::Chord;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pith-core-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn levels_and_lengths_are_distinct_and_recognised_after_a_toml_round_trip() {
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
        let words = Length::ALL.map(Length::words);
        assert!(words[0] < words[1] && words[1] < words[2]);
        assert_eq!(Length::of(&Config::default()), Some(Length::Normal));
        for length in Length::ALL {
            let mut config = Config::default();
            length.apply(&mut config);
            let reloaded = Config::from_toml(&config.to_toml()).unwrap();
            assert_eq!(Length::of(&reloaded), Some(length), "{length:?}");
            assert_eq!(Suggestions::of(&reloaded), Some(Suggestions::Normal));
        }
        let custom = Config {
            max_words: 9,
            ..Config::default()
        };
        assert_eq!(Length::of(&custom), None);
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
    fn opacity_is_clamped_to_the_range_the_config_accepts() {
        for (asked, stored) in [(0, 10), (9, 10), (10, 10), (55, 55), (100, 100), (255, 100)] {
            let mut config = Config::default();
            Setting::GhostOpacity(asked).apply(&mut config);
            assert_eq!(config.ghost_opacity, stored, "{asked}");
            assert!(Config::from_toml(&config.to_toml()).is_ok(), "{asked}");
        }
    }

    #[test]
    fn blocking_normalises_names_and_never_duplicates() {
        let mut config = Config::default();
        assert!(config.block_app(r"C:\Apps\Slack.EXE"));
        assert_eq!(config.blocked_apps, ["Slack"]);
        for again in ["slack", "Slack.exe", r"D:\x\SLACK.exe"] {
            assert!(!config.block_app(again), "{again}");
        }
        let terminal = crate::apps::sample(crate::apps::Surface::Terminal);
        for always in [terminal, "KeePass.exe", "Code"] {
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
        let terminal = crate::apps::sample(crate::apps::Surface::Terminal);
        assert!(!config.unblock_app(terminal));
        assert!(config.is_blocked(terminal));
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
    fn persisting_never_overwrites_a_file_it_cannot_read() {
        let dir = scratch("persist-invalid");
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        for broken in [
            "max_words = 0",
            "this is = not toml [",
            "accept_keys = [\"Hyper+Tab\"]\n",
        ] {
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
    fn persisting_keeps_comments_layout_and_unknown_keys_and_rewrites_values_in_place() {
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
        let path = dir.join("short.toml");
        std::fs::write(
            &path,
            "# mine
ghost_opacity = 50 # faint
max_words = 6 # short ones
",
        )
        .unwrap();
        Setting::Length(Length::Long).persist(&path).unwrap();
        Setting::GhostOpacity(70).persist(&path).unwrap();
        assert_eq!(
            stored(&path),
            "# mine
ghost_opacity = 70 # faint
max_words = 10 # short ones
max_tokens = 27
"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persisting_a_no_op_leaves_the_file_byte_identical_and_writes_only_changed_keys() {
        let dir = scratch("noop");
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, COMMENTED).unwrap();
        Setting::Block("teams.exe".into()).persist(&path).unwrap();
        Setting::Unblock("nothing".into()).persist(&path).unwrap();
        Setting::Enabled(true).persist(&path).unwrap();
        assert_eq!(stored(&path), COMMENTED);
        let sparse = dir.join("sparse.toml");
        Setting::Enabled(false).persist(&sparse).unwrap();
        assert_eq!(stored(&sparse).trim(), "enabled = false");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persisting_keeps_the_key_names_as_the_user_wrote_them() {
        let dir = scratch("keys");
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        let source = "accept_keys = [\"ctrl + j\"] # mine\nforce_trigger_key = \"f9\"\n";
        std::fs::write(&path, source).unwrap();
        Setting::Enabled(false).persist(&path).unwrap();
        let text = stored(&path);
        assert!(text.contains(source), "{text}");
        let config = Config::from_toml(&text).unwrap();
        assert_eq!(config.accept_keys, [Chord::parse("Ctrl+J").unwrap()]);
        assert_eq!(config.force_trigger_key, Chord::parse("F9").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
