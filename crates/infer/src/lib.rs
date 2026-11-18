#[cfg(feature = "llama")]
mod llama;
mod models;
mod remote;
mod resolve;
mod sampling;
#[cfg(feature = "llama")]
mod slots;
mod update;
mod worker;

#[cfg(feature = "llama")]
pub use llama::{LlamaModelHandle, LlamaSession};
pub use models::{
    LARGE_MODEL, ModelSpec, SMALL_MODEL, STANDARD_MODEL, download, find_local, preset,
};
pub use remote::RemoteCompleter;
pub use resolve::{DownloadProgress, Stage, resolve_backend};
pub use update::{Release, UpdateCheck, UpdateFailure, check_for_update};
pub use worker::{Backend, DoneCallback, Prepare, ReadyCallback, Worker, cpu_supported};
