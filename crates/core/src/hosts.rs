use crate::config::app_name;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    FirstArgument,
    AfterOption(&'static str),
    Unnameable,
}

struct Runtime {
    name: &'static str,
    valued: &'static [&'static str],
    inline: &'static [&'static str],
    entry: Entry,
}

const RUNTIMES: &[Runtime] = &[
    Runtime {
        name: "python",
        valued: &["-W", "-X", "-Q"],
        inline: &["-c", "-m"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "perl",
        valued: &["-I", "-M", "-m"],
        inline: &["-e", "-E"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "ruby",
        valued: &["-I", "-r"],
        inline: &["-e"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "node",
        valued: &["-r", "--require", "--import", "--loader"],
        inline: &["-e", "-p", "--eval", "--print"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "nodejs",
        valued: &["-r", "--require", "--import", "--loader"],
        inline: &["-e", "-p", "--eval", "--print"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "electron",
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "mono",
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "mono-sgen",
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "dotnet",
        valued: &[],
        inline: &[],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "java",
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
        valued: &["-o", "-O"],
        inline: &["-c"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "sh",
        valued: &["-o"],
        inline: &["-c"],
        entry: Entry::FirstArgument,
    },
    Runtime {
        name: "bwrap",
        valued: &[],
        inline: &[],
        entry: Entry::Unnameable,
    },
    Runtime {
        name: "flatpak-spawn",
        valued: &[],
        inline: &[],
        entry: Entry::Unnameable,
    },
    Runtime {
        name: "xdg-dbus-proxy",
        valued: &[],
        inline: &[],
        entry: Entry::Unnameable,
    },
];

const ALIASES: &[(&str, &str)] = &[
    ("com.1password.1Password", "1password"),
    ("com.bitwarden.desktop", "bitwarden"),
    ("org.keepassxc.KeePassXC", "keepassxc"),
    ("in.enpass.Enpass", "enpass"),
    ("org.gnome.seahorse.Application", "seahorse"),
    ("org.gnome.Console", "kgx"),
    ("org.gnome.Terminal", "gnome-terminal-server"),
    ("org.kde.konsole", "konsole"),
    ("org.kde.yakuake", "yakuake"),
    ("org.wezfurlong.wezterm", "wezterm"),
    ("org.alacritty.Alacritty", "alacritty"),
    ("net.kovidgoyal.kitty", "kitty"),
    ("com.mitchellh.ghostty", "ghostty"),
    ("app.devsuite.Ptyxis", "ptyxis"),
    ("com.gexperts.Tilix", "tilix"),
    ("org.xfce.Terminal", "xfce4-terminal"),
    ("com.visualstudio.code", "code"),
    ("com.vscodium.codium", "codium"),
    ("dev.zed.Zed", "zed"),
    ("com.jetbrains.IntelliJ-IDEA-Community", "idea"),
    ("com.jetbrains.IntelliJ-IDEA-Ultimate", "idea"),
    ("com.jetbrains.PyCharm-Community", "pycharm"),
    ("com.jetbrains.PyCharm-Professional", "pycharm"),
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
        } else if !argument.starts_with('-') {
            return (runtime.entry == Entry::FirstArgument).then_some(argument);
        }
    }
    None
}

pub fn alias_of(id: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(id))
        .map(|(_, alias)| *alias)
}
