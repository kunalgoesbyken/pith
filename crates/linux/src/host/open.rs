use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use pith_core::{Config, log};

const OPENER: &str = "xdg-open";

fn ensure_config(path: &Path, config: &Config) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    std::fs::write(path, config.to_toml())
}

fn launch(path: PathBuf) {
    std::thread::spawn(move || match Command::new(OPENER).arg(&path).status() {
        Ok(status) if status.success() => {}
        Ok(status) => log(&format!("{OPENER} {}: {status}", path.display())),
        Err(error) => log(&format!("{OPENER} did not start: {error}")),
    });
}

pub fn open_config(path: &Path, config: &Config) {
    match ensure_config(path, config) {
        Ok(()) => launch(path.to_path_buf()),
        Err(error) => log(&format!("settings file not created: {error}")),
    }
}
