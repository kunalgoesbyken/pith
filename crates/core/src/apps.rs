use crate::config::app_name;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Secure,
    Terminal,
    Editor,
}

const SURFACES: [(Surface, &[&str]); 3] = [
    (
        Surface::Secure,
        &[
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
        ],
    ),
    (
        Surface::Terminal,
        &[
            "WindowsTerminal",
            "cmd",
            "powershell",
            "pwsh",
            "conhost",
            "wt",
        ],
    ),
    (
        Surface::Editor,
        &[
            "Code",
            "Cursor",
            "devenv",
            "idea64",
            "pycharm64",
            "rider64",
            "zed",
        ],
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    On,
    OptedIn,
    OffByDefault,
    Off,
}

impl Access {
    pub fn is_blocked(self) -> bool {
        matches!(self, Self::OffByDefault | Self::Off)
    }
}

pub fn same_app(a: &str, b: &str) -> bool {
    app_name(a).eq_ignore_ascii_case(app_name(b))
}

pub fn surface(app: &str) -> Option<(Surface, &'static str)> {
    SURFACES.iter().find_map(|(surface, names)| {
        names
            .iter()
            .find(|name| same_app(name, app))
            .map(|name| (*surface, *name))
    })
}

pub fn editor(app: &str) -> Option<&'static str> {
    surface(app).and_then(|(surface, name)| (surface == Surface::Editor).then_some(name))
}

pub fn editors_among(apps: &[String]) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for name in apps.iter().filter_map(|app| editor(app)) {
        if !kept.iter().any(|known| known == name) {
            kept.push(name.to_string());
        }
    }
    kept
}
