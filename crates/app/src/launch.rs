use pith_core::{init_log, log};
use pith_windows::{Inbound, Inbox, InsertMode, Options, Source, hardware, run, single_instance};

use crate::setup::{Cli, Paths, WorkerMessage, directories, load_config, recommend, spawn_worker};

const SINGLE_INSTANCE_NAME: &str = "Local\\PithSingleInstance";
const POLL_MS: u32 = 250;
const ONBOARDED_MARKER: &str = "onboarded";

fn start() -> Result<(), String> {
    let dirs = directories()?;
    let paths = Paths::of(&dirs);
    init_log(&paths.log);
    let args: Vec<String> = std::env::args().collect();
    let cli = Cli::parse(&args, &[])?;
    let (config, config_error) = load_config(&paths.config, &cli)?;
    if !single_instance(SINGLE_INSTANCE_NAME) {
        log("another instance is running");
        return Ok(());
    }
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
    let onboarded = dirs.data_local_dir().join(ONBOARDED_MARKER);
    let options = Options {
        source: Source::Focused,
        insert: InsertMode::Paste,
        tray: true,
        poll_ms: POLL_MS,
        config_path: Some(paths.config.clone()),
        observer: None,
        global_keys: true,
        onboarding: (!onboarded.exists()).then_some(onboarded),
        data_dir: Some(dirs.data_local_dir().to_path_buf()),
        notice: config_error,
        recommended,
    };
    log("pith starting");
    run(config, worker, inbox, receiver, options).map_err(|error| error.to_string())
}

pub fn launch() {
    if let Err(error) = start() {
        eprintln!("{error}");
        log(&format!("fatal: {error}"));
    }
}
