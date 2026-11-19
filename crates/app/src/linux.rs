use std::process::ExitCode;

use pith_core::{Completion, init_log, log};
use pith_infer::Stage;
use pith_linux::{Consent, Inbound, Inbox, Options, hardware, on_wayland, run, single_instance};

use crate::hooks;
use crate::setup::{Paths, WorkerEvents, directories, load_config, recommend, spawn_worker};

const ALLOW_ACCESSIBILITY: &str = "--allow-accessibility";
const WAYLAND_MESSAGE: &str =
    "Pith needs an X11 session and this is a Wayland session. See the Linux section of the README.";

impl WorkerEvents for Inbox {
    fn preparing(&self, stage: Stage) {
        self.post(Inbound::Preparing(stage));
    }

    fn ready(&self, status: Result<String, String>) {
        self.post(Inbound::Ready(status));
    }

    fn done(&self, generation: u64, result: Result<Completion, String>) {
        self.post(Inbound::Done(generation, result));
    }
}

fn start() -> Result<(), String> {
    if on_wayland() {
        return Err(WAYLAND_MESSAGE.to_string());
    }
    let paths = Paths::of(&directories()?);
    init_log(&paths.log);
    let args: Vec<String> = std::env::args().collect();
    let hooks = hooks::from_args(&args)?;
    let (config, _) = load_config(&paths.config, &args)?;
    let Some(_instance) = single_instance() else {
        log("another instance is running");
        return Ok(());
    };
    let (inbox, receiver) = Inbox::channel();
    let recommended = recommend(hardware());
    let worker = spawn_worker(&config, recommended, paths.models.clone(), inbox.clone());
    let options = Options {
        insert: hooks.insert,
        consent: args
            .iter()
            .any(|arg| arg == ALLOW_ACCESSIBILITY)
            .then(Consent::granted),
        observer: hooks.observer,
    };
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
