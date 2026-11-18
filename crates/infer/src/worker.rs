use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

use nib_core::{Completer, Completion, CompletionRequest};

use crate::RemoteCompleter;

pub enum Backend {
    Local {
        model_path: PathBuf,
        threads: usize,
        context_tokens: u32,
        idle_unload_secs: u64,
    },
    Remote {
        url: String,
        model: Option<String>,
    },
}

pub type DoneCallback = Box<dyn Fn(u64, Result<Completion, String>) + Send>;
pub type ReadyCallback = Box<dyn Fn(Result<String, String>) + Send>;
pub type Prepare = Box<dyn FnOnce() -> Result<Backend, String> + Send>;

pub struct Worker {
    jobs: Sender<Job>,
    latest: Arc<AtomicU64>,
}

impl Worker {
    pub fn spawn(prepare: Prepare, on_ready: ReadyCallback, on_done: DoneCallback) -> Self {
        let (jobs, rx) = channel();
        let latest = Arc::new(AtomicU64::new(0));
        let shared = latest.clone();
        std::thread::Builder::new()
            .name("nib-infer".into())
            .spawn(move || match prepare() {
                Ok(backend) => run(backend, rx, shared, on_ready, on_done),
                Err(error) => on_ready(Err(error)),
            })
            .expect("spawn inference thread");
        Self { jobs, latest }
    }

    pub fn submit(&self, generation: u64, request: CompletionRequest) {
        self.latest.store(generation, Ordering::SeqCst);
        let _ = self.jobs.send((generation, request));
    }

    pub fn cancel(&self) {
        self.latest.store(0, Ordering::SeqCst);
    }
}

type Job = (u64, CompletionRequest);

enum Exit {
    Idle,
    Closed,
}

fn serve(
    completer: &mut dyn Completer,
    rx: &Receiver<Job>,
    latest: &AtomicU64,
    on_done: &DoneCallback,
    idle: Option<Duration>,
    mut pending: Option<Job>,
) -> Exit {
    loop {
        let mut job = match pending.take() {
            Some(job) => job,
            None => match idle {
                Some(limit) => match rx.recv_timeout(limit) {
                    Ok(job) => job,
                    Err(RecvTimeoutError::Timeout) => return Exit::Idle,
                    Err(RecvTimeoutError::Disconnected) => return Exit::Closed,
                },
                None => match rx.recv() {
                    Ok(job) => job,
                    Err(_) => return Exit::Closed,
                },
            },
        };
        while let Ok(newer) = rx.try_recv() {
            job = newer;
        }
        let (generation, request) = job;
        if latest.load(Ordering::SeqCst) != generation {
            continue;
        }
        let cancelled = || latest.load(Ordering::SeqCst) != generation;
        let result = completer.complete(&request, &cancelled);
        if !cancelled() {
            on_done(generation, result);
        }
    }
}

fn run(
    backend: Backend,
    rx: Receiver<Job>,
    latest: Arc<AtomicU64>,
    on_ready: ReadyCallback,
    on_done: DoneCallback,
) {
    match backend {
        Backend::Remote { url, model } => {
            on_ready(Ok(format!("remote {url}")));
            serve(
                &mut RemoteCompleter::new(&url, model),
                &rx,
                &latest,
                &on_done,
                None,
                None,
            );
        }
        #[cfg(feature = "llama")]
        Backend::Local {
            model_path,
            threads,
            context_tokens,
            idle_unload_secs,
        } => {
            if let Err(missing) = cpu_supported() {
                return on_ready(Err(format!(
                    "this CPU lacks {missing}, which the built-in model runtime needs; set remote_url in settings to use a server instead"
                )));
            }
            let name = model_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let idle = (idle_unload_secs > 0).then(|| Duration::from_secs(idle_unload_secs));
            let mut pending = None;
            loop {
                let handle = match crate::LlamaModelHandle::load(&model_path) {
                    Ok(handle) => handle,
                    Err(error) => return on_ready(Err(error)),
                };
                let mut session = match handle.session(threads, context_tokens) {
                    Ok(session) => session,
                    Err(error) => return on_ready(Err(error)),
                };
                on_ready(Ok(name.clone()));
                if let Exit::Closed =
                    serve(&mut session, &rx, &latest, &on_done, idle, pending.take())
                {
                    return;
                }
                drop(session);
                drop(handle);
                on_ready(Ok(format!("{name}, unloaded while idle")));
                match rx.recv() {
                    Ok(job) => pending = Some(job),
                    Err(_) => return,
                }
            }
        }
        #[cfg(not(feature = "llama"))]
        Backend::Local { .. } => on_ready(Err("built without local inference".into())),
    }
}

#[cfg(target_arch = "x86_64")]
fn cpu_supported() -> Result<(), String> {
    let missing: Vec<&str> = [
        ("AVX", std::arch::is_x86_feature_detected!("avx")),
        ("AVX2", std::arch::is_x86_feature_detected!("avx2")),
        ("FMA", std::arch::is_x86_feature_detected!("fma")),
        ("F16C", std::arch::is_x86_feature_detected!("f16c")),
    ]
    .into_iter()
    .filter(|(_, present)| !present)
    .map(|(name, _)| name)
    .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join("/"))
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn cpu_supported() -> Result<(), String> {
    Ok(())
}
