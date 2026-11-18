use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use pith_core::{Completer, Completion, CompletionRequest};

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
pub type Prepare = Box<dyn Fn() -> Result<Backend, String> + Send>;

const RETRY_AFTER: Duration = Duration::from_secs(30);

pub struct Worker {
    jobs: Sender<Message>,
    latest: Arc<AtomicU64>,
    exited: Receiver<()>,
}

impl Worker {
    pub fn spawn(prepare: Prepare, on_ready: ReadyCallback, on_done: DoneCallback) -> Self {
        let (jobs, rx) = channel();
        let latest = Arc::new(AtomicU64::new(0));
        let shared = latest.clone();
        let (alive, exited) = channel::<()>();
        std::thread::Builder::new()
            .name("pith-infer".into())
            .spawn(move || {
                let _alive = alive;
                work(prepare, rx, shared, on_ready, on_done)
            })
            .expect("spawn inference thread");
        Self {
            jobs,
            latest,
            exited,
        }
    }

    pub fn submit(&self, generation: u64, request: CompletionRequest) {
        self.latest.store(generation, Ordering::SeqCst);
        let _ = self.jobs.send(Message::Job((generation, request)));
    }

    pub fn unload(&self) {
        let _ = self.jobs.send(Message::Unload);
    }

    pub fn cancel(&self) {
        self.latest.store(0, Ordering::SeqCst);
    }

    pub fn shutdown(self, wait: Duration) -> bool {
        self.latest.store(0, Ordering::SeqCst);
        drop(self.jobs);
        matches!(
            self.exited.recv_timeout(wait),
            Err(RecvTimeoutError::Disconnected)
        )
    }
}

type Job = (u64, CompletionRequest);

enum Message {
    Job(Job),
    Unload,
}

enum Exit {
    Unload,
    Closed,
}

fn next_job(rx: &Receiver<Message>) -> Option<Job> {
    loop {
        match rx.recv() {
            Ok(Message::Job(job)) => return Some(job),
            Ok(Message::Unload) => {}
            Err(_) => return None,
        }
    }
}

fn serve(
    completer: &mut dyn Completer,
    rx: &Receiver<Message>,
    latest: &AtomicU64,
    on_done: &DoneCallback,
    idle: Option<Duration>,
    mut pending: Option<Job>,
) -> Exit {
    loop {
        let mut job = match pending.take() {
            Some(job) => job,
            None => {
                let message = match idle {
                    Some(limit) => match rx.recv_timeout(limit) {
                        Ok(message) => message,
                        Err(RecvTimeoutError::Timeout) => return Exit::Unload,
                        Err(RecvTimeoutError::Disconnected) => return Exit::Closed,
                    },
                    None => match rx.recv() {
                        Ok(message) => message,
                        Err(_) => return Exit::Closed,
                    },
                };
                match message {
                    Message::Job(job) => job,
                    Message::Unload => return Exit::Unload,
                }
            }
        };
        let mut unload = false;
        while let Ok(newer) = rx.try_recv() {
            match newer {
                Message::Job(newer) => job = newer,
                Message::Unload => unload = true,
            }
        }
        let (generation, request) = job;
        if latest.load(Ordering::SeqCst) == generation {
            let cancelled = || latest.load(Ordering::SeqCst) != generation;
            let result = completer.complete(&request, &cancelled);
            if !cancelled() {
                on_done(generation, result);
            }
        }
        if unload {
            return Exit::Unload;
        }
    }
}

fn work(
    prepare: Prepare,
    rx: Receiver<Message>,
    latest: Arc<AtomicU64>,
    on_ready: ReadyCallback,
    on_done: DoneCallback,
) {
    let mut pending = None;
    loop {
        let outcome = prepare()
            .and_then(|backend| run(backend, &rx, &latest, &on_ready, &on_done, &mut pending));
        match outcome {
            Ok(()) => return,
            Err(error) => on_ready(Err(error)),
        }
        let failed_at = Instant::now();
        pending = loop {
            match next_job(&rx) {
                Some(job) if failed_at.elapsed() >= RETRY_AFTER => break Some(job),
                Some(_) => {}
                None => return,
            }
        };
    }
}

