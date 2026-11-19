#[cfg(not(feature = "e2e"))]
use pith_linux::Options;

#[cfg(not(feature = "e2e"))]
use crate::setup::Cli;
use crate::setup::Extra;

pub const ALLOW_ACCESSIBILITY: &str = "--allow-accessibility";
pub const OPEN_SETTINGS: &str = "--settings";

pub const FLAGS: &[Extra] = &[
    Extra {
        name: ALLOW_ACCESSIBILITY,
        takes_value: false,
    },
    Extra {
        name: OPEN_SETTINGS,
        takes_value: false,
    },
    #[cfg(feature = "e2e")]
    Extra {
        name: enabled::INSERT_FLAG,
        takes_value: true,
    },
    #[cfg(feature = "e2e")]
    Extra {
        name: enabled::RECORD_FLAG,
        takes_value: true,
    },
];

#[cfg(feature = "e2e")]
mod enabled {
    use std::fs::File;
    use std::io::Write;
    use std::sync::mpsc::{Sender, channel};

    use pith_core::Action;
    use pith_linux::{InsertPolicy, Options};
    use serde_json::json;

    use crate::setup::Cli;

    pub const INSERT_FLAG: &str = "--insert";
    pub const RECORD_FLAG: &str = "--record";

    fn record(path: &str) -> Result<Sender<Action>, String> {
        let mut file = File::create(path).map_err(|error| format!("{RECORD_FLAG}: {error}"))?;
        let (sender, received) = channel();
        std::thread::spawn(move || {
            for action in received {
                let line = match action {
                    Action::Show { cuts, .. } => json!({ "event": "show", "text": cuts }),
                    Action::Insert(text) => json!({ "event": "insert", "text": text }),
                    _ => continue,
                };
                if writeln!(file, "{line}").is_err() {
                    return;
                }
            }
        });
        Ok(sender)
    }

    pub fn apply(cli: &Cli, options: &mut Options) -> Result<(), String> {
        options.insert = match cli.value(INSERT_FLAG) {
            None | Some("auto") => InsertPolicy::Auto,
            Some("accessibility") => InsertPolicy::Accessibility,
            Some("clipboard") => InsertPolicy::Clipboard,
            Some(other) => {
                return Err(format!(
                    "{INSERT_FLAG}: {other} is not auto, accessibility or clipboard"
                ));
            }
        };
        options.observer = cli.value(RECORD_FLAG).map(record).transpose()?;
        Ok(())
    }
}

#[cfg(feature = "e2e")]
pub use enabled::apply;

#[cfg(not(feature = "e2e"))]
pub fn apply(_cli: &Cli, _options: &mut Options) -> Result<(), String> {
    Ok(())
}
