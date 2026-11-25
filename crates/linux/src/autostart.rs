use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use pith_core::log;

const HEADER: &str = "[Desktop Entry]";
const DIRECTORY: &str = "autostart";
const FILE: &str = "pith.desktop";
const EXEC: &str = "Exec";
const HIDDEN: &str = "Hidden";
const GNOME_ENABLED: &str = "X-GNOME-Autostart-enabled";

#[derive(Clone)]
pub struct Autostart {
    path: PathBuf,
    executable: String,
    command: String,
}

fn quoted(argument: &str) -> String {
    let mut quoted = String::from("\"");
    for character in argument.chars() {
        match character {
            '\\' => quoted.push_str(r"\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str(r"\\");
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

fn command_line(executable: &str, arguments: &[&str]) -> String {
    std::iter::once(executable)
        .chain(arguments.iter().copied())
        .map(quoted)
        .collect::<Vec<_>>()
        .join(" ")
}

fn unquoted(text: &str) -> Option<(String, &str)> {
    let mut characters = text.strip_prefix('"')?.char_indices();
    let mut value = String::new();
    while let Some((end, character)) = characters.next() {
        match character {
            '"' => return Some((value, &text[end + 2..])),
            '%' => {
                characters.next()?;
                value.push('%');
            }
            '\\' => {
                characters.next()?;
                let (_, escaped) = characters.next()?;
                if escaped == '\\' {
                    characters.next()?;
                }
                value.push(escaped);
            }
            other => value.push(other),
        }
    }
    None
}

fn value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')
        .map(str::trim)
}

fn group(entry: &str) -> impl Iterator<Item = (usize, &str)> {
    entry
        .lines()
        .enumerate()
        .skip_while(|(_, line)| line.trim() != HEADER)
        .skip(1)
        .take_while(|(_, line)| !line.trim_start().starts_with('['))
}

fn exec_line(entry: &str) -> Option<(usize, &str)> {
    group(entry).find_map(|(index, line)| Some((index, value(line, EXEC)?)))
}

fn field<'a>(entry: &'a str, key: &str) -> Option<&'a str> {
    group(entry).find_map(|(_, line)| value(line, key))
}

fn disabled(entry: &str) -> bool {
    field(entry, HIDDEN) == Some("true") || field(entry, GNOME_ENABLED) == Some("false")
}

fn read(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

impl Autostart {
    pub fn new(arguments: &[&str]) -> Option<Self> {
        let path = BaseDirs::new()?.config_dir().join(DIRECTORY).join(FILE);
        let executable = std::env::current_exe().ok()?;
        let Some(executable) = executable.to_str() else {
            log("start at login is off: the path of this program is not valid text");
            return None;
        };
        Some(Self {
            path,
            executable: executable.to_string(),
            command: command_line(executable, arguments),
        })
    }

    pub fn enabled(&self) -> io::Result<bool> {
        Ok(read(&self.path)?.is_some_and(|entry| !disabled(&entry)))
    }

    pub fn set(&self, enabled: bool) -> io::Result<()> {
        if !enabled {
            return match std::fs::remove_file(&self.path) {
                Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        if let Some(directory) = self.path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        std::fs::write(&self.path, self.entry())
    }

    pub fn refresh(&self) -> io::Result<()> {
        let Some(entry) = read(&self.path)? else {
            return Ok(());
        };
        let Some((index, exec)) = exec_line(&entry) else {
            return Ok(());
        };
        let Some((previous, arguments)) = unquoted(exec) else {
            return Ok(());
        };
        let previous = Path::new(&previous);
        if !previous.is_absolute() || previous.exists() {
            return Ok(());
        }
        let mut lines: Vec<String> = entry.lines().map(str::to_string).collect();
        lines[index] = format!("{EXEC}={}{arguments}", quoted(&self.executable));
        std::fs::write(&self.path, lines.join("\n") + "\n")
    }

    fn entry(&self) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Pith\nComment=Local autocomplete for every text box\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            self.command
        )
    }
}
