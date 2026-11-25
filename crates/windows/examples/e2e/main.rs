use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pith_core::{Config, ModelPreset, TrayCommand};
use pith_infer::{Backend, Worker};
use pith_windows::{Inbound, Inbox, InsertMode, Options, Reader, Source, Start, run};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

use crate::checks::document::{long_document, read_cost};
use crate::checks::keys::{
    accept_is_one_undo_step, accept_word_then_rest, forced_trigger, keys_without_a_suggestion,
    rapid_accept_chain, stale_keys,
};
use crate::checks::language::{prompt_matches_typed_text, reader_fidelity, scripts};
use crate::checks::midline::{midline, midline_crowded, midline_fit, text_after_caret};
use crate::checks::nearby::nearby_context;
use crate::checks::onboarding::onboarding_checks;
use crate::checks::overlay::{
    overlay_anchors_without_a_usable_caret, overlay_at_screen_edge, overlay_geometry,
};
use crate::checks::pause::{auto_pause_on_ignore, pause_and_resume};
use crate::checks::privacy::password_field;
use crate::checks::style::style_learning;
use crate::checks::suggestion::{
    basic_suggestions, caret_moves, escape_then_flicker, multi_line, selection_while_visible,
    switching_fields, typing_through_and_escape,
};
use crate::checks::unload::idle_unload;
use crate::harness::insert::{TARGET, insert_after, insert_now};
use crate::harness::memory::print_memory;
use crate::harness::rig::{Report, Rig, Scenario};
use crate::harness::window::{
    ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_PASSWORD, spawn_form_window, spawn_test_window,
};

mod checks;
#[path = "../common/mod.rs"]
mod common;
mod harness;

use crate::checks::typo;

