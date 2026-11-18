#[cfg(feature = "llama")]
mod llama;
mod models;
mod remote;
mod sampling;
#[cfg(feature = "llama")]
mod slots;
mod worker;

#[cfg(feature = "llama")]
pub use llama::{LlamaModelHandle, LlamaSession};
pub use models::{LITE_MODEL, ModelSpec, STANDARD_MODEL, download, find_local, preset};
pub use remote::RemoteCompleter;
pub use worker::{Backend, DoneCallback, Prepare, ReadyCallback, Worker, cpu_supported};
