use pith_core::HISTORY_DAYS;

use super::super::action::Action;
use super::super::page::Page;
use crate::ui::{ChipState, Seam, Tone};

#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    Toggle,
    Segment(Seam),
    Button(Tone),
    Nav { first: bool, icon: char },
    Chip(ChipState),
    Slider { value: i32, low: i32, high: i32 },
    Tile(String),
    Strip([u64; HISTORY_DAYS]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Check,
    Radio { first: bool },
    Push,
    Slider,
    Label,
}

impl Face {
    pub fn class(&self) -> Class {
        match self {
            Self::Toggle => Class::Check,
            Self::Segment(seam) => Class::Radio {
                first: matches!(seam, Seam::First | Seam::Only),
            },
            Self::Nav { first, .. } => Class::Radio { first: *first },
            Self::Button(_) | Self::Chip(_) => Class::Push,
            Self::Slider { .. } => Class::Slider,
            Self::Tile(_) | Self::Strip(_) => Class::Label,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub face: Face,
    pub label: String,
    pub name: String,
    pub checked: bool,
    pub enabled: bool,
    pub action: Action,
}

impl Item {
    fn new(face: Face, label: &str, name: &str, action: Action) -> Self {
        Self {
            face,
            label: label.to_string(),
            name: name.to_string(),
            checked: false,
            enabled: true,
            action,
        }
    }

    pub fn toggle(name: &str, on: bool, action: Action) -> Self {
        Self {
            checked: on,
            ..Self::new(Face::Toggle, "", name, action)
        }
    }

    pub fn button(label: &str, tone: Tone, action: Action) -> Self {
        Self::new(Face::Button(tone), label, label, action)
    }

    pub fn chip(name: &str, label: &str, state: ChipState, action: Action) -> Self {
        Self::new(Face::Chip(state), label, name, action)
    }

    pub fn slider(name: &str, (value, low, high): (i32, i32, i32), action: Action) -> Self {
        Self::new(Face::Slider { value, low, high }, "", name, action)
    }

    pub fn tile(name: &str, value: &str, caption: &str) -> Self {
        Self::new(Face::Tile(value.to_string()), caption, name, Action::None)
    }

    pub fn strip(name: &str, caption: &str, days: [u64; HISTORY_DAYS]) -> Self {
        Self::new(Face::Strip(days), caption, name, Action::None)
    }

    pub fn nav(page: Page, first: bool, selected: bool) -> Self {
        let face = Face::Nav {
            first,
            icon: page.icon(),
        };
        let label = page.label();
        Self {
            checked: selected,
            ..Self::new(face, label, label, Action::Go(page))
        }
    }

    pub fn segments(options: Vec<(String, bool, Action)>) -> Vec<Self> {
        let count = options.len();
        options
            .into_iter()
            .enumerate()
            .map(|(index, (label, selected, action))| Self {
                checked: selected,
                ..Self::new(
                    Face::Segment(Seam::of(index, count)),
                    &label,
                    &label,
                    action,
                )
            })
            .collect()
    }

    pub fn named(self, name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..self
        }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }
}
