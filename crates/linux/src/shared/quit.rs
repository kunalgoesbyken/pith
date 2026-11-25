use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
