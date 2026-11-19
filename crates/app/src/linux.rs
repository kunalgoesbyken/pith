use std::process::ExitCode;

use pith_core::{init_log, log};
use pith_linux::{Consent, Inbound, Inbox, Options, hardware, on_wayland, run, single_instance};

use crate::hooks::{self, ALLOW_ACCESSIBILITY};
use crate::setup::{Cli, Paths, WorkerMessage, directories, load_config, recommend, spawn_worker};

const WAYLAND_MESSAGE: &str =
    "Pith needs an X11 session and this is a Wayland session. See the Linux section of the README.";

fn start() -> Result<(), String> {
    if on_wayland() {
        return Err(WAYLAND_MESSAGE.to_string());
    }
    let dirs = directories()?;
    let paths = Paths::of(&dirs);
    init_log(&paths.log);
    let args: Vec<String> = std::env::args().collect();
    let cli = Cli::parse(&args, hooks::FLAGS)?;
    let (config, _) = load_config(&paths.config, &cli)?;
    let Some(_instance) = single_instance().map_err(|error| format!("single instance: {error}"))?
    else {
        log("another instance is running");
        return Ok(());
    };
    let mut options = Options::new(cli.has(ALLOW_ACCESSIBILITY).then(Consent::granted));
    options.data_dir = Some(dirs.data_local_dir().to_path_buf());
    hooks::apply(&cli, &mut options)?;
    let (inbox, receiver) = Inbox::channel();
    let recommended = recommend(hardware());
    let events = inbox.clone();
    let worker = spawn_worker(&config, recommended, paths.models.clone(), move |message| {
        events.post(match message {
            WorkerMessage::Preparing(stage) => Inbound::Preparing(stage),
            WorkerMessage::Ready(status) => Inbound::Ready(status),
            WorkerMessage::Done(generation, result) => Inbound::Done(generation, result),
        });
    });
    log("pith starting");
    run(config, worker, inbox, receiver, options).map_err(|error| error.to_string())
}

pub fn launch() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            log(&format!("fatal: {error}"));
            ExitCode::FAILURE
        }
    }
}
