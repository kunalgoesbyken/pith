use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static SINK: OnceLock<Mutex<File>> = OnceLock::new();
const MAX_LOG_BYTES: u64 = 1 << 20;

pub fn init_log(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let keep = std::fs::metadata(path).is_ok_and(|m| m.len() < MAX_LOG_BYTES);
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(keep)
        .truncate(!keep)
        .open(path);
    if let Ok(file) = file {
        let _ = SINK.set(Mutex::new(file));
    }
}

pub fn log(line: &str) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let entry = format!("{millis} {line}\n");
    eprint!("{entry}");
    if let Some(sink) = SINK.get()
        && let Ok(mut file) = sink.lock()
    {
        let _ = file.write_all(entry.as_bytes());
    }
}
