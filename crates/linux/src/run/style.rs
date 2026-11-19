use std::path::PathBuf;
use std::sync::Arc;

use pith_core::{Config, Event, PlainVault, Sample, StyleStore, log};

use super::App;

pub(super) fn open(path: Option<PathBuf>, config: &Config) -> Option<StyleStore> {
    let mut store = StyleStore::open(path?, Box::new(PlainVault))
        .inspect_err(|error| log(&format!("style not loaded, not learning: {error}")))
        .ok()?;
    if let Err(error) = store.forget_blocked(config) {
        log(&format!("style not saved: {error}"));
    }
    Some(store)
}

impl App {
    pub(super) fn learn(&mut self, sample: Sample) {
        let Some(store) = &mut self.style else { return };
        if let Err(error) = store.learn(sample) {
            log(&format!("style not saved: {error}"));
        }
        self.share_style();
    }

    pub(super) fn share_style(&mut self) {
        let Some(store) = &self.style else { return };
        let memory = Arc::new(store.memory().clone());
        self.dispatch(Event::Style(memory));
    }
}
