use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pith_core::{Action, Completion, CompletionRequest, Config, FieldSnapshot};
use pith_infer::{Backend, Worker};
use pith_windows::{
    ACCEPT_ALL, ACCEPT_WORD, DISMISS, Inbound, InsertMode, Onboarding, OnboardingEvent,
    OnboardingStep, Options, Outcome, Reader, Source, Waker, run,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::{
    EM_GETFIRSTVISIBLELINE, EM_LINEFROMCHAR, EM_LINESCROLL, EM_POSFROMCHAR, EM_REPLACESEL,
    EM_SETSEL, PBM_GETPOS,
};
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

static TARGET: AtomicIsize = AtomicIsize::new(0);

const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_PASSWORD: u32 = 0x0020;
const ES_AUTOHSCROLL: u32 = 0x0080;
const LOOKBACK_CHARS: usize = 19_200;
const MIN_SAMPLES: usize = 5;
const MAX_STEADY_COST: usize = 4;

const PROSE: [&str; 6] = [
    "Hi Sarah, thanks for sending the revised contract. I reviewed the payment terms and they look",
    "Quick update on the landing page: the new hero section is live on staging and I would love your",
    "Please let me know if you have any",
    "Thanks again for the quick turnaround on the budget review, we should schedule the follow up for",
    "The release notes are ready and I just need someone to double check the migration steps before",
    "Could you send me the latest version of the proposal so that I can share it with the",
];

const LANGUAGES: [(&str, &str); 5] = [
    ("japanese", "今日は天気が良いので、公園に"),
    ("hindi", "मुझे कल सुबह दफ्तर जाना है क्योंकि"),
    ("arabic", "أريد أن أشكرك على مساعدتك في"),
    ("emoji", "Let's celebrate 🎉🎉 the launch with"),
    ("mixed", "Meeting notes: आज की बैठक में हमने budget पर"),
];

#[derive(Clone, Copy)]
struct Field {
    host: isize,
    edit: isize,
}

unsafe extern "system" fn host_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn spawn_test_window(title: &str, left: i32, top: i32, edit_style: u32) -> Field {
    let (tx, rx) = channel();
    let title = wide(title);
    std::thread::spawn(move || unsafe {
        let _ = LoadLibraryW(w!("Msftedit.dll"));
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("PithE2EHost");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(host_procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let host = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            left,
            top,
            900,
            200,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RICHEDIT50W"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(edit_style),
            10,
            10,
            860,
            130,
            Some(host),
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let _ = ShowWindow(host, SW_SHOWNOACTIVATE);
        tx.send(Field {
            host: host.0 as isize,
            edit: edit.0 as isize,
        })
        .unwrap();
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    rx.recv().unwrap()
}

fn send(edit: isize, message: u32, wparam: usize, lparam: isize) -> isize {
    unsafe {
        SendMessageW(
            HWND(edit as _),
            message,
            Some(WPARAM(wparam)),
            Some(LPARAM(lparam)),
        )
        .0
    }
}

fn scroll_to_caret(edit: isize) {
    let line = send(edit, EM_LINEFROMCHAR, usize::MAX, 0);
    let first = send(edit, EM_GETFIRSTVISIBLELINE, 0, 0);
    send(edit, EM_LINESCROLL, 0, line - first);
}

fn send_text(edit: isize, text: &str) {
    let text = wide(text);
    send(edit, EM_REPLACESEL, 1, text.as_ptr() as isize);
    scroll_to_caret(edit);
}

fn text_length(edit: isize) -> usize {
    send(edit, WM_GETTEXTLENGTH, 0, 0) as usize
}

fn select_range(edit: isize, start: usize, end: usize) {
    send(edit, EM_SETSEL, start, end as isize);
    scroll_to_caret(edit);
}

fn select(edit: isize, at: usize) {
    select_range(edit, at, at);
}

fn select_end(edit: isize) {
    select(edit, text_length(edit));
}

fn set_text(edit: isize, text: &str) {
    let text = wide(text);
    send(edit, WM_SETTEXT, 0, text.as_ptr() as isize);
    select_end(edit);
}

fn get_text(edit: isize) -> String {
    let mut buffer = vec![0u16; text_length(edit) + 1];
    let length = send(edit, WM_GETTEXT, buffer.len(), buffer.as_mut_ptr() as isize);
    String::from_utf16_lossy(&buffer[..length as usize]).replace('\r', "\n")
}

fn caret_screen(edit: isize, index: usize) -> (i32, i32) {
    let mut point = POINT::default();
    send(
        edit,
        EM_POSFROMCHAR,
        &mut point as *mut POINT as usize,
        index as isize,
    );
    unsafe {
        MapWindowPoints(
            Some(HWND(edit as _)),
            None,
            std::slice::from_mut(&mut point),
        );
    }
    (point.x, point.y)
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    rect
}

fn monitor_of(hwnd: HWND) -> HMONITOR {
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) }
}

fn monitor_rect(monitor: HMONITOR) -> RECT {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetMonitorInfoW(monitor, &mut info);
    }
    info.rcWork
}

fn move_host(field: Field, left: i32, top: i32) {
    unsafe {
        let _ = SetWindowPos(
            HWND(field.host as _),
            None,
            left,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

fn overlay_window() -> Option<HWND> {
    unsafe { FindWindowW(w!("PithGhostText"), None).ok() }
}

fn overlay_visible() -> bool {
    overlay_window().is_some_and(|hwnd| unsafe { IsWindowVisible(hwnd).as_bool() })
}

fn screenshot(host: isize, path: &PathBuf) {
    unsafe {
        let rect = window_rect(HWND(host as _));
        let (x, y, width, height) = (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap.into());
        let _ = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        );
        SelectObject(memory, previous);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        let mut file = Vec::with_capacity(54 + pixels.len());
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&54u32.to_le_bytes());
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&width.to_le_bytes());
        file.extend_from_slice(&height.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&[0; 24]);
        file.extend_from_slice(&pixels);
        let _ = std::fs::write(path, file);
    }
}

struct Memory {
    working_mb: usize,
    peak_mb: usize,
    private_mb: usize,
}

fn memory() -> Memory {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
    }
    Memory {
        working_mb: counters.WorkingSetSize >> 20,
        peak_mb: counters.PeakWorkingSetSize >> 20,
        private_mb: counters.PagefileUsage >> 20,
    }
}

