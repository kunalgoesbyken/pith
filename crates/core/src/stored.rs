use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::Config;

fn describe(value: &Value) -> String {
    value.clone().decorated("", "").to_string()
}

fn same(a: &Value, b: &Value) -> bool {
    match (a.as_str(), b.as_str()) {
        (Some(a), Some(b)) => a == b,
        _ => describe(a) == describe(b),
    }
}

fn merge_array(stored: &mut Array, wanted: &Array) {
    let merged: Vec<(Value, bool)> = wanted
        .iter()
        .map(|value| match stored.iter().find(|kept| same(kept, value)) {
            Some(kept) => (kept.clone(), true),
            None => (value.clone(), false),
        })
        .collect();
    stored.clear();
    for (value, kept) in merged {
        if kept {
            stored.push_formatted(value);
        } else {
            stored.push(value);
        }
    }
}

fn write(table: &mut Table, key: &str, wanted: &Item) {
    if let (Some(Item::Value(stored)), Item::Value(value)) = (table.get_mut(key), wanted) {
        if let (Value::Array(stored), Value::Array(value)) = (&mut *stored, value) {
            merge_array(stored, value);
            return;
        }
        let decor = stored.decor().clone();
        *stored = value.clone();
        *stored.decor_mut() = decor;
        return;
    }
    table.insert(key, wanted.clone());
}

fn serialized(config: &Config) -> Result<DocumentMut, String> {
    toml_edit::ser::to_document(config).map_err(|error| error.to_string())
}

pub fn patch(source: &str, before: &Config, after: &Config) -> Result<String, String> {
    let mut document: DocumentMut = source.parse().map_err(|error| format!("{error}"))?;
    let (old, new) = (serialized(before)?, serialized(after)?);
    for (key, item) in new.iter() {
        let unchanged = matches!(
            (old.get(key), item),
            (Some(Item::Value(a)), Item::Value(b)) if same(a, b)
        );
        if !unchanged {
            write(document.as_table_mut(), key, item);
        }
    }
    for (key, _) in old.iter().filter(|(key, _)| new.get(key).is_none()) {
        document.remove(key);
    }
    Ok(document.to_string())
}

pub fn read(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut named = path.as_os_str().to_owned();
    named.push(suffix);
    PathBuf::from(named)
}

pub(crate) fn aside_path(path: &Path) -> PathBuf {
    beside(path, ".bad")
}

pub(crate) fn set_aside(path: &Path) -> Result<(), String> {
    std::fs::rename(path, aside_path(path)).map_err(|error| error.to_string())
}

fn publish(temporary: &Path, path: &Path) -> io::Result<()> {
    std::fs::rename(temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(temporary);
    })
}

pub fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = beside(path, ".tmp");
    std::fs::write(&temporary, text)?;
    publish(&temporary, path)
}

#[cfg(unix)]
fn private_directory(directory: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn private_directory(directory: &Path) -> io::Result<()> {
    std::fs::create_dir_all(directory)
}

#[cfg(unix)]
fn private_file(path: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> io::Result<std::fs::File> {
    std::fs::File::create(path)
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        private_directory(parent)?;
    }
    let temporary = beside(path, ".tmp");
    private_file(&temporary)?.write_all(bytes)?;
    publish(&temporary, path)
}
