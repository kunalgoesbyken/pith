use crate::{Config, FieldSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    Disabled,
    Password,
    Composing,
    BlockedApp,
    Selection,
    TextAfterCaret,
    TooShort,
    NotProse,
}

pub fn check(field: &FieldSnapshot, config: &Config) -> Result<(), SkipReason> {
    evaluate(field, config, false)
}

pub fn check_forced(field: &FieldSnapshot, config: &Config) -> Result<(), SkipReason> {
    evaluate(field, config, true)
}

fn evaluate(field: &FieldSnapshot, config: &Config, forced: bool) -> Result<(), SkipReason> {
    if !config.enabled {
        return Err(SkipReason::Disabled);
    }
    if field.is_password {
        return Err(SkipReason::Password);
    }
    if field.composing {
        return Err(SkipReason::Composing);
    }
    if config.is_blocked(&field.app) {
        return Err(SkipReason::BlockedApp);
    }
    if field.has_selection {
        return Err(SkipReason::Selection);
    }
    let rest_of_line = field.after.split(['\n', '\r']).next().unwrap_or_default();
    if !rest_of_line.trim().is_empty() {
        return Err(SkipReason::TextAfterCaret);
    }
    let current_line = field.before.rsplit(['\n', '\r']).next().unwrap_or_default();
    let has_context = !field.before.trim().is_empty();
    if forced {
        return if has_context { Ok(()) } else { Err(SkipReason::TooShort) };
    }
    if current_line.trim().chars().count() < config.min_chars_before {
        return Err(SkipReason::TooShort);
    }
    let visible = current_line.chars().filter(|c| !c.is_whitespace()).count();
    let letters = current_line.chars().filter(|c| c.is_alphabetic()).count();
    if letters * 2 < visible {
        return Err(SkipReason::NotProse);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(before: &str, after: &str) -> FieldSnapshot {
        FieldSnapshot {
            app: "notepad".into(),
            before: before.into(),
            after: after.into(),
            ..Default::default()
        }
    }

    #[test]
    fn matrix() {
        let config = Config::default();
        assert_eq!(check(&field("Hello the", ""), &config), Ok(()));
        assert_eq!(check(&field("Hello the", "\nnext line"), &config), Ok(()));
        assert_eq!(check(&field("Hello the", "\r\nnext line"), &config), Ok(()));
        assert_eq!(check(&field("Hello the", "   "), &config), Ok(()));
        assert_eq!(
            check(&field("Hello the", " world"), &config),
            Err(SkipReason::TextAfterCaret)
        );
        assert_eq!(
            check(&field("long first line\nhi", ""), &config),
            Err(SkipReason::TooShort)
        );
        assert_eq!(
            check(&field("long first line\r\nhi", ""), &config),
            Err(SkipReason::TooShort)
        );
        assert_eq!(check(&field("", ""), &config), Err(SkipReason::TooShort));
        let mut f = field("Hello the", "");
        f.is_password = true;
        assert_eq!(check(&f, &config), Err(SkipReason::Password));
        let mut f = field("Hello the", "");
        f.composing = true;
        assert_eq!(check(&f, &config), Err(SkipReason::Composing));
        let mut f = field("Hello the", "");
        f.has_selection = true;
        assert_eq!(check(&f, &config), Err(SkipReason::Selection));
        for app in [
            "WindowsTerminal.exe",
            "code",
            "Code.exe",
            "1Password.exe",
            "KEEPASS",
            "Code.EXE",
            "",
            r"C:\Program Files\KeePass\KeePass.exe",
        ] {
            let mut f = field("Hello the", "");
            f.app = app.into();
            assert_eq!(check(&f, &config), Err(SkipReason::BlockedApp), "{app}");
        }
        for line in [
            "123456",
            "4242 4242 4242 4242",
            "+1 (415) 555-0100",
            "$12,345.67",
        ] {
            assert_eq!(
                check(&field(line, ""), &config),
                Err(SkipReason::NotProse),
                "{line}"
            );
        }
        for line in [
            "Hello the",
            "你好世界和平",
            "नमस्ते दोस्त",
            "Meet at 10:30 tomorrow",
            "user@example.com",
        ] {
            assert_eq!(check(&field(line, ""), &config), Ok(()), "{line}");
        }
        for (before, ok) in [
            ("Hi", true),
            ("x", true),
            ("4242 4242", true),
            ("long first line
hi", true),
            ("long first line
", true),
            ("", false),
            (" 
	", false),
        ] {
            let verdict = check_forced(&field(before, ""), &config);
            assert_eq!(verdict.is_ok(), ok, "{before:?}");
        }
        for (change, reason) in [
            (
                (|f: &mut FieldSnapshot| f.is_password = true) as fn(&mut FieldSnapshot),
                SkipReason::Password,
            ),
            (|f| f.composing = true, SkipReason::Composing),
            (|f| f.has_selection = true, SkipReason::Selection),
            (|f| f.app = "cmd".into(), SkipReason::BlockedApp),
            (|f| f.after = " tail".into(), SkipReason::TextAfterCaret),
        ] {
            let mut f = field("Hi", "");
            change(&mut f);
            assert_eq!(check_forced(&f, &config), Err(reason));
        }
        let disabled = Config {
            enabled: false,
            ..Config::default()
        };
        assert_eq!(
            check(&field("Hello the", ""), &disabled),
            Err(SkipReason::Disabled)
        );
        assert_eq!(
            check_forced(&field("Hello the", ""), &disabled),
            Err(SkipReason::Disabled)
        );
    }
}