fn print_memory(stage: &str) {
    let m = memory();
    println!(
        "[MEM] {stage}: working set {} MB, peak {} MB, private {} MB",
        m.working_mb, m.peak_mb, m.private_mb
    );
}

fn tail(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars[chars.len().saturating_sub(40)..].iter().collect()
}

fn trace(action: &Action) {
    if std::env::var("PITH_E2E_TRACE").is_ok() {
        println!(
            "[TRACE] {}",
            format!("{action:?}").chars().take(200).collect::<String>()
        );
    }
}

fn shown_text(action: &Action) -> String {
    match action {
        Action::Show { text, .. } => text.clone(),
        _ => String::new(),
    }
}

fn is_show(action: &Action) -> bool {
    matches!(action, Action::Show { .. })
}

fn is_hide(action: &Action) -> bool {
    matches!(action, Action::Hide)
}

fn is_request(action: &Action) -> bool {
    matches!(action, Action::StartCompletion { .. })
}

fn inserted(seen: &[Action]) -> Vec<String> {
    seen.iter()
        .filter_map(|a| match a {
            Action::Insert(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn last_show(seen: &[Action]) -> Option<String> {
    seen.iter().rev().find(|a| is_show(a)).map(shown_text)
}

fn requests(seen: &[Action]) -> Vec<&CompletionRequest> {
    seen.iter()
        .filter_map(|a| match a {
            Action::StartCompletion { request, .. } => Some(request),
            _ => None,
        })
        .collect()
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

struct Report {
    passed: usize,
    failed: usize,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: String) {
        if ok {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        println!("[{}] {name}: {detail}", if ok { "PASS" } else { "FAIL" });
    }
}

type Scenario = fn(&mut Rig);

struct Rig {
    a: Field,
    b: Field,
    secret: Field,
    locked: Field,
    actions: Receiver<Action>,
    inbound: Sender<Inbound>,
    waker: Waker,
    completions: Arc<Mutex<Vec<Result<Completion, String>>>>,
    statuses: Receiver<String>,
    probe: Reader,
    probed: Receiver<(Option<FieldSnapshot>, u64)>,
    probe_id: u64,
    report: Report,
    out_dir: PathBuf,
    latencies: Vec<u128>,
}

impl Rig {
    fn key(&self, id: i32) {
        let _ = self.inbound.send(Inbound::Key(id));
        self.waker.wake();
    }

    fn set_enabled(&self, enabled: bool) {
        let _ = self.inbound.send(Inbound::SetEnabled(enabled));
        self.waker.wake();
    }

    fn use_field(&self, field: Field) {
        TARGET.store(field.edit, Ordering::SeqCst);
    }

    fn drain(&self) {
        while let Ok(action) = self.actions.try_recv() {
            trace(&action);
        }
    }

    fn settle(&self, quiet: Duration) -> Vec<Action> {
        let mut seen = Vec::new();
        while let Ok(action) = self.actions.recv_timeout(quiet) {
            trace(&action);
            seen.push(action);
        }
        seen
    }

    fn wait_for(
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

    fn wait_show(&self, timeout: Duration) -> Option<String> {
        self.wait_for(timeout, is_show).map(|(a, _)| shown_text(&a))
    }

    fn type_slowly(&self, edit: isize, text: &str, gap_ms: u64) {
        for character in text.chars() {
            send_text(edit, &character.to_string());
            std::thread::sleep(Duration::from_millis(gap_ms));
        }
    }

    fn fresh(&self, field: Field, text: &str) {
        self.use_field(field);
        set_text(field.edit, text);
        std::thread::sleep(Duration::from_millis(350));
        self.drain();
    }

    fn type_and_wait(&self, field: Field, text: &str, gap_ms: u64) -> Option<String> {
        self.fresh(field, "");
        self.type_slowly(field.edit, text, gap_ms);
        self.wait_show(Duration::from_secs(5))
    }

    fn suggestion_for(
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

    fn suggestion_for_long(&self, field: Field, texts: &[&str]) -> Option<(String, String)> {
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

    fn read(&mut self, field: Field) -> Option<FieldSnapshot> {
        self.use_field(field);
        self.probe_id += 1;
        self.probe.request(self.probe_id);
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.probed.recv_timeout(remaining) {
                Ok((snapshot, served)) if served == self.probe_id => return snapshot,
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    fn completions_since(&self, from: usize) -> Vec<Completion> {
        self.completions.lock().unwrap()[from..]
            .iter()
            .filter_map(|r| r.as_ref().ok().cloned())
            .collect()
    }

    fn completion_count(&self) -> usize {
        self.completions.lock().unwrap().len()
    }

    fn check(&mut self, name: &str, ok: bool, detail: String) {
        self.report.check(name, ok, detail);
    }
}

fn basic_suggestions(rig: &mut Rig) {
    let field = rig.a;
    for (index, sentence) in PROSE.iter().take(3).enumerate() {
        let shown = rig.type_and_wait(field, sentence, 25);
        rig.check(
            &format!("basic {index} shows a suggestion"),
            shown.is_some(),
            format!("{shown:?}"),
        );
        if index == 0 && shown.is_some() {
            std::thread::sleep(Duration::from_millis(150));
            screenshot(field.host, &rig.out_dir.join("e2e-ghost.bmp"));
        }
    }
    let started = Instant::now();
    rig.fresh(field, "");
    rig.type_slowly(field.edit, PROSE[1], 25);
    let typed = started.elapsed();
    let shown = rig.wait_for(Duration::from_secs(4), is_show);
    if let Some((_, latency)) = shown {
        rig.latencies.push(latency.as_millis());
    }
    println!(
        "[INFO] typed in {typed:?}, ghost after {:?}",
        shown.map(|s| s.1)
    );
}

fn accept_word_then_rest(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("accept flow", false, "no suggestion".into());
    };
    let before = get_text(field.edit);
    rig.key(ACCEPT_WORD);
    let seen = rig.settle(Duration::from_millis(500));
    let words = inserted(&seen);
    let word = words.first().cloned().unwrap_or_default();
    let text = get_text(field.edit);
    rig.check(
        "accept word inserts the next word",
        words.len() == 1 && text == format!("{before}{word}") && suggestion.starts_with(&word),
        format!(
            "suggestion {suggestion:?}, inserted {words:?}, tail {:?}",
            tail(&text)
        ),
    );
    let rest = suggestion[word.len().min(suggestion.len())..].to_string();
    if rest.trim().is_empty() {
        rig.check(
            "single-word suggestion is fully consumed by accept word",
            seen.iter().any(is_hide),
            format!("{seen:?}"),
        );
        return;
    }
    rig.check(
        "rest stays visible after accept word",
        last_show(&seen).as_deref() == Some(rest.as_str()) && !seen.iter().any(is_request),
        format!("rest {rest:?}, shown {:?}", last_show(&seen)),
    );
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(500));
    let text = get_text(field.edit);
    rig.check(
        "accept all inserts the rest",
        inserted(&seen).first() == Some(&rest) && text == format!("{before}{suggestion}"),
        format!("rest {rest:?}, tail {:?}", tail(&text)),
    );
}

fn typing_through_and_escape(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check(
            "typing-through flow",
            false,
            "no multi-char suggestion".into(),
        );
    };
    let typed: String = suggestion.chars().take(2).collect();
    rig.type_slowly(field.edit, &typed, 30);
    let seen = rig.settle(Duration::from_millis(500));
    let expected = suggestion.strip_prefix(typed.as_str()).map(str::to_string);
    rig.check(
        "typing through shrinks the suggestion without a new request",
        !seen.iter().any(is_request) && last_show(&seen) == expected,
        format!(
            "suggestion {suggestion:?}, typed {typed:?}, last {:?}",
            last_show(&seen)
        ),
    );
    rig.key(DISMISS);
    let seen = rig.settle(Duration::from_millis(900));
    rig.check(
        "escape dismisses and stays dismissed",
        seen.iter().any(is_hide) && !seen.iter().any(is_show) && !overlay_visible(),
        format!("{seen:?}"),
    );
}

fn text_after_caret(rig: &mut Rig) {
    let field = rig.a;
    rig.fresh(field, "We should schedule the review for next week");
    select(field.edit, 10);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "no suggestion when text follows the caret",
        !seen.iter().any(is_show) && !seen.iter().any(is_request),
        format!("{seen:?}"),
    );
    rig.fresh(field, "We should schedule the review for next week   ");
    select(field.edit, 43);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "caret before trailing spaces still suggests",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
    rig.fresh(
        field,
        "We should schedule the review for\rnext week is fine",
    );
    select(field.edit, 33);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "caret at the end of a line with text on the next line still suggests",
        requests(&seen)
            .first()
            .is_some_and(|r| r.prompt.ends_with("review") && r.forced_prefix == " for"),
        format!(
            "{:?}",
            requests(&seen)
                .iter()
                .map(|r| r.prompt.clone())
                .collect::<Vec<_>>()
        ),
    );
}

fn multi_line(rig: &mut Rig) {
    let field = rig.a;
    rig.fresh(field, "");
    rig.type_slowly(
        field.edit,
        "Dear team,\r\rThe quarterly numbers are in and the headline is that we",
        10,
    );
    let seen = rig.settle(Duration::from_millis(1200));
    let asked = requests(&seen)
        .last()
        .map(|r| r.prompt.clone())
        .unwrap_or_default();
    rig.check(
        "multi-line prompt carries earlier lines",
        asked.contains("Dear team,\n\nThe quarterly numbers"),
        format!("{asked:?}"),
    );
    println!("[INFO] multi-line suggestion: {:?}", last_show(&seen));
    rig.fresh(field, "First line is long enough.\rOk");
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "short fresh line does not request",
        !seen.iter().any(is_request),
        format!("{seen:?}"),
    );
    rig.type_slowly(field.edit, " then", 20);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "line grows past the minimum and requests",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
}

fn reader_fidelity(rig: &mut Rig) {
    let field = rig.a;
    for (name, text) in LANGUAGES {
        rig.fresh(field, text);
        let snapshot = rig.read(field);
        let before = snapshot
            .as_ref()
            .map(|s| s.before.clone())
            .unwrap_or_default();
        rig.check(
            &format!("reader returns {name} text exactly"),
            before == text,
            format!("{before:?}"),
        );
    }
    rig.fresh(field, "line one\rline two\r");
    let snapshot = rig.read(field);
    rig.check(
        "reader normalizes paragraph marks",
        snapshot
            .as_ref()
            .is_some_and(|s| s.before == "line one\nline two\n"),
        format!("{:?}", snapshot.map(|s| s.before)),
    );
}

fn scripts(rig: &mut Rig) {
    let field = rig.a;
    for (name, text) in LANGUAGES {
        let shown = rig.type_and_wait(field, text, 15);
        let seen = rig.settle(Duration::from_millis(400));
        let suggestion = last_show(&seen).or(shown);
        println!("[INFO] {name}: model suggested {suggestion:?}");
        let Some(suggestion) = suggestion else {
            continue;
        };
        let before = get_text(field.edit);
        rig.key(ACCEPT_ALL);
        let seen = rig.settle(Duration::from_millis(500));
        let after = get_text(field.edit);
        rig.check(
            &format!("{name} accept inserts exactly the suggestion"),
            after == format!("{before}{suggestion}") && inserted(&seen) == vec![suggestion.clone()],
            format!("{:?}", tail(&after)),
        );
    }
}

fn prompt_matches_typed_text(rig: &mut Rig) {
    let field = rig.a;
    for (name, text) in LANGUAGES {
        rig.fresh(field, "");
        rig.type_slowly(field.edit, text, 10);
        let seen = rig.settle(Duration::from_millis(1200));
        let Some(request) = requests(&seen).last().map(|r| (*r).clone()) else {
            rig.check(&format!("{name} prompt"), false, "no request".into());
            continue;
        };
        let rebuilt = format!(
            "{}{}",
            request
                .prompt
                .strip_prefix("Pith e2e target\n\n")
                .unwrap_or(&request.prompt),
            request.forced_prefix
        );
        rig.check(
            &format!("{name} prompt plus forced prefix equals the typed text"),
            rebuilt == text,
            format!("{rebuilt:?}"),
        );
    }
}

fn steady_cost(costs: &[usize]) -> usize {
    let steady = costs.get(1..).unwrap_or_default();
    steady.iter().sum::<usize>().div_ceil(steady.len().max(1))
}

fn long_document_text() -> String {
    let paragraph = "The team reviewed the budget and agreed that the schedule needs another week before the launch can go ahead, so please plan accordingly. ";
    (0..40)
        .map(|n| {
            format!(
                "Item {n}: {}{}\r",
                paragraph.repeat(4),
                "Next steps are listed below."
            )
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn keystroke_costs(rig: &Rig, field: Field, text: &str) -> Vec<usize> {
    let mut costs = Vec::new();
    for character in text.chars() {
        let mark = rig.completion_count();
        send_text(field.edit, &character.to_string());
        let deadline = Instant::now() + Duration::from_secs(4);
        while rig.completion_count() == mark && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(done) = rig.completions_since(mark).first() {
            costs.push(done.prompt_tokens_evaluated);
        }
    }
    costs
}

fn long_document(rig: &mut Rig) {
    let field = rig.a;
    let probe_text = " the quick brown fox jumps over the lazy dog";
    rig.fresh(
        field,
        "Hello team, here is a short note that I am writing today.",
    );
    let short_costs = keystroke_costs(rig, field, probe_text);
    let document = long_document_text();
    let normalized = document.replace('\r', "\n");
    println!(
        "[INFO] long document is {} chars",
        normalized.chars().count()
    );
    rig.fresh(field, &document);
    let snapshot = rig.read(field);
    let before = snapshot
        .as_ref()
        .map(|s| s.before.clone())
        .unwrap_or_default();
    println!(
        "[INFO] long document caret {:?}",
        snapshot.as_ref().map(|s| s.caret)
    );
    rig.check(
        "long document read is a bounded suffix of the document",
        normalized.ends_with(&before)
            && before.chars().count() <= LOOKBACK_CHARS
            && before.chars().count() > 1000,
        format!(
            "{} chars read of {}",
            before.chars().count(),
            normalized.chars().count()
        ),
    );
    let long_costs = keystroke_costs(rig, field, probe_text);
    let (short_mean, long_mean) = (steady_cost(&short_costs), steady_cost(&long_costs));
    rig.check(
        "typing at the end of a long document stays incremental",
        short_costs.len() >= MIN_SAMPLES && long_costs.len() >= MIN_SAMPLES && long_mean <= MAX_STEADY_COST,
        format!("prompt tokens re-evaluated per keystroke after the first: short {short_costs:?} (mean {short_mean}), long {long_costs:?} (mean {long_mean})"),
    );

    let middle = normalized
        .match_indices('\n')
        .nth(20)
        .map(|(i, _)| i)
        .unwrap_or(0);
    select(field.edit, normalized[..middle].encode_utf16().count());
    rig.drain();
    let middle_costs = keystroke_costs(rig, field, " Also remember to");
    let seen = rig.settle(Duration::from_millis(300));
    let middle_mean = steady_cost(&middle_costs);
    rig.check(
        "typing in the middle of a long document stays incremental",
        middle_costs.len() >= MIN_SAMPLES && middle_mean <= MAX_STEADY_COST,
        format!(
            "{middle_costs:?} (mean {middle_mean}), last shown {:?}",
            last_show(&seen)
        ),
    );

    rig.fresh(field, &document);
    let candidates = [
        " Thanks again for the quick turnaround on the",
        " Please let me know if you have any",
        " Quick update on the landing page: the new hero section is live on staging and I would love your",
    ];
    println!(
        "[INFO] caret before the accept flow {:?}",
        rig.read(field).map(|s| s.caret)
    );
    let Some((_, suggestion)) = rig.suggestion_for_long(field, &candidates) else {
        return rig.check("long document accept flow", false, "no suggestion".into());
    };
    let text_before = get_text(field.edit);
    rig.key(ACCEPT_WORD);
    let seen = rig.settle(Duration::from_millis(1500));
    let word = inserted(&seen).first().cloned().unwrap_or_default();
    let rest = suggestion[word.len().min(suggestion.len())..].to_string();
    rig.check(
        "long document: accept word keeps the rest visible without a new request",
        rest.trim().is_empty()
            || (last_show(&seen).as_deref() == Some(rest.as_str()) && !seen.iter().any(is_request)),
        format!(
            "suggestion {suggestion:?}, word {word:?}, shown {:?}, requests {}",
            last_show(&seen),
            requests(&seen).len()
        ),
    );
    rig.check(
        "long document: accepted word reached the field",
        get_text(field.edit) == format!("{text_before}{word}"),
        format!("{:?}", tail(&get_text(field.edit))),
    );
    if !rest.trim().is_empty() && last_show(&seen).as_deref() == Some(rest.as_str()) {
        let typed: String = rest.chars().take(2).collect();
        rig.type_slowly(field.edit, &typed, 30);
        let seen = rig.settle(Duration::from_millis(1500));
        rig.check(
            "long document: typing through the suggestion shrinks it without a new request",
            !seen.iter().any(is_request)
                && last_show(&seen).as_deref() == rest.strip_prefix(typed.as_str()),
            format!(
                "rest {rest:?}, typed {typed:?}, shown {:?}, requests {}",
                last_show(&seen),
                requests(&seen).len()
            ),
        );
    }
}

fn read_cost(rig: &mut Rig) {
    let field = rig.a;
    let measure = |rig: &mut Rig, name: &str, text: &str| {
        rig.fresh(field, text);
        let mut times = Vec::new();
        let mut failures = 0;
        for _ in 0..12 {
            let started = Instant::now();
            match rig.read(field) {
                Some(s) if !s.before.is_empty() => times.push(started.elapsed().as_millis()),
                _ => failures += 1,
            }
        }
        times.sort_unstable();
        println!(
            "[INFO] read of {name}: {} ok, {failures} empty or failed, times ms {times:?}",
            times.len()
        );
        (times.len(), failures)
    };
    let (_, short_failures) = measure(
        rig,
        "a short field",
        "Hello team, here is a short note that I am writing today.",
    );
    let (long_ok, long_failures) = measure(rig, "a 23000 char document", &long_document_text());
    rig.check(
        "every read of a long document returns its text",
        short_failures == 0 && long_failures == 0 && long_ok == 12,
        format!("short failures {short_failures}, long failures {long_failures}"),
    );
}

fn rapid_accept_chain(rig: &mut Rig) {
    let field = rig.a;
    let Some((typed, suggestion)) = rig.suggestion_for(field, &PROSE, 3) else {
        return rig.check(
            "accept chain setup",
            false,
            "no suggestion with 3+ words".into(),
        );
    };
    let base = get_text(field.edit);
    let words = {
        let mut rest = suggestion.as_str();
        let mut count = 0;
        while !rest.trim().is_empty() {
            let word = pith_core::next_word(rest);
            rest = &rest[word.len()..];
            count += 1;
        }
        count
    };
    rig.drain();
    let gap = std::env::var("PITH_E2E_KEY_GAP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for _ in 0..words + 3 {
        rig.key(ACCEPT_WORD);
        std::thread::sleep(Duration::from_millis(gap));
    }
    let seen = rig.settle(Duration::from_millis(700));
    let text = get_text(field.edit);
    rig.check(
        "rapid accept-word chain inserts the suggestion once, in order",
        text == format!("{base}{suggestion}") && inserted(&seen).concat() == suggestion,
        format!(
            "typed {typed:?}, suggestion {suggestion:?}, inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&text)
        ),
    );
}

fn stale_keys(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    let base = get_text(field.edit);
    send_text(field.edit, "z");
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after a diverging keystroke never pastes the stale suggestion",
        inserted(&seen).is_empty() && get_text(field.edit) == format!("{base}z"),
        format!(
            "suggestion {suggestion:?}, inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&get_text(field.edit))
        ),
    );

    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    let base = get_text(field.edit);
    let first: String = suggestion.chars().take(1).collect();
    send_text(field.edit, &first);
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after typing the first suggested character completes without doubling",
        get_text(field.edit) == format!("{base}{suggestion}"),
        format!(
            "suggestion {suggestion:?}, inserts {:?}, tail {:?}",
            inserted(&seen),
            tail(&get_text(field.edit))
        ),
    );

    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    set_text(
        field.edit,
        "Completely different text that the model never saw",
    );
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after the text was replaced never pastes the stale suggestion",
        inserted(&seen).is_empty()
            && get_text(field.edit) == "Completely different text that the model never saw",
        format!("suggestion {suggestion:?}, inserts {:?}", inserted(&seen)),
    );

    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 2) else {
        return rig.check("stale key setup", false, "no suggestion".into());
    };
    let base = get_text(field.edit);
    select_range(field.edit, 0, 5);
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(700));
    rig.check(
        "tab right after selecting text never pastes over the selection",
        inserted(&seen).is_empty() && get_text(field.edit) == base,
        format!("suggestion {suggestion:?}, inserts {:?}", inserted(&seen)),
    );
}

fn escape_then_flicker(rig: &mut Rig) {
    let field = rig.a;
    let Some((typed, _)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("escape setup", false, "no suggestion".into());
    };
    rig.key(DISMISS);
    rig.settle(Duration::from_millis(300));
    select(field.edit, 0);
    std::thread::sleep(Duration::from_millis(250));
    select_end(field.edit);
    std::thread::sleep(Duration::from_millis(250));
    select(field.edit, 3);
    std::thread::sleep(Duration::from_millis(150));
    select_end(field.edit);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "escape holds through caret flicker",
        !seen.iter().any(is_show) && !seen.iter().any(is_request) && !overlay_visible(),
        format!("typed {typed:?}, actions {seen:?}"),
    );
    rig.type_slowly(field.edit, " and", 30);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "new typing after escape and flicker suggests again",
        seen.iter().any(is_request),
        format!(
            "{} requests, shown {:?}",
            requests(&seen).len(),
            last_show(&seen)
        ),
    );
}

