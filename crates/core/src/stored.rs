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
    stored.retain(|kept| wanted.iter().any(|value| same(value, kept)));
    for value in wanted.iter() {
        if !stored.iter().any(|kept| same(kept, value)) {
            stored.push(value.clone());
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

pub fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}
