use crate::config::Config;
use crate::placement::Placement;
use crate::suffix::After;
use crate::types::{FieldSnapshot, TextStyle};

pub const AFTER_CHARS: usize = 200;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surrounding {
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug)]
pub struct FieldParts {
    pub field_id: u64,
    pub app: Option<String>,
    pub window_title: String,
    pub surrounding: Option<Surrounding>,
    pub has_selection: bool,
    pub is_password: bool,
    pub composing: bool,
    pub language: Option<String>,
    pub placement: Option<Placement>,
    pub style: TextStyle,
}

pub fn is_redacted(config: &Config, app: Option<&str>, is_password: bool) -> bool {
    is_password || app.is_none_or(|name| config.is_blocked(name))
}

fn normalize(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}

impl FieldSnapshot {
    pub fn from_parts(config: &Config, parts: FieldParts) -> Self {
        let redacted = is_redacted(config, parts.app.as_deref(), parts.is_password);
        let surrounding = match parts.surrounding {
            Some(text) if !redacted => text,
            _ => Surrounding::default(),
        };
        Self {
            field_id: parts.field_id,
            app: parts.app.unwrap_or_default(),
            window_title: parts.window_title,
            before: normalize(surrounding.before),
            after: After::new(normalize(surrounding.after)),
            has_selection: parts.has_selection,
            is_password: parts.is_password,
            composing: parts.composing,
            language: parts.language,
            placement: parts.placement,
            style: parts.style,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldKey {
    pub(crate) id: u64,
    app: String,
}

impl FieldKey {
    pub(crate) fn of(field: &FieldSnapshot) -> Self {
        Self {
            id: field.field_id,
            app: field.app.clone(),
        }
    }

    pub(crate) fn matches(&self, field: &FieldSnapshot) -> bool {
        self.id == field.field_id && self.app == field.app
    }
}