fn selection_while_visible(rig: &mut Rig) {
    let field = rig.a;
    let Some(_) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("selection setup", false, "no suggestion".into());
    };
    let length = text_length(field.edit);
    select_range(field.edit, length - 5, length);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "selecting text hides the ghost and requests nothing",
        seen.iter().any(is_hide)
            && !seen.iter().any(is_show)
            && !seen.iter().any(is_request)
            && !overlay_visible(),
        format!("{seen:?}"),
    );
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "tab with a selection inserts nothing",
        inserted(&seen).is_empty(),
        format!("{:?}", inserted(&seen)),
    );
    select_end(field.edit);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "collapsing the selection suggests again",
        seen.iter().any(is_request),
        format!("{} requests", requests(&seen).len()),
    );
}

fn password_field(rig: &mut Rig) {
    let secret = "my very secret passphrase is hunter two";
    let plain = rig.secret;
    rig.fresh(plain, secret);
    let snapshot = rig.read(plain);
    rig.check(
        "control: a single-line control without the password style is read",
        snapshot
            .as_ref()
            .is_some_and(|s| !s.is_password && s.before == secret),
        format!("{:?}", snapshot.map(|s| s.before)),
    );
    let field = rig.locked;
    rig.fresh(field, secret);
    let snapshot = rig.read(field);
    rig.check(
        "password field text is never read",
        snapshot
            .as_ref()
            .is_none_or(|s| s.is_password && s.before.is_empty() && s.after.is_empty()),
        format!("{snapshot:?}"),
    );
    rig.drain();
    rig.type_slowly(field.edit, " and more words to type", 20);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "password field never requests or shows",
        !seen.iter().any(is_request) && !seen.iter().any(is_show),
        format!("{seen:?}"),
    );
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(400));
    rig.check(
        "tab in a password field inserts nothing",
        inserted(&seen).is_empty(),
        format!("{seen:?}"),
    );
    rig.use_field(rig.a);
}

