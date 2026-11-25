use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Default)]
pub struct Held(Arc<AtomicBool>);

impl Held {
    pub(crate) fn begin(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub(crate) fn end(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    pub(crate) fn is_held(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
