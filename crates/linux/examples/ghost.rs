#[cfg(target_os = "linux")]
mod ghost {
    use std::process::ExitCode;

    use pith_core::{Drawn, Placement, Rect, TextStyle};
    use pith_linux::{Desktop, Display, Overlay};

    const READY_LINE: &str = "shown";
    const OPACITY_PERCENT: u8 = 60;

    struct Options {
        placement: Placement,
        cuts: Vec<String>,
    }

    fn rect(source: &str) -> Result<Rect, String> {
        let values: Vec<i32> = source
            .split(',')
            .map(|value| value.parse().map_err(|_| format!("bad number in {source}")))
            .collect::<Result<_, _>>()?;
        match values[..] {
            [x, y, width, height] => Ok(Rect {
                x,
                y,
                width,
                height,
            }),
            _ => Err(format!("{source} is not x,y,width,height")),
        }
    }

    fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
        let kind = args.next().ok_or("needs inline, below or anchored")?;
        let target = rect(&args.next().ok_or("needs x,y,width,height")?)?;
        let placement = match kind.as_str() {
            "inline" => Placement::Inline(target),
            "below" => Placement::BelowCaret(target),
            "anchored" => Placement::Anchored(target),
            other => return Err(format!("unknown placement {other}")),
        };
        Ok(Options {
            placement,
            cuts: args.collect(),
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
        let display = match Display::connect() {
            Ok(display) => display,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        let desktop = match Desktop::new(display.clone()) {
            Ok(desktop) => desktop,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        let mut overlay = match Overlay::create(display, desktop, OPACITY_PERCENT) {
            Ok(overlay) => overlay,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        };
        match overlay.show(&options.cuts, options.placement, &TextStyle::default()) {
            Drawn::Shown(_) => eprintln!("{READY_LINE}"),
            Drawn::Nothing => return ExitCode::FAILURE,
        }
        loop {
            std::thread::park();
        }
    }
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    ghost::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {}
