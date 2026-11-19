#[path = "../common/mod.rs"]
mod common;
mod input;
mod launch;
mod probe;
mod report;

use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::Duration;

use pith_core::{Config, ModelPreset};
use pith_infer::{Worker, resolve_backend};
use pith_windows::{Inbound, Inbox, InsertMode, Options, Reader, Source, Start, TrayCommand, run};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

use launch::Target;
use probe::{Rig, run_case};
use report::{App, Row, record};

const WORKER_READY_WAIT: Duration = Duration::from_secs(120);
const CI_FLAG: &str = "--ci-runner";
const CI_ENVIRONMENT: [(&str, &str); 3] = [
    ("CI", "true"),
    ("GITHUB_ACTIONS", "true"),
    ("RUNNER_ENVIRONMENT", "github-hosted"),
];
const NOT_INSTALLED: &str = "not installed on this runner";

struct QuitOnDrop(Inbox);

impl Drop for QuitOnDrop {
    fn drop(&mut self) {
        self.0.post(Inbound::Command(TrayCommand::Quit));
    }
}

fn value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1).cloned())
}

fn required(args: &[String], name: &str) -> String {
    value(args, name).unwrap_or_else(|| {
        eprintln!("missing {name}");
        std::process::exit(2)
    })
}

fn require_ci_runner(args: &[String]) {
    let hosted = CI_ENVIRONMENT
        .iter()
        .all(|(name, expected)| std::env::var(name).is_ok_and(|actual| actual == *expected));
    if !hosted || !args.iter().any(|arg| arg == CI_FLAG) {
        eprintln!(
            "apps injects synthetic input and runs only on a GitHub-hosted runner: it needs {CI_FLAG} and the environment {CI_ENVIRONMENT:?}"
        );
        std::process::exit(2);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    require_ci_runner(&args);
    let name = required(&args, "--app");
    let app = App::from_arg(&name).unwrap_or_else(|| {
        eprintln!("unknown app {name}");
        std::process::exit(2)
    });
    let target = Target {
        app,
        exe: PathBuf::from(required(&args, "--exe")),
        fixtures: PathBuf::from(required(&args, "--fixtures")),
    };
    let models = PathBuf::from(required(&args, "--model-dir"));
    let out = PathBuf::from(required(&args, "--out"));
    std::fs::create_dir_all(&out).expect("create output directory");
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    if !target.exe.exists() {
        let mut failed = false;
        for case in app.cases() {
            let mut row = Row::new(app, case);
            if app.optional() {
                row = Row::skipped(app, case, NOT_INSTALLED);
            } else {
                row.fail(NOT_INSTALLED);
            }
            failed |= row.failed();
            record(&out, &row);
        }
        std::process::exit(i32::from(failed));
    }
    let config = Config {
        model: Some(ModelPreset::Small),
        ..Config::default()
    };
    let (inbox, receiver) = Inbox::channel();
    let (ready_tx, ready_rx) = channel();
    let done_inbox = inbox.clone();
    let worker = Worker::spawn(
        config.clone(),
        Box::new(move |config, superseded| {
            resolve_backend(config, ModelPreset::Small, &models, superseded, &|_| {})
        }),
        Box::new(move |status| {
            let _ = ready_tx.send(status);
        }),
        Box::new(move |generation, result| {
            done_inbox.post(Inbound::Done(generation, result));
        }),
    );
    let (observer, actions) = channel();
    let (probe_tx, probed) = channel();
    let probe = Reader::spawn(
        Source::Focused,
        config.clone(),
        Box::new(move |snapshot, served| {
            let _ = probe_tx.send((snapshot, served));
        }),
    );
    let quit = QuitOnDrop(inbox.clone());
    let rig_config = config.clone();
    let driver = std::thread::spawn(move || {
        let _quit = quit;
        let ready = ready_rx.recv_timeout(WORKER_READY_WAIT);
        let mut failures = 0;
        if matches!(ready, Ok(Ok(_))) {
            let mut rig = Rig {
                config: rig_config,
                actions,
                probe,
                probed,
                probe_id: 0,
            };
            for case in app.cases() {
                let row = run_case(&mut rig, &target, case, &out);
                failures += usize::from(row.failed());
                record(&out, &row);
            }
        } else {
            eprintln!("model did not load: {ready:?}");
            failures += 1;
        }
        failures
    });
    let options = Options {
        start: Start::Tray,
        source: Source::Focused,
        insert: InsertMode::Paste,
        tray: false,
        poll_ms: 50,
        config_path: None,
        observer: Some(observer),
        global_keys: true,
        data_dir: None,
        notice: None,
        recommended: ModelPreset::Small,
    };
    run(config, worker, inbox, receiver, options).expect("run");
    let failures = driver.join().unwrap_or(1);
    std::process::exit(i32::from(failures > 0));
}