fn caret_moves(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, _)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("caret setup", false, "no suggestion".into());
    };
    let length = text_length(field.edit);
    select(field.edit, length - 4);
    let seen = rig.settle(Duration::from_millis(1000));
    rig.check(
        "caret moved into the text hides the ghost",
        seen.iter().any(is_hide) && !overlay_visible() && !seen.iter().any(is_show),
        format!("{seen:?}"),
    );
    select_end(field.edit);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "caret back at the end suggests again",
        seen.iter().any(is_request) && last_show(&seen).is_some(),
        format!(
            "{} requests, shown {:?}",
            requests(&seen).len(),
            last_show(&seen)
        ),
    );
    let rect = window_rect(HWND(field.host as _));
    move_host(field, rect.left + 40, rect.top + 20);
    let seen = rig.settle(Duration::from_millis(900));
    let moved_caret = caret_screen(field.edit, text_length(field.edit));
    let ghost = overlay_window().map(window_rect);
    move_host(field, rect.left, rect.top);
    let settled = rig.settle(Duration::from_millis(600));
    rig.check(
        "ghost follows the field when its window moves",
        ghost.is_some_and(|g| (g.left - moved_caret.0).abs() <= 8) && !seen.iter().any(is_request),
        format!(
            "caret {moved_caret:?}, ghost {:?}, back {:?}",
            ghost.map(|g| (g.left, g.top)),
            last_show(&settled)
        ),
    );
}

