use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::chord::Chord;

pub fn app_name(path_or_name: &str) -> String {
    let leaf = path_or_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .trim();
    let mut stem = leaf;
    while stem.to_ascii_lowercase().ends_with(".exe") {
        stem = stem[..stem.len() - 4].trim_end();
    }
    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_idempotent() {
        for raw in [
            "Foo.Bar.exe",
            "Foo.Bar",
            r"C:\Apps\Foo.Bar.EXE",
            "code.exe",
            "",
            ".exe",
            "a.exe.exe",
            "foo .exe",
            ".EXE \u{a0}\t.exe",
        ] {
            let once = app_name(raw);
            assert_eq!(app_name(&once), once, "{raw}");
        }
        assert_eq!(app_name(r"C:\Apps\Foo.Bar.EXE"), "Foo.Bar");
        assert_eq!(app_name("foo .exe"), "foo");
    }

    #[test]
    fn user_list_extends_the_built_in_list() {
        let config = Config {
            blocked_apps: vec!["slack".into()],
            ..Config::default()
        };
        assert!(config.is_blocked("slack.exe"));
        assert!(config.is_blocked("KeePass"));
        assert!(!config.is_blocked("notepad"));
    }

    #[test]
    fn validation_rejects_unsafe_or_broken_settings() {
        for (toml, ok) in [
            ("", true),
            ("model = \"lite\"", true),
            ("model = \"huge\"", false),
            ("max_words = 0", false),
            ("debounce_ms = 99999999999", false),
            ("remote_url = \"\"", true),
            ("remote_url = \"https://example.com/v1\"", true),
            ("remote_url = \"http://127.0.0.1:8080\"", true),
            ("remote_url = \"http://[::1]:8080\"", true),
            ("remote_url = \"http://localhost\"", true),
            ("remote_url = \"http://192.168.1.5:8080\"", false),
            ("remote_url = \"http://localhost.evil.com\"", false),
            ("remote_url = \"http://127.0.0.1@evil.com\"", false),
            ("model_path = \"\"", true),
            ("model_path = \"Z:/missing/model.gguf\"", false),
            ("context_tokens = 32", false),
            ("min_mean_logprob = nan", false),
            ("min_token_logprob = nan", false),
            ("min_mean_logprob = -inf", true),
            ("accept_race_ms = 0", true),
            ("accept_race_ms = 1001", false),
            ("trigger_mode = \"manual\"", true),
            ("trigger_mode = \"manual\"\nforce_trigger = false", false),
            ("trigger_mode = \"sometimes\"", false),
            ("accept_keys = [\"Right\", \"Tab\"]", true),
            ("accept_keys = []", true),
            ("accept_keys = [\"Tab\", \"Tab\"]", false),
            ("accept_keys = [\"Shift+Tab\"]", false),
            ("accept_keys = [\"Esc\"]", false),
            ("accept_keys = [\"a\"]", false),
            ("accept_keys = [\"Hyper+Tab\"]", false),
            ("accept_word_keys = [\"Tab\"]", false),
            (
                "accept_keys = [\"Right\"]\naccept_word_keys = [\"Tab\"]",
                true,
            ),
            ("force_trigger_key = \"Tab\"", false),
            ("force_trigger_key = \"Ctrl+Right\"", false),
            ("force_trigger = false\nforce_trigger_key = \"Tab\"", true),
            ("force_trigger_key = \"Ctrl+Alt+Space\"", false),
        ] {
            assert_eq!(Config::from_toml(toml).is_ok(), ok, "{toml}");
        }
        assert_eq!(
            Config::from_toml("model_path = \"\"").unwrap().model_path,
            None
        );
    }

    #[test]
    fn old_config_files_get_the_default_keys() {
        let old = Config::from_toml(
            "debounce_ms = 200
max_words = 4",
        )
        .unwrap();
        let keymap = old.keymap().unwrap();
        let defaults = Config::default().keymap().unwrap();
        assert_eq!(keymap, defaults);
        assert_eq!(old.trigger_mode, TriggerMode::Auto);
        assert_eq!(old.accept_race_ms, 150);
        assert_eq!(keymap.accept_all.len(), 1);
        assert_eq!(keymap.accept_word.len(), 2);
        assert!(keymap.trigger.is_some());
    }

    #[test]
    fn custom_keys_replace_the_defaults() {
        let config = Config::from_toml(
            "accept_keys = [\"Right\", \"Ctrl+j\"]
accept_word_keys = [\"Alt+Right\"]
force_trigger_key = \"F9\"",
        )
        .unwrap();
        let keymap = config.keymap().unwrap();
        assert_eq!(keymap.accept_all.len(), 2);
        assert_eq!(keymap.accept_word.len(), 1);
        assert_eq!(keymap.trigger, Some(Chord::parse("F9").unwrap()));
        let off = Config::from_toml("force_trigger = false").unwrap();
        assert_eq!(off.keymap().unwrap().trigger, None);
    }

    #[test]
    fn dotted_names_stay_blocked() {
        let config = Config {
            blocked_apps: vec!["Foo.Bar.exe".into()],
            ..Config::default()
        };
        assert!(config.is_blocked("Foo.Bar"));
        assert!(config.is_blocked(&app_name(r"C:\x\FOO.BAR.exe")));
        assert!(!config.is_blocked("Foo"));
    }
}

const ALWAYS_BLOCKED: [&str; 23] = [
    "WindowsTerminal",
    "cmd",
    "powershell",
    "pwsh",
    "conhost",
    "wt",
    "Code",
    "Cursor",
    "devenv",
    "idea64",
    "pycharm64",
    "rider64",
    "zed",
    "1Password",
    "KeePass",
    "KeePassXC",
    "Bitwarden",
    "Dashlane",
    "LastPass",
    "LockApp",
    "LogonUI",
    "consent",
    "CredentialUIBroker",
];

const MIN_CONTEXT_HEADROOM: u32 = 64;
const LOOKBACK_WINDOWS: usize = 16;
const MAX_DEBOUNCE_MS: u64 = 5_000;
const MAX_ACCEPT_RACE_MS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelPreset {
    #[default]
    Standard,
    Lite,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TriggerMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keymap {
    pub accept_all: Vec<Chord>,
    pub accept_word: Vec<Chord>,
    pub trigger: Option<Chord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub model: ModelPreset,
    pub model_path: Option<PathBuf>,
    pub threads: usize,
    pub context_tokens: u32,
    pub idle_unload_secs: u64,
    pub debounce_ms: u64,
    pub max_tokens: usize,
    pub max_words: usize,
    pub min_token_logprob: f32,
    pub min_mean_logprob: f32,
    pub context_chars: usize,
    pub min_chars_before: usize,
    pub trigger_mode: TriggerMode,
    pub accept_keys: Vec<String>,
    pub accept_word_keys: Vec<String>,
    pub accept_race_ms: u64,
    pub force_trigger: bool,
    pub force_trigger_key: String,
    pub blocked_apps: Vec<String>,
    pub remote_url: Option<String>,
    pub remote_model: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            model: ModelPreset::Standard,
            model_path: None,
            threads: 0,
            context_tokens: 1024,
            idle_unload_secs: 300,
            debounce_ms: 120,
            max_tokens: 16,
            max_words: 6,
            min_token_logprob: -3.5,
            min_mean_logprob: -1.6,
            context_chars: 1200,
            min_chars_before: 3,
            trigger_mode: TriggerMode::Auto,
            accept_keys: vec!["Tab".into()],
            accept_word_keys: vec!["Shift+Tab".into(), "Ctrl+Right".into()],
            accept_race_ms: 150,
            force_trigger: true,
            force_trigger_key: "Ctrl+Shift+Space".into(),
            blocked_apps: Vec::new(),
            remote_url: None,
            remote_model: None,
        }
    }
}

