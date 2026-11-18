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
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut parts: Vec<&str> = source.split('+').map(str::trim).collect();
        let name = parts.pop().unwrap_or_default();
        let key = parse_key(name).ok_or_else(|| format!("unknown key \"{source}\""))?;
        let mut chord = Self {
            ctrl: false,
            shift: false,
            alt: false,
            win: false,
            key,
        };
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
    fn parses_the_default_bindings() {
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
        assert!(chord("Shift+Tab").shift);
        let word = chord(" ctrl + RIGHT ");
        assert!(word.ctrl && !word.shift);
        assert_eq!(word.key, Key::Right);
        let trigger = chord("Ctrl+Shift+Space");
        assert!(trigger.ctrl && trigger.shift);
        assert_eq!(trigger.key, Key::Space);
    }

    #[test]
    fn parses_letters_digits_and_function_keys() {
        assert_eq!(chord("Ctrl+j").key, Key::Char('J'));
        assert_eq!(chord("Alt+7").key, Key::Char('7'));
        assert_eq!(chord("F1").key, Key::Function(1));
        assert_eq!(chord("Ctrl+f24").key, Key::Function(24));
        assert_eq!(chord("Esc").key, Key::Escape);
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
