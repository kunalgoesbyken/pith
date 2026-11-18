use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::chord::{Chord, Key};
use crate::stored::{read, write_atomically};

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
    fn reach_names_the_server_only_when_text_leaves_the_pc() {
        for (url, expected) in [
            (None, Reach::ThisPc),
            (Some("http://localhost:8080/v1"), Reach::ThisPc),
            (Some("http://127.0.0.1:8080"), Reach::ThisPc),
            (Some("https://[::1]/v1"), Reach::ThisPc),
            (Some("https://LOCALHOST"), Reach::ThisPc),
            (
                Some("https://example.com/v1"),
                Reach::Server("example.com".into()),
            ),
            (
                Some("https://key@api.example.com:8443/v1"),
                Reach::Server("api.example.com".into()),
            ),
            (
                Some("https://[2001:db8::1]/v1"),
                Reach::Server("2001:db8::1".into()),
            ),
            (
                Some("https://localhost.example.com"),
                Reach::Server("localhost.example.com".into()),
            ),
        ] {
            let config = Config {
                remote_url: url.map(str::to_string),
                ..Config::default()
            };
            assert_eq!(config.reach(), expected, "{url:?}");
        }
    }

    #[test]
    fn blocked_apps_extend_the_built_in_list_and_keep_dotted_names() {
        let config = Config {
            blocked_apps: vec!["slack".into(), "Foo.Bar.exe".into()],
            ..Config::default()
        };
        for (app, blocked) in [
            ("slack.exe", true),
            ("KeePass", true),
            ("notepad", false),
            ("Foo.Bar", true),
            (r"C:\x\FOO.BAR.exe", true),
            ("Foo", false),
        ] {
            assert_eq!(config.is_blocked(app), blocked, "{app}");
        }
    }

    #[test]
    fn keys_are_parsed_once_replace_the_defaults_and_round_trip() {
        let defaults = Config::default().to_toml();
        for line in [
            "accept_keys = [\"Tab\"]",
            "\"Shift+Tab\",",
            "\"Ctrl+Right\",",
            "force_trigger_key = \"Ctrl+Shift+Space\"",
        ] {
            assert!(defaults.contains(line), "{line}\n{defaults}");
        }
        let custom = Config::from_toml(
            "accept_keys = [\" ctrl + J \", \"F9\"]
accept_word_keys = [\"alt+right\"]
force_trigger_key = \"WIN+shift+Space\"",
        )
        .unwrap();
        assert_eq!(
            custom.accept_keys,
            [Chord::parse("Ctrl+J").unwrap(), Chord::parse("F9").unwrap()]
        );
        assert_eq!(
            custom.accept_word_keys,
            [Chord::parse("Alt+Right").unwrap()]
        );
        let trigger = Chord::parse("Shift+Win+Space").unwrap();
        assert_eq!(custom.force_trigger_key, trigger);
        let keymap = custom.keymap();
        assert_eq!(keymap.accept_all.len(), 2);
        assert_eq!(keymap.accept_word.len(), 1);
        assert_eq!(keymap.trigger, Some(trigger));
        assert_eq!(Config::from_toml(&custom.to_toml()).unwrap(), custom);
        let off = Config::from_toml("force_trigger = false").unwrap();
        assert_eq!(off.keymap().trigger, None);
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
            ("remote_url = 'http://evil.com\\@localhost'", false),
            ("remote_url = 'http://localhost\\.evil.com'", false),
            ("remote_url = \"http://localhost /v1\"", false),
            ("remote_url = \"https://\"", false),
            ("remote_url = \"https:///v1\"", false),
            ("remote_url = \"https://user@\"", false),
            ("remote_url = \"http://\"", false),
            ("threads = 8", true),
            ("threads = 100000", false),
            ("context_tokens = 4000000000", false),
            ("context_tokens = 8192", true),
            ("remote_url = \"http://127.0.0.1@evil.com\"", false),
            ("model_path = \"\"", true),
            ("model_path = \"Z:/missing/model.gguf\"", false),
            ("context_tokens = 32", false),
            ("min_mean_logprob = nan", false),
            ("min_token_logprob = nan", false),
            ("min_mean_logprob = -inf", true),
            ("ghost_opacity = 35", true),
            ("ghost_opacity = 100", true),
            ("ghost_opacity = 9", false),
            ("ghost_opacity = 101", false),
            ("ghost_opacity = 0", false),
            ("typing_wpm = 1", true),
            ("typing_wpm = 500", true),
            ("typing_wpm = 0", false),
            ("typing_wpm = 501", false),
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
    fn invalid_key_names_are_config_errors() {
        for key in [
            "\"\"",
            "\" \"",
            "\"+\"",
            "\"Ctrl+\"",
            "\"Hyper+Tab\"",
            "\"Ctrl+Enter\"",
            "\"F25\"",
            "\"Ctrl+Alt+j\"",
            "\"j\"",
        ] {
            for toml in [
                format!("accept_keys = [{key}]"),
                format!("accept_keys = [\"Ctrl+k\", {key}]"),
                format!("accept_word_keys = [{key}]"),
                format!("force_trigger_key = {key}"),
            ] {
                assert!(Config::from_toml(&toml).is_err(), "{toml}");
            }
        }
    }

    #[test]
    fn the_error_names_the_bad_key() {
        let error = Config::from_toml("accept_keys = [\"Hyper+Tab\"]").unwrap_err();
        assert!(error.contains("Hyper+Tab"), "{error}");
        assert!(error.contains("accept_keys"), "{error}");
        let error = Config::from_toml("force_trigger_key = \"Ctrl+\u{e9}\"").unwrap_err();
        assert!(error.contains("force_trigger_key"), "{error}");
    }

    #[test]
    fn old_config_files_get_the_default_keys() {
        let old = Config::from_toml(
            "debounce_ms = 200
max_words = 4",
        )
        .unwrap();
        let keymap = old.keymap();
        let defaults = Config::default().keymap();
        assert_eq!(keymap, defaults);
        assert_eq!(old.trigger_mode, TriggerMode::Auto);
        assert_eq!(old.accept_race_ms, 150);
        assert_eq!(keymap.accept_all.len(), 1);
        assert_eq!(keymap.accept_word.len(), 2);
        assert!(keymap.trigger.is_some());
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
pub const MIN_GHOST_OPACITY: u8 = 10;
pub const MAX_GHOST_OPACITY: u8 = 100;
const MAX_ACCEPT_RACE_MS: u64 = 1_000;
const MAX_THREADS: usize = 256;
const MAX_TYPING_WPM: u32 = 500;
const MAX_CONTEXT_TOKENS: u32 = 16_384;

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
    pub pause_on_battery_saver: bool,
    pub debounce_ms: u64,
    pub max_tokens: usize,
    pub max_words: usize,
    #[serde(serialize_with = "shortest_decimal")]
    pub min_token_logprob: f32,
    #[serde(serialize_with = "shortest_decimal")]
    pub min_mean_logprob: f32,
    pub context_chars: usize,
    pub min_chars_before: usize,
    pub auto_pause_after: u32,
    pub smart_space: bool,
    pub trigger_mode: TriggerMode,
    pub accept_keys: Vec<Chord>,
    pub accept_word_keys: Vec<Chord>,
    pub accept_race_ms: u64,
    pub force_trigger: bool,
    pub force_trigger_key: Chord,
    pub blocked_apps: Vec<String>,
    pub ghost_opacity: u8,
    pub remote_url: Option<String>,
    pub remote_model: Option<String>,
    pub typing_wpm: u32,
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
            pause_on_battery_saver: true,
            debounce_ms: 120,
            max_tokens: 16,
            max_words: 6,
            min_token_logprob: -3.5,
            min_mean_logprob: -1.6,
            context_chars: 1200,
            min_chars_before: 3,
            auto_pause_after: 12,
            smart_space: false,
            trigger_mode: TriggerMode::Auto,
            accept_keys: vec![Chord::plain(Key::Tab)],
            accept_word_keys: vec![
                Chord {
                    shift: true,
                    ..Chord::plain(Key::Tab)
                },
                Chord {
                    ctrl: true,
                    ..Chord::plain(Key::Right)
                },
            ],
            accept_race_ms: 150,
            force_trigger: true,
            force_trigger_key: Chord {
                ctrl: true,
                shift: true,
                ..Chord::plain(Key::Space)
            },
            blocked_apps: Vec::new(),
            ghost_opacity: 50,
            remote_url: None,
            remote_model: None,
            typing_wpm: 40,
        }
    }
}

