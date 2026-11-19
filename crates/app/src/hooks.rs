use std::sync::mpsc::Sender;

use pith_core::Action;
use pith_linux::InsertPolicy;

#[derive(Default)]
pub struct Hooks {
    pub insert: InsertPolicy,
    pub observer: Option<Sender<Action>>,
}

#[cfg(feature = "e2e")]
mod enabled {
    use std::fs::File;
    use std::io::Write;
    use std::sync::mpsc::{Sender, channel};

    use pith_core::Action;
    use pith_linux::InsertPolicy;
    use serde_json::json;

    use super::Hooks;
    use crate::setup::flag;

    const INSERT_FLAG: &str = "--insert";
    const RECORD_FLAG: &str = "--record";

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

    pub fn from_args(args: &[String]) -> Result<Hooks, String> {
        let insert = match flag(args, INSERT_FLAG).as_deref() {
            None | Some("auto") => InsertPolicy::default(),
            Some("accessibility") => InsertPolicy::accessibility_only(),
            Some("clipboard") => InsertPolicy::clipboard_only(),
            Some(other) => {
                return Err(format!(
                    "{INSERT_FLAG}: {other} is not auto, accessibility or clipboard"
                ));
            }
        };
        let observer = flag(args, RECORD_FLAG).as_deref().map(record).transpose()?;
        Ok(Hooks { insert, observer })
    }
}

#[cfg(feature = "e2e")]
pub use enabled::from_args;

#[cfg(not(feature = "e2e"))]
pub fn from_args(_args: &[String]) -> Result<Hooks, String> {
    Ok(Hooks::default())
}
