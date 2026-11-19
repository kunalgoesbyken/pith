use std::str::FromStr;

use pith_core::StyleMemory;
use serde::Deserialize;

const SOURCES: [(&str, &str); 3] = [
    ("casual-en", include_str!("personas/casual-en.toml")),
    ("formal-en", include_str!("personas/formal-en.toml")),
    ("de", include_str!("personas/de.toml")),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Off,
    Persona(&'static str),
}

impl FromStr for Style {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, String> {
        if name == "off" {
            return Ok(Self::Off);
        }
        SOURCES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(known, _)| Self::Persona(known))
            .ok_or_else(|| format!("unknown persona {name}"))
    }
}

#[derive(Deserialize)]
struct HeldOut {
    text: String,
}

#[derive(Deserialize)]
struct Held {
    held_out: Vec<HeldOut>,
}

pub struct Persona {
    pub memory: StyleMemory,
    pub held_out: Vec<String>,
}

impl Style {
    fn source(self) -> Option<&'static str> {
        SOURCES
            .iter()
            .find(|(name, _)| Self::Persona(name) == self)
            .map(|(_, source)| *source)
    }

    pub fn memory(self) -> StyleMemory {
        self.source()
            .map(|source| StyleMemory::parse(source).expect("persona"))
            .unwrap_or_default()
    }

    pub fn persona(self) -> Option<Persona> {
        let source = self.source()?;
        let held: Held = toml_edit::de::from_str(source).expect("held out");
        Some(Persona {
            memory: self.memory(),
            held_out: held.held_out.into_iter().map(|held| held.text).collect(),
        })
    }
}
