use pith_core::{Completion, init_log, log};
use pith_infer::Stage;
use pith_windows::{Inbound, Inbox, InsertMode, Options, Source, hardware, run, single_instance};

use crate::setup::{Paths, WorkerEvents, directories, load_config, recommend, spawn_worker};

const SINGLE_INSTANCE_NAME: &str = "Local\\PithSingleInstance";
const POLL_MS: u32 = 250;
const ONBOARDED_MARKER: &str = "onboarded";

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
    let dirs = directories()?;
    let paths = Paths::of(&dirs);
    init_log(&paths.log);
    let args: Vec<String> = std::env::args().collect();
    let (config, config_error) = load_config(&paths.config, &args)?;
    if !single_instance(SINGLE_INSTANCE_NAME) {
        log("another instance is running");
        return Ok(());
    }
    let (inbox, receiver) = Inbox::channel();
    let recommended = recommend(hardware());
    let worker = spawn_worker(&config, recommended, paths.models.clone(), inbox.clone());
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
