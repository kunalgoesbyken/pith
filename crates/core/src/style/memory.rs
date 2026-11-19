use serde::{Deserialize, Serialize};

use super::sentence::Sentence;
use crate::{Config, app_name, same_app};

pub const CAPACITY: usize = 200;
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub sentence: Sentence,
    pub app: String,
}

impl Sample {
    pub fn new(sentence: Sentence, app: &str) -> Self {
        Self {
            sentence,
            app: app_name(app).to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleMemory {
    entries: Vec<Sample>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    text: String,
    app: String,
}

#[derive(Serialize, Deserialize)]
struct Document {
    version: u32,
    #[serde(default)]
    entry: Vec<Record>,
}

impl StyleMemory {
    pub fn entries(&self) -> &[Sample] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn learn(&mut self, sample: Sample) {
        self.entries
            .retain(|kept| !(kept.sentence == sample.sentence && same_app(&kept.app, &sample.app)));
        if self.entries.len() == CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(sample);
    }

    pub fn forget(&mut self, app: &str, text: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|kept| !(kept.sentence.as_str() == text && same_app(&kept.app, app)));
        self.entries.len() != before
    }

    pub fn forget_blocked(&mut self, config: &Config) -> bool {
        let before = self.entries.len();
        self.entries.retain(|kept| !config.is_blocked(&kept.app));
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let document: Document = toml_edit::de::from_str(source).map_err(|e| e.to_string())?;
        if document.version != VERSION {
            return Err(format!("unknown style memory version {}", document.version));
        }
        let mut memory = Self::default();
        for record in document.entry {
            if let Some(sentence) = Sentence::learnable(&record.text) {
                memory.learn(Sample::new(sentence, &record.app));
            }
        }
        Ok(memory)
    }

    pub fn render(&self) -> Result<String, String> {
        let document = Document {
            version: VERSION,
            entry: self
                .entries
                .iter()
                .map(|sample| Record {
                    text: sample.sentence.as_str().to_string(),
                    app: sample.app.clone(),
                })
                .collect(),
        };
        toml_edit::ser::to_string_pretty(&document).map_err(|e| e.to_string())
    }
}
