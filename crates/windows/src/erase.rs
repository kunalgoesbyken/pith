use std::path::Path;
use std::time::Duration;

use pith_core::log;
use pith_infer::Worker;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
use windows::core::{PCWSTR, w};

use crate::util::{leftover_notice, remove_contents, wide};

const DELETE_ATTEMPTS: u32 = 8;
const DELETE_RETRY: Duration = Duration::from_millis(250);
const RELEASE_WAIT: Duration = Duration::from_secs(5);

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
