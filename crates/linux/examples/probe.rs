#[cfg(target_os = "linux")]
mod probe {
    use std::process::ExitCode;
    use std::sync::mpsc::channel;

    use pith_core::Config;
    use pith_linux::{Consent, Desktop, Display, Held, Quit, Reader, Reading, Report};

    const ALLOW_ACCESSIBILITY: &str = "--allow-accessibility";
    const BLOCK_APP: &str = "--block";
    const READY_LINE: &str = "ready";

    struct Options {
        consent: Option<Consent>,
        config: Config,
    }

    fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
        let mut consent = None;
        let mut config = Config::default();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                ALLOW_ACCESSIBILITY => consent = Some(Consent::granted()),
                BLOCK_APP => config.blocked_apps.push(
                    args.next()
                        .ok_or(format!("{BLOCK_APP} needs an app name"))?,
                ),
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(Options { consent, config })
    }

    pub fn run() -> ExitCode {
        let options = match parse(std::env::args().skip(1)) {
            Ok(options) => options,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        };
        let (readings, received) = channel();
        let desktop = match Display::connect().and_then(Desktop::new) {
            Ok(desktop) => desktop,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        let reader = Reader::spawn(
            options.config,
            options.consent,
            desktop,
            Held::default(),
            &Quit::default(),
            Box::new(move |report| {
                let _ = readings.send(report);
            }),
        );
        let _reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        eprintln!("{READY_LINE}");
        for reading in received {
            match reading {
                Ok(Report::Inserted(_)) => {}
                Ok(Report::Reading(Reading::Field(field), _)) => {
                    match serde_json::to_string(&field) {
                        Ok(line) => println!("{line}"),
                        Err(error) => {
                            eprintln!("{error}");
                            return ExitCode::FAILURE;
                        }
                    }
                }
                Ok(Report::Reading(Reading::Nothing, _)) => println!("null"),
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::FAILURE;
                }
            }
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
