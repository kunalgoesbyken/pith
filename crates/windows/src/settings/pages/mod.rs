mod apps;
mod backend;
mod general;
mod home;
mod privacy;
mod shortcuts;

use pith_core::{Backend, Setting};

use super::action::Action;
use super::layout::{Item, Sheet, Voice};
use super::model::Model;
use super::page::Page;

pub fn build(model: &Model, sheet: &mut Sheet) {
    match model.page {
        Page::Home => home::build(model, sheet),
        Page::General => general::build(model, sheet),
        Page::Shortcuts => shortcuts::build(model, sheet),
        Page::Apps => apps::build(model, sheet),
        Page::Model => backend::build(model, sheet),
        Page::Privacy => privacy::build(model, sheet),
    }
}

fn backend_name(backend: &Backend) -> String {
    match backend {
        Backend::Remote { .. } => "Remote model".to_string(),
        Backend::File(_) => "Custom model".to_string(),
        Backend::Preset(preset) => format!("{} model", preset.label()),
    }
}

fn notice(model: &Model, sheet: &mut Sheet) {
    if let Some(notice) = &model.notice {
        sheet.note(notice, Voice::Alert);
    }
}

fn choice<T: Copy + PartialEq>(
    options: impl IntoIterator<Item = (String, T)>,
    current: Option<T>,
    setting: impl Fn(T) -> Setting,
) -> Vec<Item> {
    let options = options
        .into_iter()
        .map(|(label, value)| {
            let selected = current == Some(value);
            (label, selected, Action::Apply(setting(value)))
        })
        .collect();
    Item::segments(options)
}

fn switch(name: &str, on: bool, setting: impl Fn(bool) -> Setting) -> Item {
    Item::toggle(name, on, Action::Apply(setting(!on)))
}