fn shortest_decimal<S: serde::Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    match value.to_string().parse::<f64>() {
        Ok(decimal) => serializer.serialize_f64(decimal),
        Err(_) => serializer.serialize_f32(*value),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Reach {
    #[default]
    ThisPc,
    Server(String),
}

struct Remote<'a> {
    secure: bool,
    host: &'a str,
}

fn remote(url: &str) -> Option<Remote<'_>> {
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return None;
    }
    let (secure, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        (None, None) => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = match host.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => host.split(':').next().unwrap_or_default(),
    };
    (!host.is_empty()).then_some(Remote { secure, host })
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn is_private_remote(url: &str) -> bool {
    remote(url).is_some_and(|remote| remote.secure || is_loopback(remote.host))
}

impl Config {
    pub fn reach(&self) -> Reach {
        match self.remote_url.as_deref().and_then(remote) {
            Some(remote) if !is_loopback(remote.host) => Reach::Server(remote.host.to_string()),
            _ => Reach::ThisPc,
        }
    }

    pub fn from_toml(source: &str) -> Result<Self, String> {
        toml_edit::de::from_str::<Self>(source)
            .map_err(|e| e.to_string())?
            .validate()
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        match read(path)? {
            Some(source) => Self::from_toml(&source),
            None => {
                let config = Self::default();
                let _ = write_atomically(path, &config.to_toml());
                Ok(config)
            }
        }
    }