fn main() {
    let mut args = std::env::args().skip(1);
    let model = PathBuf::from(args.next().expect("model path"));
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let only = std::env::var("PITH_E2E_ONLY").ok();
    let data_dir = std::env::temp_dir().join("pith-e2e-data");
    let _ = std::fs::remove_dir_all(&data_dir);
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    println!("--- onboarding");
    let onboarding_failed = if only.as_deref().is_none_or(|o| "onboarding".contains(o)) {
        onboarding_checks()
    } else {
        0
    };
    print_memory("idle, model not loaded");
    let a = spawn_test_window("Pith e2e target", 60, 60, ES_MULTILINE | ES_AUTOVSCROLL);
    let b = spawn_test_window("Pith e2e second", 60, 290, ES_MULTILINE | ES_AUTOVSCROLL);
    let secret = spawn_test_window("Pith e2e secret", 60, 520, ES_AUTOHSCROLL);
    let locked = spawn_test_window("Pith e2e locked", 60, 750, ES_PASSWORD | ES_AUTOHSCROLL);
    let form = spawn_form_window("Pith e2e form", 1000, 60);
    TARGET.store(a.edit, Ordering::SeqCst);
    let config = Config {
        accept_race_ms: 0,
        ..Config::default()
    };
    let (inbox, receiver) = Inbox::channel();
    let (ready_tx, ready_rx) = channel();
    let (status_tx, statuses) = channel();
    let done_inbox = inbox.clone();
    let completions = Arc::new(Mutex::new(Vec::new()));
    let recorded = completions.clone();
    let threads = config.resolved_threads(
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
    );
    let context_tokens = config.context_tokens;
    let insert_lag_ms: u64 = std::env::var("PITH_E2E_INSERT_LAG_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let idle_unload_secs: u64 = std::env::var("PITH_IDLE_UNLOAD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let worker = Worker::spawn(
        config.clone(),
        Box::new(move |_, _| {
            Ok(Backend::Local {
                model_path: model.clone(),
                threads,
                context_tokens,
                idle_unload_secs,
            })
        }),
        Box::new(move |status| {
            println!("worker status: {status:?}");
            if let Ok(text) = &status {
                let _ = status_tx.send(text.clone());
            }
            let _ = ready_tx.send(());
        }),
        Box::new(move |generation, result| {
            if let Ok(c) = &result {
                println!(
                    "[DONE] g{generation} {:?} logprob {} {:?} eval {}",
                    c.text,
                    c.logprob_label(),
                    c.stop,
                    c.prompt_tokens_evaluated
                );
            }
            recorded.lock().unwrap().push(result.clone());
            done_inbox.post(Inbound::Done(generation, result));
        }),
    );
    let (observer, actions) = channel();
    let (probe_tx, probed) = channel();
    let probe = Reader::spawn(
        Source::Window(&TARGET),
        config.clone(),
        Box::new(move |snapshot, served| {
            let _ = probe_tx.send((snapshot, served));
        }),
    );
    let driver_inbox = inbox.clone();
    let driver_data_dir = data_dir.clone();
    let keymap = config.keymap();
    let driver = std::thread::spawn(move || {
        let _ = ready_rx.recv_timeout(Duration::from_secs(60));
        print_memory("loaded, no request yet");
        let mut rig = Rig {
            a,
            b,
            secret,
            locked,
            form,
            actions,
            inbox: driver_inbox,
            keymap,
            completions,
            statuses,
            probe,
            probed,
            probe_id: 0,
            report: Report {
                passed: 0,
                failed: 0,
            },
            out_dir,
            data_dir: driver_data_dir,
            latencies: Vec::new(),
        };
        let scenarios: [(&str, Scenario); 34] = [
            ("basic", basic_suggestions),
            ("accept", accept_word_then_rest),
            ("undo", accept_is_one_undo_step),
            ("through", typing_through_and_escape),
            ("after", text_after_caret),
            ("midline", midline),
            ("fit", midline_fit),
            ("crowded", midline_crowded),
            ("multiline", multi_line),
            ("fidelity", reader_fidelity),
            ("scripts", scripts),
            ("prompt", prompt_matches_typed_text),
            ("readcost", read_cost),
            ("long", long_document),
            ("chain", rapid_accept_chain),
            ("force", forced_trigger),
            ("nokeys", keys_without_a_suggestion),
            ("stale", stale_keys),
            ("escape", escape_then_flicker),
            ("selection", selection_while_visible),
            ("password", password_field),
            ("nearby", nearby_context),
            ("caret", caret_moves),
            ("pause", pause_and_resume),
            ("switch", switching_fields),
            ("geometry", overlay_geometry),
            ("autopause", auto_pause_on_ignore),
            ("typo", typo::offer_and_apply),
            ("typodismiss", typo::dismiss_and_decline),
            ("typostale", typo::stale_accept),
            ("typopast", typo::typed_past),
            ("typoend", typo::fix_ending_like_the_typo),
            ("typoskip", typo::skips),
            ("style", style_learning),
        ];
        for (name, scenario) in scenarios {
            if only.as_deref().is_some_and(|o| !name.contains(o)) {
                continue;
            }
            println!("--- {name}");
            rig.resume_target();
            scenario(&mut rig);
        }
        if only.as_deref().is_none_or(|o| "edge".contains(o)) {
            println!("--- edge");
            rig.resume_target();
            overlay_at_screen_edge(&mut rig);
        }
        if only.as_deref().is_none_or(|o| "anchor".contains(o)) {
            println!("--- anchor");
            rig.resume_target();
            overlay_anchors_without_a_usable_caret(&mut rig);
        }
        print_memory("loaded, after scenarios");
        if idle_unload_secs > 0 && only.as_deref().is_none_or(|o| "unload".contains(o)) {
            println!("--- unload");
            idle_unload(&mut rig, Duration::from_secs(idle_unload_secs));
        }
        rig.latencies.sort_unstable();
        println!(
            "\nlast keystroke -> ghost text latency (ms): {:?}",
            rig.latencies
        );
        println!(
            "RESULT: {} passed, {} failed",
            rig.report.passed, rig.report.failed
        );
        rig.command(TrayCommand::Quit);
        rig.report.failed
    });
    let options = Options {
        start: Start::Tray,
        source: Source::Window(&TARGET),
        insert: InsertMode::Custom(match insert_lag_ms {
            0 => Box::new(insert_now),
            lag => Box::new(move |text| insert_after(text, Duration::from_millis(lag))),
        }),
        tray: false,
        poll_ms: 50,
        config_path: None,
        observer: Some(observer),
        global_keys: false,
        data_dir: Some(data_dir),
        notice: None,
        recommended: ModelPreset::Standard,
    };
    run(config, worker, inbox, receiver, options).expect("run");
    let failed = driver.join().unwrap_or(1);
    std::process::exit(if failed + onboarding_failed == 0 {
        0
    } else {
        1
    });
}
