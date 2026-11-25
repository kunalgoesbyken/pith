use pith_core::Reach;

const TOOLTIP_UNITS: usize = 127;

pub fn promise(reach: &Reach) -> String {
    match reach {
        Reach::ThisPc => {
            "Finishes your sentences as you type, on this PC. Your text never leaves it.".into()
        }
        Reach::Server(host) => format!(
            "Finishes your sentences as you type, using the server at {host}. The text before the caret is sent there."
        ),
    }
}

pub fn footer(reach: &Reach) -> String {
    match reach {
        Reach::ThisPc => "Runs on this PC. Your text never leaves it.".into(),
        Reach::Server(host) => format!("Sends the text before the caret to {host}."),
    }
}

pub fn tooltip(reach: &Reach) -> String {
    let full = match reach {
        Reach::ThisPc => "Pith \u{b7} local only, your text stays on this PC".to_string(),
        Reach::Server(host) => format!("Pith \u{b7} sends text to {host}"),
    };
    if full.encode_utf16().count() <= TOOLTIP_UNITS {
        return full;
    }
    let mut shown = String::new();
    for c in full.chars() {
        if shown.encode_utf16().count() + c.len_utf16() + 1 > TOOLTIP_UNITS {
            break;
        }
        shown.push(c);
    }
    shown.push('\u{2026}');
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(host: &str) -> Reach {
        Reach::Server(host.into())
    }

    #[test]
    fn server_copy_names_the_host_and_never_claims_local() {
        let reach = server("api.example.com");
        for text in [promise(&reach), footer(&reach), tooltip(&reach)] {
            assert!(text.contains("api.example.com"), "{text}");
            assert!(!text.contains("never leaves"), "{text}");
            assert!(!text.contains("stays on this PC"), "{text}");
        }
    }

    #[test]
    fn a_long_host_is_cut_to_fit_the_tray_tooltip() {
        for host in [
            "a".repeat(253),
            "\u{1f600}".repeat(120),
            "\u{4e2d}".repeat(200),
        ] {
            let text = tooltip(&server(&host));
            assert!(text.encode_utf16().count() <= TOOLTIP_UNITS, "{text}");
            assert!(text.ends_with('\u{2026}'));
        }
    }
}
