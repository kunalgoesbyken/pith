use std::process::ExitCode;

use pith_core::{init_log, log};
use pith_linux::{
    Autostart, Consent, Inbound, Inbox, Options, Start, hardware, on_wayland, run, single_instance,
};

use crate::hooks;
use crate::setup::{
    Cli, Extra, Paths, WorkerMessage, directories, load_config, recommend, spawn_worker,
};

const ALLOW_ACCESSIBILITY: &str = "--allow-accessibility";
const OPEN_SETTINGS: &str = "--settings";
const FLAGS: &[Extra] = &[
    Extra {
        name: ALLOW_ACCESSIBILITY,
        takes_value: false,
    },
    Extra {
        name: OPEN_SETTINGS,
        takes_value: false,
    },
];
const ONBOARDED_MARKER: &str = "onboarded";
const WAYLAND_MESSAGE: &str =
    "Pith needs an X11 session and this is a Wayland session. See the Linux section of the README.";

fn autostart(cli: &Cli) -> Option<Autostart> {
    let autostart = Autostart::new(
        cli.has(ALLOW_ACCESSIBILITY)
            .then_some(ALLOW_ACCESSIBILITY)
            .as_slice(),
    )?;
    if let Err(error) = autostart.refresh() {
        log(&format!("start at login not refreshed: {error}"));
    }
    Some(autostart)
}

fn start() -> Result<(), String> {
    if on_wayland() {
        return Err(WAYLAND_MESSAGE.to_string());
    }
    let dirs = directories()?;
    let paths = Paths::of(&dirs);
    init_log(&paths.log);
    let args: Vec<String> = std::env::args().collect();
    let cli = Cli::parse(&args, &[FLAGS, hooks::FLAGS])?;
    let (config, notice) = load_config(&paths.config, &cli)?;
    let (inbox, receiver) = Inbox::channel();
    let Some(_instance) =
        single_instance(inbox.clone()).map_err(|error| format!("single instance: {error}"))?
    else {
        log("another instance is running, asked it to open its settings");
        return Ok(());
    };
    let recommended = recommend(hardware());
    let mut options = Options::new(
        cli.has(ALLOW_ACCESSIBILITY).then(Consent::granted),
        paths.config.clone(),
        recommended,
    );
    options.data_dir = Some(dirs.data_local_dir().to_path_buf());
    hooks::apply(&cli, &mut options)?;
    options.notice = notice;
    options.autostart = autostart(&cli);
    let onboarded = dirs.data_local_dir().join(ONBOARDED_MARKER);
    options.start = if cli.has(OPEN_SETTINGS) {
        Start::Home
    } else if onboarded.exists() {
        Start::Tray
    } else {
        Start::Welcome(onboarded)
    };
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
