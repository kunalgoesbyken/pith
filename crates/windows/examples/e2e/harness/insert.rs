use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use pith_windows::{Trigger, paste};
use windows::Win32::UI::Controls::EM_REPLACESEL;

use crate::common::wide;
use crate::harness::edit::send;

pub(crate) static TARGET: AtomicIsize = AtomicIsize::new(0);
pub(crate) static THROUGH_CLIPBOARD: AtomicBool = AtomicBool::new(false);

pub(crate) fn insert_now(text: String) {
    let hwnd = TARGET.load(Ordering::SeqCst);
    if THROUGH_CLIPBOARD.load(Ordering::SeqCst) {
        paste(text, Trigger::Message(hwnd));
    } else {
        replace_selection(hwnd, &text);
    }
}

fn replace_selection(edit: isize, text: &str) {
    let text = wide(text);
    send(edit, EM_REPLACESEL, 1, text.as_ptr() as isize);
}

pub(crate) fn insert_after(text: String, lag: Duration) {
    static QUEUE: OnceLock<Mutex<Sender<(isize, String, Instant)>>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = channel::<(isize, String, Instant)>();
        std::thread::spawn(move || {
            while let Ok((hwnd, text, due)) = receiver.recv() {
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                replace_selection(hwnd, &text);
            }
        });
        Mutex::new(sender)
    });
    if let Ok(sender) = queue.lock() {
        let _ = sender.send((TARGET.load(Ordering::SeqCst), text, Instant::now() + lag));
    }
}
