use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const SESSION_TYPE: &str = "XDG_SESSION_TYPE";
const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";
const WAYLAND: &str = "wayland";

pub fn on_wayland() -> bool {
    std::env::var_os(WAYLAND_DISPLAY).is_some_and(|name| !name.is_empty())
        || std::env::var(SESSION_TYPE).is_ok_and(|kind| kind.eq_ignore_ascii_case(WAYLAND))
}

#[derive(Clone, Default)]
pub struct Quit(Arc<AtomicBool>);

impl Quit {
    pub(crate) fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
