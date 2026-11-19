use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use directories::BaseDirs;

const DIRECTORY: &str = "autostart";
const FILE: &str = "pith.desktop";
const EXEC: &str = "Exec";
const HIDDEN: &str = "Hidden";
const GNOME_ENABLED: &str = "X-GNOME-Autostart-enabled";

#[derive(Clone)]
pub struct Autostart {
    path: PathBuf,
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

fn value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')
        .map(str::trim)
}

fn field<'a>(entry: &'a str, key: &str) -> Option<&'a str> {
    entry.lines().find_map(|line| value(line, key))
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
        Some(Self {
            path,
            command: command_line(executable.to_str()?, arguments),
        })
    }

    pub fn enabled(&self) -> bool {
        matches!(read(&self.path), Ok(Some(entry)) if !disabled(&entry))
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
        if field(&entry, EXEC) == Some(self.command.as_str()) {
            return Ok(());
        }
        let exec = format!("{EXEC}={}", self.command);
        let lines: Vec<&str> = entry
            .lines()
            .map(|line| {
                if value(line, EXEC).is_some() {
                    exec.as_str()
                } else {
                    line
                }
            })
            .collect();
        std::fs::write(&self.path, lines.join("\n") + "\n")
    }

    fn entry(&self) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Pith\nComment=Local autocomplete for every text box\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            self.command
        )
    }
}