fn pause_and_resume(rig: &mut Rig) {
    let field = rig.a;
    let Some(_) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("pause setup", false, "no suggestion".into());
    };
    rig.set_enabled(false);
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "pause hides the ghost",
        seen.iter().any(is_hide) && !overlay_visible(),
        format!("{seen:?}"),
    );
    rig.type_slowly(field.edit, " and so", 30);
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(1200));
    rig.check(
        "paused: typing requests nothing and tab inserts nothing",
        !seen.iter().any(is_request) && !seen.iter().any(is_show) && inserted(&seen).is_empty(),
        format!("{seen:?}"),
    );
    rig.set_enabled(true);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "resume asks the model again on the unchanged field",
        seen.iter().any(is_request),
        format!(
            "{} requests, shown {:?}",
            requests(&seen).len(),
            last_show(&seen)
        ),
    );
    rig.probe.set_enabled(false);
    let paused = rig.read(field);
    rig.probe.set_enabled(true);
    let resumed = rig.read(field);
    rig.check(
        "a paused reader reads nothing and a resumed one reads again",
        paused.is_none() && resumed.is_some(),
        format!(
            "paused {paused:?}, resumed {:?}",
            resumed.map(|s| s.before.len())
        ),
    );
}

fn switching_fields(rig: &mut Rig) {
    let (a, b) = (rig.a, rig.b);
    let text = PROSE[2];
    rig.fresh(b, text);
    rig.fresh(a, "");
    rig.type_slowly(a.edit, text, 10);
    let Some(suggestion) = rig.wait_show(Duration::from_secs(5)) else {
        return rig.check("switch setup", false, "no suggestion".into());
    };
    rig.drain();
    rig.use_field(b);
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(900));
    rig.check(
        "switching to a field with identical text never pastes the old suggestion",
        inserted(&seen).is_empty() && get_text(b.edit) == text && get_text(a.edit) == text,
        format!("suggestion {suggestion:?}, actions {seen:?}"),
    );
    rig.check(
        "switching to a field with identical text asks the model for that field",
        seen.iter().any(is_hide) || seen.iter().any(is_request),
        format!("{seen:?}"),
    );
    rig.drain();
    rig.use_field(a);
    let seen = rig.settle(Duration::from_millis(1500));
    rig.check(
        "switching back suggests again",
        seen.iter().any(is_request) || seen.iter().any(is_show),
        format!("{} actions", seen.len()),
    );
}