fn is_private_remote(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = match host.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => host.split(':').next().unwrap_or_default(),
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

impl Config {
    pub fn from_toml(source: &str) -> Result<Self, String> {
        toml::from_str::<Self>(source)
            .map_err(|e| e.to_string())?
            .validate()
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn keymap(&self) -> Result<Keymap, String> {
        let parse_all = |keys: &[String]| {
            keys.iter()
                .map(|k| Chord::parse(k))
                .collect::<Result<Vec<_>, _>>()
        };
        let keymap = Keymap {
            accept_all: parse_all(&self.accept_keys)?,
            accept_word: parse_all(&self.accept_word_keys)?,
            trigger: self
                .force_trigger
                .then(|| Chord::parse(&self.force_trigger_key))
                .transpose()?,
        };
        let mut taken = vec![Chord::ESCAPE];
        for chord in keymap
            .accept_all
            .iter()
            .chain(&keymap.accept_word)
            .chain(&keymap.trigger)
        {
            if taken.contains(chord) {
                return Err("each key can be bound to only one action, and Esc hides".into());
            }
            taken.push(*chord);
        }
        Ok(keymap)
    }

    pub fn validate(mut self) -> Result<Self, String> {
        self.keymap()?;
        self.model_path = self.model_path.filter(|p| !p.as_os_str().is_empty());
        self.remote_url = self
            .remote_url
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty());
        let rules = [
            (self.max_words >= 1, "max_words must be at least 1"),
            (self.max_tokens >= 1, "max_tokens must be at least 1"),
            (self.context_chars >= 1, "context_chars must be at least 1"),
            (
                self.context_tokens as usize >= self.max_tokens + MIN_CONTEXT_HEADROOM as usize,
                "context_tokens must exceed max_tokens by at least 64",
            ),
            (
                !self.min_token_logprob.is_nan() && !self.min_mean_logprob.is_nan(),
                "logprob thresholds must be numbers",
            ),
            (
                self.debounce_ms <= MAX_DEBOUNCE_MS,
                "debounce_ms must be at most 5000",
            ),
            (
                self.accept_race_ms <= MAX_ACCEPT_RACE_MS,
                "accept_race_ms must be at most 1000",
            ),
            (
                self.trigger_mode == TriggerMode::Auto || self.force_trigger,
                "trigger_mode manual needs force_trigger",
            ),
            (
                self.remote_url.as_deref().is_none_or(is_private_remote),
                "remote_url must use https, or http to this computer",
            ),
            (
                self.model_path.as_deref().is_none_or(Path::is_file),
                "model_path does not point to a file",
            ),
        ];
        match rules.into_iter().find(|(ok, _)| !ok) {
            Some((_, message)) => Err(message.to_string()),
            None => Ok(self),
        }
    }

    pub fn resolved_threads(&self, available: usize) -> usize {
        if self.threads > 0 {
            self.threads
        } else {
            (available / 2).clamp(1, 4)
        }
    }

    pub fn lookback_chars(&self) -> usize {
        self.context_chars.saturating_mul(LOOKBACK_WINDOWS)
    }

    pub fn is_blocked(&self, app: &str) -> bool {
        let app = app_name(app);
        if app.is_empty() {
            return true;
        }
        ALWAYS_BLOCKED
            .iter()
            .copied()
            .chain(self.blocked_apps.iter().map(String::as_str))
            .any(|blocked| app_name(blocked).eq_ignore_ascii_case(&app))
    }
}
