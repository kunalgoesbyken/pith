use crate::config::app_name;
use crate::hosts::alias_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Secure,
    Terminal,
    Editor,
}

type Names = [(Surface, &'static [&'static str]); 3];

#[cfg(windows)]
const WINDOWS: Names = [
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

#[cfg(target_os = "linux")]
const LINUX: Names = [
    (
        Surface::Secure,
        &[
            "1password",
            "keepass",
            "keepass2",
            "keepassx",
            "keepassxc",
            "bitwarden",
            "enpass",
            "dashlane",
            "lastpass",
            "gcr-prompter",
            "seahorse",
            "pinentry",
            "pinentry-curses",
            "pinentry-emacs",
            "pinentry-gnome3",
            "pinentry-gtk",
            "pinentry-gtk-2",
            "pinentry-gtk2",
            "pinentry-qt",
            "pinentry-qt4",
            "pinentry-qt5",
            "pinentry-tty",
            "ssh-askpass",
            "ksshaskpass",
            "lxqt-openssh-askpass",
            "polkit-gnome-authentication-agent-1",
            "polkit-kde-authentication-agent-1",
            "polkit-mate-authentication-agent-1",
            "lxqt-policykit-agent",
            "xfce-polkit",
            "hyprpolkitagent",
            "swaylock",
            "i3lock",
            "hyprlock",
            "gtklock",
            "xsecurelock",
            "xscreensaver",
        ],
    ),
    (
        Surface::Terminal,
        &[
            "alacritty",
            "contour",
            "cool-retro-term",
            "foot",
            "footclient",
            "ghostty",
            "gnome-console",
            "gnome-terminal-server",
            "guake",
            "kgx",
            "kitty",
            "konsole",
            "lxterminal",
            "mate-terminal",
            "ptyxis",
            "qterminal",
            "rxvt",
            "sakura",
            "st",
            "terminator",
            "terminology",
            "tilda",
            "tilix",
            "urxvt",
            "uxterm",
            "wezterm",
            "wezterm-gui",
            "xfce4-terminal",
            "xterm",
            "yakuake",
        ],
    ),
    (
        Surface::Editor,
        &[
            "code",
            "code-oss",
            "codium",
            "vscodium",
            "cursor",
            "windsurf",
            "zed",
            "zeditor",
            "zed-editor",
            "idea",
            "pycharm",
            "webstorm",
            "clion",
            "goland",
            "rider",
            "rubymine",
            "phpstorm",
            "datagrip",
            "studio",
        ],
    ),
];

#[cfg(not(any(windows, target_os = "linux")))]
const UNSUPPORTED: Names = [
    (Surface::Secure, &[]),
    (Surface::Terminal, &[]),
    (Surface::Editor, &[]),
];

#[cfg(windows)]
fn surfaces() -> &'static Names {
    &WINDOWS
}

#[cfg(target_os = "linux")]
fn surfaces() -> &'static Names {
    &LINUX
}

#[cfg(not(any(windows, target_os = "linux")))]
fn surfaces() -> &'static Names {
    &UNSUPPORTED
}

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
    canonical(a).eq_ignore_ascii_case(canonical(b))
}

fn canonical(app: &str) -> &str {
    let name = app_name(app);
    alias_of(name).unwrap_or(name)
}

pub fn surface(app: &str) -> Option<(Surface, &'static str)> {
    surfaces().iter().find_map(|(surface, names)| {
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

#[cfg(test)]
pub(crate) fn sample(wanted: Surface) -> &'static str {
    surfaces()
        .iter()
        .find(|(surface, _)| *surface == wanted)
        .and_then(|(_, names)| names.first())
        .copied()
        .unwrap_or_default()
}
