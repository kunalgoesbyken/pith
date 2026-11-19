use pith_core::{KeyAction, Setting};

use super::UiEvent;
use super::page::Page;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Go(Page),
    Apply(Setting),
    Emit(UiEvent),
    Record(KeyAction, usize),
    ChooseApp,
    Opacity,
}
