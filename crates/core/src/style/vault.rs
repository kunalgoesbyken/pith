use std::path::{Path, PathBuf};

use super::memory::{Sample, StyleMemory};
use crate::Config;
use crate::config::stored::{aside_path, set_aside, write_private};

pub trait Vault: Send {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String>;
    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, String>;
}

pub struct PlainVault;

impl Vault for PlainVault {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
        Ok(plain.to_vec())
    }

    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, String> {
        Ok(sealed.to_vec())
    }
}

pub struct StyleStore {
    path: PathBuf,
    vault: Box<dyn Vault>,
    memory: StyleMemory,
}

fn unseal(vault: &dyn Vault, bytes: &[u8]) -> Result<StyleMemory, String> {
    let plain = vault.open(bytes)?;
    let source = std::str::from_utf8(&plain).map_err(|error| error.to_string())?;
    StyleMemory::parse(source)
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

impl StyleStore {
    pub fn open(path: PathBuf, vault: Box<dyn Vault>) -> Result<Self, String> {
        let memory = match std::fs::read(&path) {
            Ok(bytes) => match unseal(vault.as_ref(), &bytes) {
                Ok(memory) => memory,
                Err(_) => {
                    set_aside(&path)?;
                    StyleMemory::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StyleMemory::default(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            path,
            vault,
            memory,
        })
    }

    pub fn memory(&self) -> &StyleMemory {
        &self.memory
    }

    pub fn learn(&mut self, sample: Sample) -> Result<(), String> {
        self.update(|memory| {
            memory.learn(sample);
            true
        })
    }

    pub fn forget(&mut self, app: &str, text: &str) -> Result<(), String> {
        self.update(|memory| memory.forget(app, text))
    }

    pub fn forget_blocked(&mut self, config: &Config) -> Result<bool, String> {
        let before = self.memory.entries().len();
        self.update(|memory| memory.forget_blocked(config))?;
        Ok(self.memory.entries().len() != before)
    }

    pub fn erase(&mut self) -> Result<(), String> {
        let kept = remove_if_present(&aside_path(&self.path));
        remove_if_present(&self.path).and(kept)?;
        self.memory.clear();
        Ok(())
    }

    fn update(&mut self, change: impl FnOnce(&mut StyleMemory) -> bool) -> Result<(), String> {
        let mut memory = self.memory.clone();
        if !change(&mut memory) {
            return Ok(());
        }
        let sealed = self.vault.seal(memory.render()?.as_bytes())?;
        write_private(&self.path, &sealed).map_err(|error| error.to_string())?;
        self.memory = memory;
        Ok(())
    }
}
