#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

use pith_core::{Config, init_log, log};
use pith_infer::{Backend, cpu_supported, download, find_local, preset};

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

fn resolve_backend(
    config: &Config,
    models: &Path,
    status: &dyn Fn(String),
    progress: &dyn Fn(u64, u64),
) -> Result<Backend, String> {
    if let Some(url) = config.remote_url.clone() {
        return Ok(Backend::Remote {
            url,
            model: config.remote_model.clone(),
        });
    }
    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let threads = config.resolved_threads(available);
    cpu_supported().map_err(|missing| {
        format!("this CPU lacks {missing}, which the built-in model needs; set remote_url in settings to use a server instead")
    })?;
    let spec = preset(config.model);
    let model_path = match config
        .model_path
        .clone()
        .or_else(|| find_local(models, spec))
    {
        Some(path) => path,
        None => {
            progress(0, spec.bytes);
            status(format!("downloading {}", spec.file));
            let last = std::cell::Cell::new(0);
            let reported = std::cell::Cell::new(0);
            download(spec, models, &|done, total| {
                let percent = done * 100 / total.max(1);
                if percent > reported.get() {
                    reported.set(percent);
                    progress(done, total);
                }
                if percent >= last.get() + 10 {
                    last.set(percent);
                    status(format!("downloading model {percent}%"));
                }
            })?
        }
    };
    status(format!(
        "loading {}",
        model_path.file_name().unwrap_or_default().to_string_lossy()
    ));
    Ok(Backend::Local {
        model_path,
        threads,
        context_tokens: config.context_tokens,
        idle_unload_secs: config.idle_unload_secs,
    })
}

#[cfg(windows)]
fn main() {
    use std::sync::mpsc::channel;

    use pith_infer::Worker;
    use pith_windows::{Inbound, InsertMode, Options, Source, Waker, run, single_instance};

    let args: Vec<String> = std::env::args().collect();
    let paths = paths();
    init_log(&paths.log);
    let (mut config, config_error) = match Config::load(&paths.config) {
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
    if !single_instance("Local\\PithSingleInstance") {
        log("another instance is running");
        return;
    }
    let (sender, receiver) = channel();
    let waker = Waker::default();
    let prepare_config = config.clone();
    let models = paths.models.clone();
    let (status_sender, status_waker) = (sender.clone(), waker.clone());
    let (progress_sender, progress_waker) = (sender.clone(), waker.clone());
    let (ready_sender, ready_waker) = (sender.clone(), waker.clone());
    let (done_sender, done_waker) = (sender.clone(), waker.clone());
    let worker = Worker::spawn(
        Box::new(move || {
            resolve_backend(
                &prepare_config,
                &models,
                &|text| {
                    let _ = status_sender.send(Inbound::Status(text));
                    status_waker.wake();
                },
                &|done, total| {
                    let _ = progress_sender.send(Inbound::Progress(done, total));
                    progress_waker.wake();
                },
            )
        }),
        Box::new(move |status: Result<String, String>| {
            let _ = ready_sender.send(Inbound::Ready(status));
            ready_waker.wake();
        }),
        Box::new(move |generation, result| {
            let _ = done_sender.send(Inbound::Done(generation, result));
            done_waker.wake();
        }),
    );
    let options = Options {
        source: Source::Focused,
        insert: InsertMode::Paste,
        tray: true,
        poll_ms: 250,
        config_path: Some(paths.config.clone()),
        observer: None,
        global_keys: true,
        onboarding: (!paths.onboarded.exists()).then(|| paths.onboarded.clone()),
        data_dir: Some(paths.data.clone()),
        notice: config_error,
    };
    log("pith starting");
    if let Err(error) = run(config, worker, (sender, receiver), waker, options) {
        log(&format!("fatal: {error}"));
    }
}

#[cfg(not(windows))]
fn main() {
    let _ = (paths, flag, resolve_backend);
    eprintln!("Pith currently runs on Windows; macOS and Linux support is planned.");
}
