use std::path::{Path, PathBuf};
use std::time::Duration;

use pith_core::log;
use pith_infer::Worker;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
use windows::core::{PCWSTR, w};

use crate::util::wide;

const DELETE_ATTEMPTS: u32 = 8;
const DELETE_RETRY: Duration = Duration::from_millis(250);
const RELEASE_WAIT: Duration = Duration::from_secs(5);
const LISTED_LEFTOVERS: usize = 5;

pub fn erase(dir: &Path, worker: Worker) {
    let released = worker.shutdown(RELEASE_WAIT);
    let left = remove_contents(dir, DELETE_ATTEMPTS, DELETE_RETRY);
    log(&format!(
        "data removed, model released {released}, {} item(s) left",
        left.len()
    ));
    if let Some(notice) = leftover_notice(dir, &left) {
        let text = wide(&notice);
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                w!("Pith"),
                MB_OK | MB_ICONWARNING,
            );
        }
    }
}

fn remove_contents(dir: &Path, attempts: u32, delay: Duration) -> Vec<PathBuf> {
    let mut remaining = leftovers(dir);
    for _ in 1..attempts {
        if remaining.is_empty() {
            break;
        }
        std::thread::sleep(delay);
        remaining = leftovers(dir);
    }
    remaining
}

fn leftover_notice(dir: &Path, left: &[PathBuf]) -> Option<String> {
    if left.is_empty() {
        return None;
    }
    let names: Vec<String> = left
        .iter()
        .take(LISTED_LEFTOVERS)
        .map(|path| {
            path.strip_prefix(dir)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let more = left.len().saturating_sub(LISTED_LEFTOVERS);
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "Pith could not delete everything. {} item(s) are still in use by another program and remain in {}: {}{}. Delete them yourself once that program has closed.",
        left.len(),
        dir.display(),
        names.join(", "),
        tail
    ))
}

fn leftovers(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut failed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let removed = match entry.file_type() {
            Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(&path),
            _ => std::fs::remove_file(&path),
        };
        if removed.is_err() {
            failed.push(path);
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pith-win-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn removes_nested_contents_keeps_the_folder_and_ignores_a_missing_one() {
        let dir = scratch("nested");
        std::fs::create_dir_all(dir.join("models").join("deep")).unwrap();
        std::fs::write(dir.join("models").join("deep").join("a.gguf"), "x").unwrap();
        std::fs::write(dir.join("pith.log"), "x").unwrap();
        assert!(remove_contents(&dir, 1, Duration::ZERO).is_empty());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let absent = dir.join("absent");
        assert!(remove_contents(&absent, 3, Duration::ZERO).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_notice_names_what_is_left_relative_to_the_folder_and_caps_the_list() {
        let dir = PathBuf::from("data");
        let left: Vec<PathBuf> = (0..7)
            .map(|n| dir.join("models").join(format!("m{n}.gguf")))
            .collect();
        let notice = leftover_notice(&dir, &left).unwrap();
        assert!(notice.contains("7 item(s)"), "{notice}");
        let first = Path::new("models").join("m0.gguf");
        assert!(notice.contains(&*first.to_string_lossy()), "{notice}");
        assert!(notice.contains(" and 2 more"), "{notice}");
        assert!(!notice.contains("m5.gguf"), "{notice}");
    }

    #[test]
    fn a_locked_file_is_reported_while_the_rest_goes() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("locked");
        std::fs::write(dir.join("other.txt"), "x").unwrap();
        let locked = dir.join("locked.log");
        let _held = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(0)
            .open(&locked)
            .unwrap();
        assert_eq!(remove_contents(&dir, 2, Duration::from_millis(1)), [locked]);
        assert!(!dir.join("other.txt").exists());
        drop(_held);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