fn overlay_geometry(rig: &mut Rig) {
    let field = rig.a;
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("overlay setup", false, "no suggestion".into());
    };
    std::thread::sleep(Duration::from_millis(200));
    let Some(ghost) = overlay_window()
        .map(window_rect)
        .filter(|_| overlay_visible())
    else {
        return rig.check("overlay is visible", false, "no overlay".into());
    };
    let (caret_x, caret_y) = caret_screen(field.edit, text_length(field.edit));
    rig.check(
        "ghost starts right of the caret on the same line",
        ghost.left >= caret_x - 2
            && ghost.left <= caret_x + 12
            && ghost.top <= caret_y + 24
            && ghost.bottom >= caret_y,
        format!(
            "caret ({caret_x},{caret_y}), ghost {:?}, suggestion {suggestion:?}",
            (ghost.left, ghost.top, ghost.right, ghost.bottom)
        ),
    );
    let ghost_monitor = overlay_window().map(monitor_of);
    let field_monitor = Some(monitor_of(HWND(field.edit as _)));
    rig.check(
        "ghost is on the same monitor as the field",
        ghost_monitor == field_monitor && ghost_monitor.is_some_and(|m| !m.is_invalid()),
        format!("{ghost_monitor:?} vs {field_monitor:?}"),
    );
    rig.key(DISMISS);
    rig.settle(Duration::from_millis(300));
    rig.check("ghost hides on escape", !overlay_visible(), String::new());
}

