use crate::config::app_name;

const MIN_ID_DOTS: usize = 2;
const SNAP_PREFIX: &str = "snap.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    FirstArgument,
    AfterOption(&'static str),
    Unnameable,
}

struct Runtime {
    name: &'static str,
    flags: &'static [&'static str],
    valued: &'static [&'static str],
    inline: &'static [&'static str],
    entry: Entry,
}

const RUNTIMES: &[Runtime] = &[
    Runtime {
        name: "python",
        flags: &[
            "-B", "-b", "-bb", "-d", "-E", "-I", "-O", "-OO", "-P", "-q", "-R", "-s", "-S", "-u",
            "-v", "-x",
        ],
        valued: &["-W", "-X", "-Q"],
        inline: &["-c", "-m"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "perl",
        flags: &["-t", "-T", "-w", "-W", "-X"],
        valued: &["-I", "-M", "-m"],
        inline: &["-e", "-E"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "ruby",
        flags: &["-d", "-v", "-w", "-W"],
        valued: &["-I", "-r"],
        inline: &["-e"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "node",
        flags: &["--no-warnings", "--no-deprecation", "--enable-source-maps"],
        valued: &["-r", "--require", "--import", "--loader"],
        inline: &["-e", "-p", "--eval", "--print"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "nodejs",
        flags: &["--no-warnings", "--no-deprecation", "--enable-source-maps"],
        valued: &["-r", "--require", "--import", "--loader"],
        inline: &["-e", "-p", "--eval", "--print"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "electron",
        flags: &["--no-sandbox", "--disable-gpu"],
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "mono",
        flags: &["--debug", "--server", "--llvm"],
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "mono-sgen",
        flags: &["--debug", "--server", "--llvm"],
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "dotnet",
        flags: &[],
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "java",
        flags: &[],
        valued: &[
            "-cp",
            "-classpath",
            "--class-path",
            "-p",
            "--module-path",
            "--add-modules",
            "--add-opens",
            "--add-exports",
        ],
        inline: &[],
        entry: Entry::AfterOption("-jar"),
    },
    Runtime {
        name: "bash",
        flags: &["-e", "-i", "-l", "-u", "-x"],
        valued: &["-o", "-O"],
        inline: &["-c"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "sh",
        flags: &["-e", "-i", "-l", "-u", "-x"],
        valued: &["-o"],
        inline: &["-c"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "bwrap",
        flags: &[],
        valued: &[],
        inline: &[],
        entry: Entry::Unnameable,
    },
    Runtime {
        name: "flatpak-spawn",
        flags: &[],
        valued: &[],
        inline: &[],
        entry: Entry::Unnameable,
    },
    Runtime {
        name: "xdg-dbus-proxy",
        flags: &[],
        valued: &[],
        inline: &[],
        entry: Entry::Unnameable,
    },
];

const GENERIC_ENTRIES: &[&str] = &[
    "main", "app", "index", "__main__", "run", "start", "server", "cli",
];

const ALIASES: &[(&str, &str)] = &[
    ("org.mozilla.firefox", "firefox"),
    ("org.mozilla.Thunderbird", "thunderbird"),
    ("io.gitlab.librewolf-community", "librewolf"),
    ("org.torproject.torbrowser-launcher", "tor-browser"),
    ("com.github.micahflee.torbrowser-launcher", "tor-browser"),
    ("snap.firefox", "firefox"),
    ("snap.thunderbird", "thunderbird"),
    ("snap.keepassxc", "keepassxc"),
    ("snap.bitwarden", "bitwarden"),
    ("snap.code", "code"),
    ("snap.codium", "codium"),
    ("snap.pycharm-community", "pycharm"),
    ("snap.intellij-idea-community", "idea"),
    ("com.1password.1Password", "1password"),
    ("com.bitwarden.desktop", "bitwarden"),
    ("org.keepassxc.KeePassXC", "keepassxc"),
    ("in.enpass.Enpass", "enpass"),
    ("org.gnome.World.Secrets", "keepass"),
    ("me.proton.Pass", "bitwarden"),
    ("org.gnome.seahorse.Application", "seahorse"),
    ("org.gnome.Console", "kgx"),
    ("org.gnome.Terminal", "gnome-terminal-server"),
    ("org.gnome.Ptyxis", "ptyxis"),
    ("app.devsuite.Ptyxis", "ptyxis"),
    ("org.kde.konsole", "konsole"),
    ("org.kde.yakuake", "yakuake"),
    ("org.wezfurlong.wezterm", "wezterm"),
    ("org.alacritty.Alacritty", "alacritty"),
    ("io.alacritty.Alacritty", "alacritty"),
    ("net.kovidgoyal.kitty", "kitty"),
    ("com.mitchellh.ghostty", "ghostty"),
    ("com.gexperts.Tilix", "tilix"),
    ("org.xfce.Terminal", "xfce4-terminal"),
    ("com.visualstudio.code", "code"),
    ("com.visualstudio.code-oss", "code-oss"),
    ("com.vscodium.codium", "codium"),
    ("dev.zed.Zed", "zed"),
    ("com.jetbrains.IntelliJ-IDEA-Community", "idea"),
    ("com.jetbrains.IntelliJ-IDEA-Ultimate", "idea"),
    ("com.jetbrains.PyCharm-Community", "pycharm"),
    ("com.jetbrains.PyCharm-Professional", "pycharm"),
    ("com.jetbrains.WebStorm", "webstorm"),
    ("com.jetbrains.CLion", "clion"),
    ("com.jetbrains.GoLand", "goland"),
    ("com.jetbrains.Rider", "rider"),
];

fn runtime(exe: &str) -> Option<&'static Runtime> {
    let name = app_name(exe).trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    RUNTIMES
        .iter()
        .find(|runtime| runtime.name.eq_ignore_ascii_case(name))
}

pub fn is_runtime(exe: &str) -> bool {
    runtime(exe).is_some()
}

pub fn entry_of<'a>(exe: &str, arguments: &'a [String]) -> Option<&'a str> {
    let runtime = runtime(exe)?;
    let mut arguments = arguments.iter().map(String::as_str);
    while let Some(argument) = arguments.next() {
        if runtime.inline.contains(&argument) {
            return None;
        }
        if matches!(runtime.entry, Entry::AfterOption(option) if option == argument) {
            return arguments.next();
        }
        if runtime.valued.contains(&argument) {
            arguments.next();
        } else if argument.starts_with('-') {
            if runtime.entry == Entry::FirstArgument && !runtime.flags.contains(&argument) {
                return None;
            }
        } else {
            return (runtime.entry == Entry::FirstArgument).then_some(argument);
        }
    }
    None
}

pub fn is_generic_entry(stem: &str) -> bool {
    GENERIC_ENTRIES
        .iter()
        .any(|generic| generic.eq_ignore_ascii_case(stem))
}

pub fn alias_of(id: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(id))
        .map(|(_, alias)| *alias)
}

pub fn is_unlisted_id(app: &str) -> bool {
    let name = app_name(app);
    (name.starts_with(SNAP_PREFIX) || name.matches('.').count() >= MIN_ID_DOTS)
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && alias_of(name).is_none()
}
