use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use pith_core::{Completion, Config, Hardware, ModelPreset, log};
use pith_infer::{Stage, Worker, resolve_backend};

const MODEL_FLAG: &str = "--model";
const THREADS_FLAG: &str = "--threads";

pub struct Paths {
    pub config: PathBuf,
    pub models: PathBuf,
    pub log: PathBuf,
}

pub fn directories() -> Result<ProjectDirs, String> {
    ProjectDirs::from("", "", "Pith").ok_or_else(|| "no home directory was found".to_string())
}

impl Paths {
    pub fn of(dirs: &ProjectDirs) -> Self {
        Self {
            config: dirs.config_dir().join("config.toml"),
            models: dirs.data_local_dir().join("models"),
            log: dirs.data_local_dir().join("pith.log"),
        }
    }
}

pub fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

pub fn load_config(path: &Path, args: &[String]) -> Result<(Config, Option<String>), String> {
    let (config, notice) = match Config::load(path) {
        Ok(config) => (config, None),
        Err(error) => {
            log(&format!("settings error, using defaults: {error}"));
            (Config::default(), Some(error))
        }
    };
    let threads = match flag(args, THREADS_FLAG) {
        Some(count) => count
            .parse()
            .map_err(|_| format!("{THREADS_FLAG} takes a number"))?,
        None => config.threads,
    };
    let overridden = Config {
        model_path: flag(args, MODEL_FLAG)
            .map(PathBuf::from)
            .or(config.model_path),
        threads,
        ..config
    };
    overridden.validate().map(|config| (config, notice))
}

pub fn recommend(machine: Hardware) -> ModelPreset {
    let recommended = machine.recommended_model();
    log(&format!(
        "{} MiB memory, {} cores, recommended model: {}",
        machine.ram_mib,
        machine.cores,
        recommended.label()
    ));
    recommended
}

pub trait WorkerEvents: Clone + Send + 'static {
    fn preparing(&self, stage: Stage);
    fn ready(&self, status: Result<String, String>);
    fn done(&self, generation: u64, result: Result<Completion, String>);
}

pub fn spawn_worker(
    config: &Config,
    recommended: ModelPreset,
    models: PathBuf,
    events: impl WorkerEvents,
) -> Worker {
    let (preparing, ready, done) = (events.clone(), events.clone(), events);
    Worker::spawn(
        config.clone(),
        Box::new(move |config, superseded| {
            resolve_backend(config, recommended, &models, superseded, &|stage| {
                preparing.preparing(stage)
            })
        }),
        Box::new(move |status| ready.ready(status)),
        Box::new(move |generation, result| done.done(generation, result)),
    )
}
