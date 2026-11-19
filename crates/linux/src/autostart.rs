use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use directories::BaseDirs;

const DIRECTORY: &str = "autostart";
const FILE: &str = "pith.desktop";

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

fn command_line(executable: &Path, arguments: &[&str]) -> String {
    std::iter::once(executable.to_string_lossy().into_owned())
        .chain(arguments.iter().map(|argument| (*argument).to_string()))
        .map(|argument| quoted(&argument))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Autostart {
    pub fn new(arguments: &[&str]) -> Option<Self> {
        let path = BaseDirs::new()?.config_dir().join(DIRECTORY).join(FILE);
        let executable = std::env::current_exe().ok()?;
        Some(Self {
            path,
            command: command_line(&executable, arguments),
        })
    }

    pub fn enabled(&self) -> bool {
        self.path.is_file()
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
        if self.enabled() {
            self.set(true)
        } else {
            Ok(())
        }
    }

    fn entry(&self) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=Pith\nComment=Local autocomplete for every text box\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            self.command
        )
    }
}
