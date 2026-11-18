use std::cell::Cell;
use std::path::{Path, PathBuf};

use pith_core::{Config, ModelPreset};

use crate::models::{ModelSpec, download, find_local, preset};
use crate::worker::{Backend, cpu_supported};

const FALLBACK_CORES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub done: u64,
    pub total: u64,
}

impl DownloadProgress {
    pub fn percent(self) -> u64 {
        self.done * 100 / self.total.max(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Downloading(DownloadProgress),
    Loading { file: String },
}

fn download_model(
    spec: &ModelSpec,
    models: &Path,
    superseded: &dyn Fn() -> bool,
    on_stage: &dyn Fn(Stage),
) -> Result<PathBuf, String> {
    let reported = Cell::new(0);
    on_stage(Stage::Downloading(DownloadProgress {
        done: 0,
        total: spec.bytes,
    }));
    download(spec, models, superseded, &|done, total| {
        let progress = DownloadProgress { done, total };
        if progress.percent() > reported.get() {
            reported.set(progress.percent());
            on_stage(Stage::Downloading(progress));
        }
    })
}

pub fn resolve_backend(
    config: &Config,
    recommended: ModelPreset,
    models: &Path,
    superseded: &dyn Fn() -> bool,
    on_stage: &dyn Fn(Stage),
) -> Result<Backend, String> {
    if let Some(url) = config.remote_url.clone() {
        return Ok(Backend::Remote {
            url,
            model: config.remote_model.clone(),
        });
    }
    cpu_supported().map_err(|missing| {
        format!("this CPU lacks {missing}, which the built-in model needs; set remote_url in settings to use a server instead")
    })?;
    let spec = preset(config.preset(recommended));
    let model_path = match config
        .model_path
        .clone()
        .or_else(|| find_local(models, spec))
    {
        Some(path) => path,
        None => download_model(spec, models, superseded, on_stage)?,
    };
    on_stage(Stage::Loading {
        file: model_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    });
    let cores = std::thread::available_parallelism().map_or(FALLBACK_CORES, |cores| cores.get());
    Ok(Backend::Local {
        model_path,
        threads: config.resolved_threads(cores),
        context_tokens: config.context_tokens,
        idle_unload_secs: config.idle_unload_secs,
    })
}