fn overlay_at_screen_edge(rig: &mut Rig) {
    let field = rig.a;
    let monitor = monitor_rect(monitor_of(HWND(field.host as _)));
    let rect = window_rect(HWND(field.host as _));
    let Some((_, suggestion)) = rig.suggestion_for(field, &PROSE, 1) else {
        return rig.check("edge setup", false, "no suggestion".into());
    };
    let (caret_x, _) = caret_screen(field.edit, text_length(field.edit));
    let margin = 24;
    move_host(
        field,
        rect.left + monitor.right - margin - caret_x,
        rect.top,
    );
    std::thread::sleep(Duration::from_millis(500));
    let ghost = overlay_window().map(window_rect);
    let (caret_x, caret_y) = caret_screen(field.edit, text_length(field.edit));
    let corner_monitor =
        |x: i32, y: i32| unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL) };
    let overflow = ghost.map(|g| g.right - monitor.right);
    rig.check(
        "ghost near the screen edge starts at the caret on the caret's monitor",
        ghost.is_some_and(|g| {
            g.left >= caret_x - 2
                && g.left <= caret_x + 12
                && corner_monitor(g.left, g.top) == corner_monitor(caret_x, caret_y)
        }),
        format!(
            "caret ({caret_x},{caret_y}), ghost {:?}, monitor right {}, suggestion {suggestion:?}",
            ghost.map(|g| (g.left, g.right)),
            monitor.right
        ),
    );
    println!("[INFO] ghost overflow past the monitor edge: {overflow:?} px for {suggestion:?}");
    move_host(field, rect.left, rect.top);
}

fn idle_unload(rig: &mut Rig, unload_after: Duration) {
    let field = rig.a;
    rig.fresh(field, "");
    print_memory("loaded, after typing");
    let unloaded = loop {
        match rig
            .statuses
            .recv_timeout(unload_after + Duration::from_secs(8))
        {
            Ok(status) if status.contains("unloaded") => break true,
            Ok(_) => {}
            Err(_) => break false,
        }
    };
    rig.check(
        "model unloads after the idle period",
        unloaded,
        String::new(),
    );
    std::thread::sleep(Duration::from_millis(800));
    print_memory("after idle unload");
    rig.type_slowly(field.edit, PROSE[2], 10);
    let started = Instant::now();
    let shown = rig.wait_show(Duration::from_secs(15));
    rig.check(
        "suggestion returns after the model was unloaded",
        shown.is_some(),
        format!("{shown:?} after {:?}", started.elapsed()),
    );
    print_memory("after reload");
    let text = get_text(field.edit);
    rig.key(ACCEPT_ALL);
    let seen = rig.settle(Duration::from_millis(500));
    rig.check(
        "accept works right after a reload",
        shown
            .as_deref()
            .is_some_and(|s| get_text(field.edit) == format!("{text}{s}")),
        format!("{:?}", inserted(&seen)),
    );
}

fn control_text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 1024];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..length])
}

fn shown(hwnd: HWND) -> bool {
    unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0 }
}

