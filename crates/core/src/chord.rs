use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    Right,
    Escape,
    Space,
    Char(char),
    Function(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub key: Key,
}

impl Chord {
    pub const ESCAPE: Self = Self::plain(Key::Escape);

    pub const fn plain(key: Key) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            win: false,
            key,
        }
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let mut parts: Vec<&str> = source.split('+').map(str::trim).collect();
        let name = parts.pop().unwrap_or_default();
        let key = parse_key(name).ok_or_else(|| format!("unknown key \"{source}\""))?;
        let mut chord = Self::plain(key);
        for part in parts {
            let flag = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut chord.ctrl,
                "shift" => &mut chord.shift,
                "alt" => &mut chord.alt,
                "win" | "super" => &mut chord.win,
                _ => return Err(format!("unknown modifier \"{source}\"")),
            };
            if std::mem::replace(flag, true) {
                return Err(format!("repeated modifier \"{source}\""));
            }
        }
        chord.ensure_safe(source)?;
        Ok(chord)
    }

    fn ensure_safe(&self, source: &str) -> Result<(), String> {
        let types_text = matches!(self.key, Key::Char(_) | Key::Space);
        let command = self.ctrl || self.alt || self.win;
        if types_text && !command {
            return Err(format!(
                "\"{source}\" would swallow typing, add Ctrl, Alt or Win"
            ));
        }
        if types_text && self.ctrl && self.alt {
            return Err(format!(
                "\"{source}\" collides with AltGr typing on many layouts"
            ));
        }
        Ok(())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Tab => f.write_str("Tab"),
            Key::Right => f.write_str("Right"),
            Key::Escape => f.write_str("Esc"),
            Key::Space => f.write_str("Space"),
            Key::Char(c) => write!(f, "{c}"),
            Key::Function(n) => write!(f, "F{n}"),
        }
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let modifiers = [
            (self.ctrl, "Ctrl"),
            (self.shift, "Shift"),
            (self.alt, "Alt"),
            (self.win, "Win"),
        ];
        for (held, name) in modifiers {
            if held {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

impl Serialize for Chord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Chord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Self::parse(&source).map_err(de::Error::custom)
    }
}

fn parse_key(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "tab" => return Some(Key::Tab),
        "right" => return Some(Key::Right),
        "esc" | "escape" => return Some(Key::Escape),
        "space" => return Some(Key::Space),
        _ => {}
    }
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&number).then_some(Key::Function(number));
    }
    let mut chars = lower.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(Key::Char(c.to_ascii_uppercase())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(source: &str) -> Chord {
        Chord::parse(source).unwrap()
    }

    #[test]
    fn parses_named_keys_letters_digits_and_function_keys() {
        assert_eq!(
            chord("Tab"),
            Chord {
                ctrl: false,
                shift: false,
                alt: false,
                win: false,
                key: Key::Tab
            }
        );
        let table = [
            ("Shift+Tab", false, true, Key::Tab),
            (" ctrl + RIGHT ", true, false, Key::Right),
            ("Ctrl+Shift+Space", true, true, Key::Space),
            ("Ctrl+j", true, false, Key::Char('J')),
            ("Alt+7", false, false, Key::Char('7')),
            ("F1", false, false, Key::Function(1)),
            ("Ctrl+f24", true, false, Key::Function(24)),
            ("Esc", false, false, Key::Escape),
        ];
        for row in table {
            let (source, ctrl, shift, key) = row;
            let parsed = chord(source);
            assert_eq!(
                (parsed.ctrl, parsed.shift, parsed.key),
                (ctrl, shift, key),
                "{row:?}"
            );
        }
    }

    #[test]
    fn displays_what_it_parses() {
        for source in [
            "Tab",
            "Shift+Tab",
            "Ctrl+Right",
            "Ctrl+Shift+Space",
            "Ctrl+Alt+Win+F24",
            "Esc",
            "Ctrl+J",
            "Alt+7",
        ] {
            let parsed = chord(source);
            assert_eq!(parsed.to_string(), source);
            assert_eq!(chord(&parsed.to_string()), parsed);
        }
        assert_eq!(chord("shift+ctrl+escape").to_string(), "Ctrl+Shift+Esc");
    }

    #[test]
    fn rejects_malformed_chords() {
        for source in [
            "",
            "+",
            "Ctrl+",
            "+Tab",
            "Ctrl++Tab",
            "Tab+Ctrl",
            "Hyper+Tab",
            "Ctrl+Ctrl+Tab",
            "Ctrl+Shift+shift+Tab",
            "F0",
            "F25",
            "Ctrl+ab",
            "Ctrl+\u{e9}",
            "Ctrl+\u{ff34}",
            "Ctrl+\u{440}",
            "\u{540d}\u{524d}",
            "Ctrl+\u{1f600}",
            "Tab\u{200b}",
            "Ctrl+Enter",
        ] {
            assert!(Chord::parse(source).is_err(), "{source:?}");
        }
    }

    #[test]
    fn rejects_chords_that_would_swallow_typing() {
        for source in [
            "a",
            "A",
            "Space",
            "Shift+Space",
            "Shift+a",
            "5",
            "Ctrl+Alt+a",
            "Ctrl+Alt+Space",
        ] {
            assert!(Chord::parse(source).is_err(), "{source:?}");
        }
        for source in [
            "Ctrl+a",
            "Alt+Space",
            "Win+a",
            "Tab",
            "Right",
            "Shift+Right",
            "F5",
            "Shift+F5",
        ] {
            assert!(Chord::parse(source).is_ok(), "{source:?}");
        }
    }
}
