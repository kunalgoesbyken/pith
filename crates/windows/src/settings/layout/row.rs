use windows::Win32::Foundation::RECT;

use super::item::Item;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Quiet,
    Alert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub typed: String,
    pub ghost: String,
    pub opacity: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub title: String,
    pub caption: Option<(String, Voice)>,
    pub badge: Option<String>,
    pub keys: Vec<String>,
    pub items: Vec<Item>,
    pub joined: bool,
    pub value: Option<String>,
    pub sample: Option<Sample>,
}

impl Row {
    pub fn titled(title: &str) -> Self {
        Self {
            title: title.to_string(),
            ..Self::default()
        }
    }

    pub fn caption(self, text: &str) -> Self {
        self.voiced(text, Voice::Quiet)
    }

    pub fn voiced(self, text: &str, voice: Voice) -> Self {
        Self {
            caption: Some((text.to_string(), voice)),
            ..self
        }
    }

    pub fn badge(self, text: &str) -> Self {
        Self {
            badge: Some(text.to_string()),
            ..self
        }
    }

    pub fn keys(self, chords: Vec<String>) -> Self {
        Self {
            keys: chords,
            ..self
        }
    }

    pub fn control(self, item: Item) -> Self {
        Self {
            items: vec![item],
            ..self
        }
    }

    pub fn buttons(self, items: Vec<Item>) -> Self {
        Self { items, ..self }
    }

    pub fn segments(self, items: Vec<Item>) -> Self {
        Self {
            items,
            joined: true,
            ..self
        }
    }

    pub fn value(self, text: &str) -> Self {
        Self {
            value: Some(text.to_string()),
            ..self
        }
    }

    pub fn sample(sample: Sample) -> Self {
        Self {
            sample: Some(sample),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct KeyGroup {
    pub origin: (i32, i32),
    pub chord: String,
}

#[derive(Clone, Debug)]
pub struct PlacedRow {
    pub bounds: RECT,
    pub divided: bool,
    pub title: Option<(RECT, String)>,
    pub caption: Option<(RECT, String, Voice)>,
    pub badge: Option<(RECT, String)>,
    pub keys: Vec<KeyGroup>,
    pub separators: Vec<RECT>,
    pub value: Option<(RECT, String)>,
    pub sample: Option<(RECT, Sample)>,
}
