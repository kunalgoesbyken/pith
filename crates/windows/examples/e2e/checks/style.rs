use std::time::{Duration, Instant};

use pith_core::{Action, Sample, Setting, StyleStore, sentence_end};
use pith_windows::{DpapiVault, Inbound, KeyAction, UiEvent};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

use crate::common::wide;
use crate::harness::actions::{inserted, requests};
use crate::harness::fixtures::PROSE;
use crate::harness::rig::Rig;
use crate::harness::window::Field;

fn is_learn(action: &Action) -> bool {
    matches!(action, Action::Learn(_))
}

fn learned(seen: &[Action]) -> Vec<&Sample> {
    seen.iter()
        .filter_map(|action| match action {
            Action::Learn(sample) => Some(sample),
            _ => None,
        })
        .collect()
}

fn stored(file: &std::path::Path) -> Option<Vec<Sample>> {
    let store = StyleStore::open(file.to_path_buf(), Box::new(DpapiVault)).ok()?;
    Some(store.memory().entries().to_vec())
}

fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|chunk| chunk == needle.as_bytes())
}

fn accept_in(rig: &mut Rig, field: Field) -> Option<String> {
    rig.suggestion_for(field, &PROSE, 2)?;
    rig.key(KeyAction::AcceptAll);
    let seen = rig.settle(Duration::from_millis(500));
    let accepted = inserted(&seen).concat();
    (!accepted.is_empty()).then_some(accepted)
}

fn learn_one(rig: &mut Rig) -> Option<Sample> {
    let field = rig.a;
    let accepted = accept_in(rig, field)?;
    let finish = if sentence_end(&accepted).is_some() {
        " "
    } else {
        ". "
    };
    rig.type_slowly(field.edit, finish, 30);
    let learned = rig.wait_for(Duration::from_secs(3), is_learn)?.0;
    rig.settle(Duration::from_millis(300));
    match learned {
        Action::Learn(sample) => Some(sample),
        _ => None,
    }
}

fn answer_dialog(title: &'static str) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let title = wide(title);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(dialog) = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) } {
                unsafe {
                    let _ = PostMessageW(
                        Some(dialog),
                        WM_COMMAND,
                        WPARAM(IDYES.0 as usize),
                        LPARAM(0),
                    );
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })
}

pub(crate) fn style_learning(rig: &mut Rig) {
    let field = rig.a;
    let file = rig.data_dir.join("style.bin");
    rig.apply(Setting::StyleMemory(true));
    let sample = learn_one(rig);
    rig.check(
        "accepting a suggestion and finishing the sentence learns it",
        sample.is_some(),
        format!("{sample:?}"),
    );
    let Some(sample) = sample else { return };
    let sentence = sample.sentence.as_str();
    let on_disk = std::fs::read(&file).unwrap_or_default();
    let longest = sentence
        .split_whitespace()
        .max_by_key(|word| word.len())
        .unwrap_or_default();
    rig.check(
        "the store on disk holds no plaintext",
        !on_disk.is_empty()
            && !contains_bytes(&on_disk, sentence)
            && !contains_bytes(&on_disk, longest),
        format!("{} bytes", on_disk.len()),
    );
    rig.check(
        "the store opens again through DPAPI with the sentence in it",
        stored(&file).is_some_and(|entries| entries.contains(&sample)),
        format!("{:?}", stored(&file)),
    );
    rig.fresh(field, "");
    rig.type_slowly(field.edit, PROSE[2], 20);
    let seen = rig.settle(Duration::from_millis(2500));
    let prompts: Vec<&str> = requests(&seen).iter().map(|r| r.prompt.as_str()).collect();
    rig.check(
        "a learned sentence leads the prompt in the same app",
        prompts
            .last()
            .is_some_and(|prompt| prompt.starts_with(sentence)),
        format!("{prompts:?}"),
    );
    let locked = rig.locked;
    rig.fresh(locked, "");
    rig.type_slowly(
        locked.edit,
        "a secret sentence that should never be kept. ",
        15,
    );
    let seen = rig.settle(Duration::from_millis(800));
    rig.check(
        "a password box never learns",
        learned(&seen).is_empty(),
        format!("{seen:?}"),
    );
    rig.use_field(field);
    rig.fresh(field, "");
    let accepted = accept_in(rig, field);
    rig.set_enabled(false);
    rig.type_slowly(field.edit, ". ", 30);
    let seen = rig.settle(Duration::from_millis(1200));
    rig.check(
        "a sentence finished while paused is never learned",
        accepted.is_some() && learned(&seen).is_empty(),
        format!("{accepted:?} {seen:?}"),
    );
    rig.set_enabled(true);
    rig.drain();
    rig.apply(Setting::Block(sample.app.clone()));
    let after_block = stored(&file);
    rig.check(
        "turning Pith off in an app forgets what it learned there",
        after_block.as_ref().is_some_and(Vec::is_empty),
        format!("{after_block:?}"),
    );
    rig.apply(Setting::Unblock(sample.app.clone()));
    rig.resume(sample.app.clone());
    rig.drain();
    let again = learn_one(rig);
    rig.check(
        "learning works again once the app is back on",
        again.is_some() && stored(&file).is_some_and(|entries| !entries.is_empty()),
        format!("{again:?}"),
    );
    let answered = answer_dialog("Forget my style");
    rig.inbox.post(Inbound::Ui(UiEvent::ForgetStyle));
    let _ = answered.join();
    rig.settle(Duration::from_millis(500));
    rig.check(
        "forgetting everything deletes the file",
        !file.exists(),
        format!("{}", file.display()),
    );
    rig.apply(Setting::StyleMemory(false));
    rig.use_field(rig.a);
}
