#[cfg(target_os = "linux")]
mod probe {
    use std::process::ExitCode;
    use std::sync::mpsc::channel;

    use pith_core::{Config, FieldSnapshot, Placement};
    use pith_linux::{Accessibility, Consent, Reader};
    use serde_json::{Value, json};

    const ALLOW_ACCESSIBILITY: &str = "--allow-accessibility";
    const BLOCK_APP: &str = "--block";

    struct Options {
        accessibility: Accessibility,
        config: Config,
    }

    fn parse(args: impl Iterator<Item = String>) -> Result<Options, String> {
        let mut accessibility = Accessibility::RequireEnabled;
        let mut config = Config::default();
        let mut args = args;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                ALLOW_ACCESSIBILITY => {
                    accessibility = Accessibility::EnableWithConsent(Consent::granted());
                }
                BLOCK_APP => config.blocked_apps.push(
                    args.next()
                        .ok_or(format!("{BLOCK_APP} needs an app name"))?,
                ),
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(Options {
            accessibility,
            config,
        })
    }

    fn placement(placement: Option<Placement>) -> Value {
        let Some(placement) = placement else {
            return Value::Null;
        };
        let (kind, rect) = match placement {
            Placement::Inline(rect) => ("inline", rect),
            Placement::Anchored(rect) => ("anchored", rect),
        };
        json!({
            "kind": kind,
            "x": rect.x,
            "y": rect.y,
            "width": rect.width,
            "height": rect.height,
        })
    }

    fn describe(snapshot: Option<FieldSnapshot>) -> Value {
        let Some(field) = snapshot else {
            return Value::Null;
        };
        json!({
            "field_id": field.field_id,
            "app": field.app,
            "window_title": field.window_title,
            "before": field.before,
            "after": field.after,
            "has_selection": field.has_selection,
            "is_password": field.is_password,
            "composing": field.composing,
            "placement": placement(field.placement),
            "style": {
                "family": field.style.family,
                "px": field.style.px,
                "color": field.style.color.map(|color| color.0),
            },
        })
    }

    pub fn run() -> ExitCode {
        let options = match parse(std::env::args().skip(1)) {
            Ok(options) => options,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        };
        let (snapshots, received) = channel();
        let reader = Reader::spawn(
            options.config,
            options.accessibility,
            Box::new(move |snapshot| {
                let _ = snapshots.send(snapshot);
            }),
        );
        let _reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        for snapshot in received {
            println!("{}", describe(snapshot));
        }
        ExitCode::SUCCESS
    }
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    probe::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {}
