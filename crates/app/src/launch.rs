use std::path::PathBuf;

use pith_core::{Config, init_log, log};
use pith_infer::{Worker, resolve_backend};
use pith_windows::{Inbound, Inbox, InsertMode, Options, Source, hardware, run, single_instance};

const SINGLE_INSTANCE_NAME: &str = "Local\\PithSingleInstance";
const POLL_MS: u32 = 250;

struct Paths {
    config: PathBuf,
    data: PathBuf,
    models: PathBuf,
    log: PathBuf,
    onboarded: PathBuf,
}

fn paths() -> Paths {
    let dirs = directories::ProjectDirs::from("", "", "Pith").expect("home directory");
    Paths {
        config: dirs.config_dir().join("config.toml"),
        data: dirs.data_local_dir().to_path_buf(),
        models: dirs.data_local_dir().join("models"),
        log: dirs.data_local_dir().join("pith.log"),
        onboarded: dirs.data_local_dir().join("onboarded"),
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn load_config(paths: &Paths) -> (Config, Option<String>) {
    let args: Vec<String> = std::env::args().collect();
    let (mut config, error) = match Config::load(&paths.config) {
        Ok(config) => (config, None),
        Err(error) => {
            log(&format!("settings error, using defaults: {error}"));
            (Config::default(), Some(error))
        }
    };
    if let Some(model) = flag(&args, "--model") {
        config.model_path = Some(PathBuf::from(model));
    }
    if let Some(threads) = flag(&args, "--threads").and_then(|t| t.parse().ok()) {
        config.threads = threads;
    }
    (config, error)
}

pub fn launch() {
    let paths = paths();
    init_log(&paths.log);
    let (config, config_error) = load_config(&paths);
    if !single_instance(SINGLE_INSTANCE_NAME) {
        log("another instance is running");
        return;
    }
    let (inbox, receiver) = Inbox::channel();
    let machine = hardware();
    let recommended = machine.recommended_model();
    log(&format!(
        "{} MiB memory, {} cores, recommended model: {}",
        machine.ram_mib,
        machine.cores,
        recommended.label()
    ));
    let models = paths.models.clone();
    let (preparing, ready, done) = (inbox.clone(), inbox.clone(), inbox.clone());
    let worker = Worker::spawn(
        config.clone(),
        Box::new(move |config, superseded| {
            resolve_backend(config, recommended, &models, superseded, &|stage| {
                preparing.post(Inbound::Preparing(stage))
            })
        }),
        Box::new(move |status| ready.post(Inbound::Ready(status))),
        Box::new(move |generation, result| done.post(Inbound::Done(generation, result))),
    );
    let options = Options {
        source: Source::Focused,
        insert: InsertMode::Paste,
        tray: true,
        poll_ms: POLL_MS,
        config_path: Some(paths.config.clone()),
        observer: None,
        global_keys: true,
        onboarding: (!paths.onboarded.exists()).then(|| paths.onboarded.clone()),
        data_dir: Some(paths.data.clone()),
        notice: config_error,
        recommended,
    };
    log("pith starting");
    if let Err(error) = run(config, worker, inbox, receiver, options) {
        log(&format!("fatal: {error}"));
    }
}
