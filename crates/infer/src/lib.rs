#[cfg(feature = "llama")]
mod llama;
mod models;
mod remote;
mod sampling;
mod worker;

#[cfg(feature = "llama")]
pub use llama::{LlamaModelHandle, LlamaSession};
pub use models::{DEFAULT_MODEL, ModelSpec, download, find_local};
pub use remote::RemoteCompleter;
pub use worker::{Backend, DoneCallback, Prepare, ReadyCallback, Worker};
