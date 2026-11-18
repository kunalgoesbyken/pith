use std::path::Path;

use serde::{Deserialize, Serialize};

pub fn app_name(path_or_name: &str) -> String {
    let leaf = path_or_name.rsplit(['\\', '/']).next().unwrap_or_default();
    Path::new(leaf)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub model_path: String,
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
    pub blocked_apps: Vec<String>,
    pub remote_url: Option<String>,
    pub remote_model: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            model_path: String::new(),
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
            blocked_apps: [
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
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            remote_url: None,
            remote_model: None,
        }
    }
}

impl Config {
    pub fn from_toml(source: &str) -> Result<Self, String> {
        toml::from_str(source).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn resolved_threads(&self, available: usize) -> usize {
        if self.threads > 0 {
            self.threads
        } else {
            (available / 2).clamp(1, 4)
        }
    }

    pub fn is_blocked(&self, app: &str) -> bool {
        let app = app_name(app);
        if app.is_empty() {
            return true;
        }
        self.blocked_apps
            .iter()
            .any(|blocked| app_name(blocked).eq_ignore_ascii_case(&app))
    }
}
