mod apps;
mod general;
mod home;
mod model;
mod privacy;
mod shortcuts;

use pith_core::Setting;

use super::action::Action;
use super::layout::{Item, Sheet};
use super::model::Model as State;
use super::page::Page;

pub fn build(state: &State, sheet: &mut Sheet) {
    match state.page {
        Page::Home => home::build(state, sheet),
        Page::General => general::build(state, sheet),
        Page::Shortcuts => shortcuts::build(state, sheet),
        Page::Apps => apps::build(state, sheet),
        Page::Model => model::build(state, sheet),
        Page::Privacy => privacy::build(state, sheet),
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