fn onboarding_flow(marker: PathBuf) -> usize {
    let mut report = Report {
        passed: 0,
        failed: 0,
    };
    let mut window = Onboarding::create(marker.clone(), false).expect("onboarding window");
    let controls = window.controls();
    report.check(
        "onboarding starts quiet",
        control_text(controls.status) == "Starting Pith"
            && !shown(controls.progress)
            && !shown(controls.practice)
            && !unsafe { IsWindowEnabled(controls.done).as_bool() },
        control_text(controls.status),
    );
    window.event(OnboardingEvent::Progress(150, 400));
    let position = unsafe { SendMessageW(controls.progress, PBM_GETPOS, None, None) }.0;
    report.check(
        "onboarding shows download progress",
        shown(controls.progress)
            && position == 375
            && control_text(controls.status).contains("MB of"),
        format!("{} at {position}", control_text(controls.status)),
    );
    window.event(OnboardingEvent::Progress(400, 400));
    report.check(
        "onboarding reports loading",
        *window.step() == OnboardingStep::Loading
            && control_text(controls.status) == "Loading the model"
            && !shown(controls.progress),
        control_text(controls.status),
    );
    window.event(OnboardingEvent::Ready);
    let seeded = get_text(controls.practice.0 as isize);
    report.check(
        "onboarding opens the practice box with a sentence start",
        control_text(controls.status) == "Ready"
            && shown(controls.practice)
            && unsafe { IsWindowEnabled(controls.practice).as_bool() }
            && !seeded.is_empty()
            && !seeded.ends_with(char::is_whitespace),
        seeded.clone(),
    );
    let keys = control_text(controls.keys);
    report.check(
        "onboarding lists the keys",
        ["Tab", "Shift+Tab", "Esc"]
            .iter()
            .all(|key| keys.contains(key)),
        keys,
    );
    let style = unsafe { GetWindowLongPtrW(controls.practice, GWL_STYLE) } as u32;
    report.check(
        "practice box is multi-line",
        style & ES_MULTILINE != 0,
        format!("{style:#x}"),
    );
    report.check(
        "polling before Done stays pending without a marker",
        window.poll() == Outcome::Pending && !marker.exists(),
        format!("{:?}", window.step()),
    );
    unsafe {
        SendMessageW(controls.done, BM_CLICK, None, None);
    }
    let outcome = window.poll();
    report.check(
        "done writes the marker and finishes",
        outcome == Outcome::Finished && marker.exists() && !shown(controls.window),
        format!("{outcome:?}"),
    );
    drop(window);
    report.failed
}

fn onboarding_failure() -> usize {
    let mut report = Report {
        passed: 0,
        failed: 0,
    };
    let marker = std::env::temp_dir().join("pith-e2e-onboarding-failed");
    let mut window = Onboarding::create(marker.clone(), false).expect("onboarding window");
    let controls = window.controls();
    window.event(OnboardingEvent::Failed("offline & retry".into()));
    report.check(
        "onboarding shows a startup failure",
        control_text(controls.status).contains("offline & retry")
            && !shown(controls.practice)
            && unsafe { IsWindowEnabled(controls.done).as_bool() },
        control_text(controls.status),
    );
    unsafe {
        SendMessageW(controls.done, BM_CLICK, None, None);
    }
    report.check(
        "closing after failure leaves no marker",
        window.poll() == Outcome::Closed && !marker.exists(),
        format!("{:?}", window.step()),
    );
    report.failed
}

fn onboarding_checks() -> usize {
    let marker = std::env::temp_dir().join("pith-e2e-onboarded");
    let _ = std::fs::remove_file(&marker);
    let flow = marker.clone();
    let failed = std::thread::spawn(move || onboarding_flow(flow) + onboarding_failure())
        .join()
        .unwrap_or(1);
    let _ = std::fs::remove_file(&marker);
    failed
}

fn main() {
    let mut args = std::env::args().skip(1);
    let model = PathBuf::from(args.next().expect("model path"));
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let only = std::env::var("PITH_E2E_ONLY").ok();
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
    TARGET.store(a.edit, Ordering::SeqCst);
    let config = Config::default();
    let (sender, receiver) = channel();
    let waker = Waker::default();
    let (ready_tx, ready_rx) = channel();
    let (status_tx, statuses) = channel();
    let (done_sender, done_waker) = (sender.clone(), waker.clone());
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
        Box::new(move || {
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
                    "[DONE] g{generation} {:?} logprob {:.2} {:?} eval {}",
                    c.text, c.mean_logprob, c.stop, c.prompt_tokens_evaluated
                );
            }
            recorded.lock().unwrap().push(result.clone());
            let _ = done_sender.send(Inbound::Done(generation, result));
            done_waker.wake();
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
    let driver_inbound = sender.clone();
    let driver_waker = waker.clone();
    let driver = std::thread::spawn(move || {
        let _ = ready_rx.recv_timeout(Duration::from_secs(60));
        print_memory("loaded, no request yet");
        let mut rig = Rig {
            a,
            b,
            secret,
            locked,
            actions,
            inbound: driver_inbound,
            waker: driver_waker,
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
            latencies: Vec::new(),
        };
        let scenarios: [(&str, Scenario); 19] = [
            ("basic", basic_suggestions),
            ("accept", accept_word_then_rest),
            ("through", typing_through_and_escape),
            ("after", text_after_caret),
            ("multiline", multi_line),
            ("fidelity", reader_fidelity),
            ("scripts", scripts),
            ("prompt", prompt_matches_typed_text),
            ("readcost", read_cost),
            ("long", long_document),
            ("chain", rapid_accept_chain),
            ("stale", stale_keys),
            ("escape", escape_then_flicker),
            ("selection", selection_while_visible),
            ("password", password_field),
            ("caret", caret_moves),
            ("pause", pause_and_resume),
            ("switch", switching_fields),
            ("geometry", overlay_geometry),
        ];
        for (name, scenario) in scenarios {
            if only.as_deref().is_some_and(|o| !name.contains(o)) {
                continue;
            }
            println!("--- {name}");
            scenario(&mut rig);
        }
        if only.as_deref().is_none_or(|o| "edge".contains(o)) {
            println!("--- edge");
            overlay_at_screen_edge(&mut rig);
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
        let _ = rig.inbound.send(Inbound::Quit);
        rig.waker.wake();
        rig.report.failed
    });
    let options = Options {
        onboarding: None,
        source: Source::Window(&TARGET),
        insert: match insert_lag_ms {
            0 => InsertMode::Window(&TARGET),
            lag => InsertMode::Lagging(&TARGET, Duration::from_millis(lag)),
        },
        tray: false,
        poll_ms: 50,
        config_path: None,
        observer: Some(observer),
        global_keys: false,
    };
    run(config, worker, (sender, receiver), waker, options).expect("run");
    let failed = driver.join().unwrap_or(1);
    std::process::exit(if failed + onboarding_failed == 0 { 0 } else { 1 });
}