fn run(
    backend: Backend,
    rx: &Receiver<Message>,
    latest: &AtomicU64,
    on_ready: &ReadyCallback,
    on_done: &DoneCallback,
    pending: &mut Option<Job>,
) -> Result<(), String> {
    match backend {
        Backend::Remote { url, model } => {
            on_ready(Ok("remote server".into()));
            let mut completer = RemoteCompleter::new(&url, model);
            while let Exit::Unload =
                serve(&mut completer, rx, latest, on_done, None, pending.take())
            {}
            Ok(())
        }
        #[cfg(feature = "llama")]
        Backend::Local {
            model_path,
            threads,
            context_tokens,
            idle_unload_secs,
        } => {
            let name = model_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let idle = (idle_unload_secs > 0).then(|| Duration::from_secs(idle_unload_secs));
            loop {
                let handle = crate::LlamaModelHandle::load(&model_path)?;
                let mut session = handle.session(threads, context_tokens)?;
                on_ready(Ok(name.clone()));
                if let Exit::Closed = serve(&mut session, rx, latest, on_done, idle, pending.take())
                {
                    return Ok(());
                }
                drop(session);
                drop(handle);
                on_ready(Ok(format!("{name}, unloaded")));
                match next_job(rx) {
                    Some(job) => *pending = Some(job),
                    None => return Ok(()),
                }
            }
        }
        #[cfg(not(feature = "llama"))]
        Backend::Local { .. } => Err("built without local inference".into()),
    }
}

#[cfg(target_arch = "x86_64")]
pub fn cpu_supported() -> Result<(), String> {
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
pub fn cpu_supported() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(prepare: Prepare) -> Worker {
        Worker::spawn(prepare, Box::new(|_| {}), Box::new(|_, _| {}))
    }

    struct Echo;

    impl Completer for Echo {
        fn complete(
            &mut self,
            request: &CompletionRequest,
            _cancelled: &dyn Fn() -> bool,
        ) -> Result<Completion, String> {
            Ok(Completion {
                text: request.prompt.clone(),
                mean_logprob: 0.0,
                tokens: 1,
                prompt_tokens_evaluated: 0,
                stop: pith_core::StopReason::EndOfText,
            })
        }
    }

    fn request(prompt: &str) -> CompletionRequest {
        CompletionRequest {
            field_id: 1,
            prompt: prompt.into(),
            forced_prefix: String::new(),
            max_tokens: 1,
            max_words: 1,
            min_token_logprob: 0.0,
        }
    }

    fn serve_all(messages: Vec<Message>, done: &Arc<std::sync::Mutex<Vec<String>>>) -> Exit {
        let (tx, rx) = channel();
        for message in messages {
            tx.send(message).unwrap();
        }
        let sink = done.clone();
        let on_done: DoneCallback = Box::new(move |_, result| {
            sink.lock().unwrap().push(result.unwrap().text);
        });
        drop(tx);
        let latest = AtomicU64::new(1);
        serve(&mut Echo, &rx, &latest, &on_done, None, None)
    }

    #[test]
    fn an_unload_request_finishes_the_latest_job_before_ending_the_session() {
        let done = Arc::default();
        let exit = serve_all(
            vec![
                Message::Job((1, request("a"))),
                Message::Unload,
                Message::Job((1, request("b"))),
            ],
            &done,
        );
        assert!(matches!(exit, Exit::Unload));
        assert_eq!(*done.lock().unwrap(), ["b"]);
    }

    #[test]
    fn a_closed_queue_still_ends_the_session_as_closed() {
        let done = Arc::default();
        let exit = serve_all(vec![Message::Job((1, request("a")))], &done);
        assert!(matches!(exit, Exit::Closed));
        assert_eq!(*done.lock().unwrap(), ["a"]);
    }

    #[test]
    fn waiting_for_work_skips_unload_requests() {
        let (tx, rx) = channel();
        tx.send(Message::Unload).unwrap();
        tx.send(Message::Job((7, request("x")))).unwrap();
        assert_eq!(next_job(&rx).map(|(generation, _)| generation), Some(7));
        drop(tx);
        assert!(next_job(&rx).is_none());
    }

    #[test]
    fn shutdown_reports_whether_the_thread_let_go_within_the_wait() {
        let rows = [
            (Duration::ZERO, Duration::from_secs(5), true),
            (Duration::from_millis(400), Duration::from_millis(20), false),
        ];
        for row in rows {
            let (delay, wait, released) = row;
            let worker = worker(Box::new(move || {
                std::thread::sleep(delay);
                Err("no model".into())
            }));
            assert_eq!(worker.shutdown(wait), released, "{row:?}");
        }
    }
}