    pub fn to_toml(&self) -> String {
        toml_edit::ser::to_string_pretty(self).unwrap_or_default()
    }

    pub fn keymap(&self) -> Keymap {
        Keymap {
            accept_all: self.accept_keys.clone(),
            accept_word: self.accept_word_keys.clone(),
            trigger: self.force_trigger.then_some(self.force_trigger_key),
        }
    }

    fn keys_are_distinct(&self) -> bool {
        let keymap = self.keymap();
        let mut taken = vec![Chord::ESCAPE];
        for chord in keymap
            .accept_all
            .iter()
            .chain(&keymap.accept_word)
            .chain(&keymap.trigger)
        {
            if taken.contains(chord) {
                return false;
            }
            taken.push(*chord);
        }
        true
    }

    pub fn validate(mut self) -> Result<Self, String> {
        if !self.keys_are_distinct() {
            return Err("each key can be bound to only one action, and Esc hides".into());
        }
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
            (self.threads <= MAX_THREADS, "threads must be at most 256"),
            (
                self.context_tokens <= MAX_CONTEXT_TOKENS,
                "context_tokens must be at most 16384",
            ),
            (
                self.debounce_ms <= MAX_DEBOUNCE_MS,
                "debounce_ms must be at most 5000",
            ),
            (
                (MIN_GHOST_OPACITY..=MAX_GHOST_OPACITY).contains(&self.ghost_opacity),
                "ghost_opacity must be a percentage from 10 to 100",
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
            (
                (1..=MAX_TYPING_WPM).contains(&self.typing_wpm),
                "typing_wpm must be from 1 to 500",
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
