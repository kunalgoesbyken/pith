use pith_core::{init_log, log};
use pith_windows::{
    BACKGROUND_FLAG, Inbound, Inbox, InsertMode, Options, Source, Start,
    ensure_start_menu_shortcut, hardware, refresh_start_with_windows, reveal_running_instance, run,
    single_instance,
};

use crate::setup::{
    Cli, Extra, Paths, WorkerMessage, directories, load_config, recommend, spawn_worker,
};

const SINGLE_INSTANCE_NAME: &str = "Local\\PithSingleInstance";
const POLL_MS: u32 = 250;
const ONBOARDED_MARKER: &str = "onboarded";

fn start() -> Result<(), String> {
    let dirs = directories()?;
    let paths = Paths::of(&dirs);
    init_log(&paths.log);
    let args: Vec<String> = std::env::args().collect();
    let background = Extra {
        name: BACKGROUND_FLAG,
        takes_value: false,
    };
    let cli = Cli::parse(&args, &[background])?;
    let (config, config_error) = load_config(&paths.config, &cli)?;
    if !single_instance(SINGLE_INSTANCE_NAME) {
        log("another instance is running, asking it to show its window");
        reveal_running_instance();
        return Ok(());
    }
    if let Err(error) = ensure_start_menu_shortcut() {
        log(&format!("start menu shortcut not created: {error}"));
    }
    if let Err(error) = refresh_start_with_windows() {
        log(&format!("start with Windows not refreshed: {error}"));
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
    let start = if !onboarded.exists() {
        Start::Onboarding(onboarded)
    } else if cli.has(BACKGROUND_FLAG) {
        Start::Tray
    } else {
        Start::Home
    };
    let options = Options {
        source: Source::Focused,
        insert: InsertMode::Paste,
        tray: true,
        poll_ms: POLL_MS,
        config_path: Some(paths.config.clone()),
        observer: None,
        global_keys: true,
        start,
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
