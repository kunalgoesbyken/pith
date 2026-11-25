use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pith_core::{Action, Completion, FieldSnapshot, Keymap, PauseSpan, Setting, TrayCommand};
use pith_windows::{Inbound, Inbox, KeyAction, Reader, UiEvent};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::harness::actions::{is_show, last_show, shown_text, trace};
use crate::harness::edit::{select, send_text, set_text};
use crate::harness::insert::TARGET;
use crate::harness::text::word_count;
use crate::harness::window::Field;

pub(crate) struct Report {
    pub(crate) passed: usize,
    pub(crate) failed: usize,
}

impl Report {
    pub(crate) fn check(&mut self, name: &str, ok: bool, detail: String) {
        if ok {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        println!("[{}] {name}: {detail}", if ok { "PASS" } else { "FAIL" });
    }
}

pub(crate) type Scenario = fn(&mut Rig);

pub(crate) struct Rig {
    pub(crate) a: Field,
    pub(crate) b: Field,
    pub(crate) secret: Field,
    pub(crate) locked: Field,
    pub(crate) form: Field,
    pub(crate) actions: Receiver<Action>,
    pub(crate) inbox: Inbox,
    pub(crate) keymap: Keymap,
    pub(crate) completions: Arc<Mutex<Vec<Result<Completion, String>>>>,
    pub(crate) statuses: Receiver<String>,
    pub(crate) probe: Reader,
    pub(crate) probed: Receiver<(Option<FieldSnapshot>, Option<u64>)>,
    pub(crate) probe_id: u64,
    pub(crate) report: Report,
    pub(crate) out_dir: PathBuf,
    pub(crate) data_dir: PathBuf,
    pub(crate) latencies: Vec<u128>,
}

impl Rig {
    pub(crate) fn apply(&self, setting: Setting) {
        self.inbox.post(Inbound::Ui(UiEvent::Apply(setting)));
        self.settle(Duration::from_millis(400));
    }

    pub(crate) fn key(&self, action: KeyAction) {
        let control = unsafe { FindWindowW(w!("PithControl"), None).expect("control window") };
        let id = hotkey_id(&self.keymap, action);
        unsafe {
            PostMessageW(Some(control), WM_HOTKEY, WPARAM(id as usize), LPARAM(0))
                .expect("post hotkey");
        }
    }

    pub(crate) fn command(&self, command: TrayCommand) {
        self.inbox.post(Inbound::Command(command));
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.command(if enabled {
            TrayCommand::Resume
        } else {
            TrayCommand::Pause(PauseSpan::UntilResumed)
        });
    }

    pub(crate) fn use_field(&self, field: Field) {
        TARGET.store(field.edit, Ordering::SeqCst);
    }

    pub(crate) fn resume(&self, app: String) {
        self.command(TrayCommand::ResumeApp(app));
    }

    pub(crate) fn resume_target(&mut self) {
        if let Some(snapshot) = self.read(self.a) {
            self.resume(snapshot.app);
            self.drain();
        }
    }

    pub(crate) fn drain(&self) {
        while let Ok(action) = self.actions.try_recv() {
            trace(&action);
        }
    }

    pub(crate) fn settle(&self, quiet: Duration) -> Vec<Action> {
        let mut seen = Vec::new();
        while let Ok(action) = self.actions.recv_timeout(quiet) {
            trace(&action);
            seen.push(action);
        }
        seen
    }

    pub(crate) fn wait_for(
        &self,
        timeout: Duration,
        predicate: impl Fn(&Action) -> bool,
    ) -> Option<(Action, Duration)> {
        let started = Instant::now();
        while let Some(remaining) = timeout.checked_sub(started.elapsed()) {
            match self.actions.recv_timeout(remaining) {
                Ok(action) if predicate(&action) => return Some((action, started.elapsed())),
                Ok(action) => trace(&action),
                Err(_) => return None,
            }
        }
        None
    }

    pub(crate) fn wait_show(&self, timeout: Duration) -> Option<String> {
        self.wait_for(timeout, is_show).map(|(a, _)| shown_text(&a))
    }

    pub(crate) fn type_slowly(&self, edit: isize, text: &str, gap_ms: u64) {
        for character in text.chars() {
            send_text(edit, &character.to_string());
            std::thread::sleep(Duration::from_millis(gap_ms));
        }
    }

    pub(crate) fn place(&self, field: Field, text: &str, caret: usize) {
        self.use_field(field);
        set_text(field.edit, text);
        select(field.edit, caret);
    }

    pub(crate) fn fresh(&self, field: Field, text: &str) {
        self.fresh_at(field, text, text.encode_utf16().count());
    }

    pub(crate) fn fresh_at(&self, field: Field, text: &str, caret: usize) {
        self.place(field, text, caret);
        std::thread::sleep(Duration::from_millis(350));
        self.drain();
    }

    pub(crate) fn type_and_wait(&self, field: Field, text: &str, gap_ms: u64) -> Option<String> {
        self.fresh(field, "");
        self.type_slowly(field.edit, text, gap_ms);
        self.wait_show(Duration::from_secs(5))
    }

    pub(crate) fn suggestion_for(
        &self,
        field: Field,
        texts: &[&str],
        min_words: usize,
    ) -> Option<(String, String)> {
        for text in texts {
            if let Some(suggestion) = self.type_and_wait(field, text, 10) {
                let settled = self.settle(Duration::from_millis(500));
                let suggestion = last_show(&settled).unwrap_or(suggestion);
                if word_count(&suggestion) >= min_words {
                    return Some((text.to_string(), suggestion));
                }
            }
        }
        None
    }

    pub(crate) fn suggestion_for_long(
        &self,
        field: Field,
        texts: &[&str],
    ) -> Option<(String, String)> {
        for text in texts {
            self.drain();
            self.type_slowly(field.edit, text, 10);
            if let Some(suggestion) = self.wait_show(Duration::from_secs(20)) {
                let settled = self.settle(Duration::from_millis(800));
                return Some((text.to_string(), last_show(&settled).unwrap_or(suggestion)));
            }
        }
        None
    }

    pub(crate) fn read(&mut self, field: Field) -> Option<FieldSnapshot> {
        self.use_field(field);
        self.probe_id += 1;
        self.probe.request(self.probe_id);
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.probed.recv_timeout(remaining) {
                Ok((snapshot, served)) if served == Some(self.probe_id) => return snapshot,
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    pub(crate) fn completions_since(&self, from: usize) -> Vec<Completion> {
        self.completions.lock().unwrap()[from..]
            .iter()
            .filter_map(|r| r.as_ref().ok().cloned())
            .collect()
    }

    pub(crate) fn completion_count(&self) -> usize {
        self.completions.lock().unwrap().len()
    }

    pub(crate) fn check(&mut self, name: &str, ok: bool, detail: String) {
        self.report.check(name, ok, detail);
    }
}

fn hotkey_id(keymap: &Keymap, action: KeyAction) -> i32 {
    let accept_all = keymap.accept_all.len();
    let accept_word = keymap.accept_word.len();
    let before = match action {
        KeyAction::AcceptAll => 0,
        KeyAction::AcceptWord => accept_all,
        KeyAction::Dismiss => accept_all + accept_word,
        KeyAction::Trigger => accept_all + accept_word + 1,
    };
    before as i32 + 1
}
