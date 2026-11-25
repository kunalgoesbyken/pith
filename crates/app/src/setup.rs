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

pub struct Extra {
    pub name: &'static str,
    pub takes_value: bool,
}

pub struct Cli {
    pub model: Option<PathBuf>,
    pub threads: Option<usize>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    extras: Vec<(&'static str, Option<String>)>,
}

impl Cli {
    pub fn parse(args: &[String], extras: &[&[Extra]]) -> Result<Self, String> {
        let mut cli = Self {
            model: None,
            threads: None,
            extras: Vec::new(),
        };
        let mut args = args.iter().skip(1);
        while let Some(arg) = args.next() {
            let mut value = |name: &str| {
                args.next()
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a value"))
            };
            if arg == MODEL_FLAG {
                cli.model = Some(PathBuf::from(value(MODEL_FLAG)?));
            } else if arg == THREADS_FLAG {
                cli.threads = Some(
                    value(THREADS_FLAG)?
                        .parse()
                        .map_err(|_| format!("{THREADS_FLAG} takes a number"))?,
                );
            } else if let Some(extra) = extras
                .iter()
                .copied()
                .flatten()
                .find(|extra| extra.name == arg)
            {
                let given = if extra.takes_value {
                    Some(value(extra.name)?)
                } else {
                    None
                };
                cli.extras.push((extra.name, given));
            } else {
                return Err(format!("unknown option {arg}"));
            }
        }
        Ok(cli)
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn has(&self, name: &str) -> bool {
        self.extras.iter().any(|(known, _)| *known == name)
    }

    #[cfg(feature = "e2e")]
    pub fn value(&self, name: &str) -> Option<&str> {
        self.extras
            .iter()
            .find(|(known, _)| *known == name)
            .and_then(|(_, value)| value.as_deref())
    }
}

pub fn load_config(path: &Path, cli: &Cli) -> Result<(Config, Option<String>), String> {
    let (config, notice) = match Config::load(path) {
        Ok(config) => (config, None),
        Err(error) => {
            log(&format!("settings error, using defaults: {error}"));
            (Config::default(), Some(error))
        }
    };
    let overridden = Config {
        model_path: cli.model.clone().or(config.model_path),
        threads: cli.threads.unwrap_or(config.threads),
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

pub enum WorkerMessage {
    Preparing(Stage),
    Ready(Result<String, String>),
    Done(u64, Result<Completion, String>),
}

pub fn spawn_worker(
    config: &Config,
    recommended: ModelPreset,
    models: PathBuf,
    notify: impl Fn(WorkerMessage) + Clone + Send + 'static,
) -> Worker {
    let (preparing, ready, done) = (notify.clone(), notify.clone(), notify);
    Worker::spawn(
        config.clone(),
        Box::new(move |config, superseded| {
            resolve_backend(config, recommended, &models, superseded, &|stage| {
                preparing(WorkerMessage::Preparing(stage))
            })
        }),
        Box::new(move |status| ready(WorkerMessage::Ready(status))),
        Box::new(move |generation, result| done(WorkerMessage::Done(generation, result))),
    )
}
