use std::sync::{Arc, Mutex, PoisonError};

use crate::error::Target;

#[derive(Clone, Default)]
pub struct Current(Arc<Mutex<Option<Target>>>);

impl Current {
    pub fn set(&self, target: Option<Target>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = target;
    }

    pub fn get(&self) -> Option<Target> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
