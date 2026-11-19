use std::path::Path;
use std::sync::Arc;

use pith_core::{Event, Sample, StyleMemory, StyleStore, log};

use super::App;
use crate::settings::StyleLine;
use crate::vault::DpapiVault;

const STYLE_FILE: &str = "style.bin";

pub struct Style(Option<StyleStore>);

impl Style {
    pub fn open(data_dir: Option<&Path>) -> Self {
        let store = data_dir.and_then(|dir| {
            StyleStore::open(dir.join(STYLE_FILE), Box::new(DpapiVault))
                .inspect_err(|error| log(&format!("style not loaded, not learning: {error}")))
                .ok()
        });
        Self(store)
    }

    pub fn memory(&self) -> Arc<StyleMemory> {
        let memory = self.0.as_ref().map(|store| store.memory().clone());
        Arc::new(memory.unwrap_or_default())
    }

    pub fn lines(&self) -> Vec<StyleLine> {
        let entries = self.0.as_ref().map(|store| store.memory().entries());
        entries
            .unwrap_or_default()
            .iter()
            .rev()
            .map(|sample| StyleLine {
                app: sample.app.clone(),
                text: sample.sentence.as_str().to_string(),
            })
            .collect()
    }

    fn len(&self) -> usize {
        self.0
            .as_ref()
            .map_or(0, |store| store.memory().entries().len())
    }

    fn edit(&mut self, change: impl FnOnce(&mut StyleStore) -> Result<(), String>) {
        let Some(store) = &mut self.0 else { return };
        if let Err(error) = change(store) {
            log(&format!("style not saved: {error}"));
        }
    }
}

impl App {
    pub(super) fn learn(&mut self, sample: Sample) {
        self.style.edit(|store| store.learn(sample));
        self.style_changed();
    }

    pub(super) fn forget_sentence(&mut self, app: &str, text: &str) {
        self.style.edit(|store| store.forget(app, text));
        self.style_changed();
    }

    pub(super) fn forget_blocked_style(&mut self) {
        let before = self.style.len();
        let config = self.prefs.saved().clone();
        self.style.edit(|store| store.forget_blocked(&config));
        if self.style.len() != before {
            self.style_changed();
        }
    }

    pub(super) fn forget_all_style(&mut self) {
        if self.presenter.confirm_forget_style() {
            self.style.edit(StyleStore::erase);
            self.style_changed();
        }
    }

    pub(super) fn style_changed(&mut self) {
        self.presenter.set_style(self.style.lines());
        self.dispatch(Event::Style(self.style.memory()));
    }
}
